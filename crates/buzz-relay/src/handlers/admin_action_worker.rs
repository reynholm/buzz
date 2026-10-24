//! Action recovery worker for stranded `relay_admin_actions`.
//!
//! Scans for `relay_admin_actions` rows in `pending` or `enforcing` state whose
//! action lease has expired (or was never set), claims them via an exclusive lease
//! (`SELECT FOR UPDATE SKIP LOCKED`), and re-drives each through the enforcement
//! state machine via `drive_enforcement`.
//!
//! This is the crash-recovery path: if the request-handling process dies between
//! claim and finalization, this worker picks up the stranded action and resumes
//! from the persisted `step_marker` state without re-running the mutation.
//!
//! Multiple pod replicas can run this worker concurrently — the DB-level action
//! lease (`action_lease_token` / `action_lease_expires_at`) prevents double-mutation.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tracing::{error, info, warn};
use uuid::Uuid;

use buzz_db::relay_admin_actions::StrandedActionClaim;

use crate::state::AppState;

/// Lease duration for stranded-action claims. Long enough for the mutation to
/// complete; the HTTP driver uses 60 s, so we use 120 s for recovery.
const LEASE_SECS: i64 = 120;
/// Actions claimed per tick.
const BATCH_SIZE: i64 = 8;

/// Run the leased worker; register before spawn and join admitted batches on drain.
pub fn run(state: Arc<AppState>) -> impl std::future::Future<Output = ()> {
    let lifecycle = state.deployment_worker("admin-action");
    async move {
        let worker_id = format!("admin-action-{}", Uuid::new_v4());
        let mut idle_delay = Duration::from_secs(5);
        loop {
            let batch = lifecycle
                .batch(async {
                    let lease_until = Utc::now() + chrono::Duration::seconds(LEASE_SECS);
                    let rows = state
                        .db
                        .claim_stranded_admin_action_batch(&worker_id, lease_until, BATCH_SIZE)
                        .await
                        .map_err(|e| {
                            error!(worker_id = %worker_id, "Worker claim failed: {e}");
                        })?;
                    if rows.is_empty() {
                        let delay = idle_delay;
                        idle_delay = (idle_delay * 2).min(Duration::from_secs(60));
                        return Ok(delay);
                    }
                    idle_delay = Duration::from_secs(5);
                    let mut complete = true;
                    for row in rows {
                        complete &= recover_one(&state, row).await;
                    }
                    if complete {
                        Ok(Duration::ZERO)
                    } else {
                        Err(())
                    }
                })
                .await;
            let Some(result) = batch else { break };
            if lifecycle
                .idle(tokio::time::sleep(
                    result.unwrap_or(Duration::from_secs(10)),
                ))
                .await
                .is_none()
            {
                break;
            }
        }
        lifecycle.finish();
    }
}

/// Recover one stranded action from the batch claim.
/// Made pub(crate) for integration tests — allows tests to call through
/// the real production recovery path without the infinite worker loop.
pub(crate) async fn recover_one(state: &Arc<AppState>, claim: StrandedActionClaim) -> bool {
    let rec = &claim.record;
    let action_id = rec.id;

    // Resolve the community tenant for this action.
    let community_id = buzz_core::CommunityId::from_uuid(rec.report_community_id);
    let tenant = match state.db.lookup_community_host(community_id).await {
        Ok(Some(host)) => buzz_core::tenant::TenantContext::resolved(community_id, host),
        Ok(None) => {
            warn!(
                action_id = %action_id,
                "Action recovery: community not found, skipping"
            );
            return false;
        }
        Err(e) => {
            warn!(
                action_id = %action_id,
                "Action recovery: host lookup failed: {e}"
            );
            return false;
        }
    };

    info!(
        action_id = %action_id,
        report_id = ?rec.report_id,
        state = %rec.state,
        step_marker = ?rec.step_marker,
        "Action recovery worker re-driving stranded action"
    );

    let Some(report_id) = rec.report_id else {
        // Direct action: every target field was persisted at acceptance.
        return match crate::handlers::report_resolution::drive_direct_action(
            state,
            &tenant,
            rec,
            Some(claim.lease_token),
        )
        .await
        {
            Ok(_) => {
                info!(action_id = %action_id, "Action recovery worker: action converged");
                true
            }
            Err(e) => {
                warn!(action_id = %action_id, "Action recovery worker: re-drive failed: {e:?}");
                false
            }
        };
    };

    // Use the target context persisted at claim time for kick actions only.
    // Migration 0047 added enforcement_target_pubkey/enforcement_channel_id
    // for kicks; other actions (ban, timeout, delete) do not set these columns
    // and must re-derive from the report row on every recovery. Applying the
    // persisted-context branch to non-kick actions breaks delete recovery:
    // delete requires target_event_id (not NULL'd here for kick only) and any
    // persisted pubkey from a prior kick on the same report would force
    // target_event_id=None, causing pre-marker delete recovery to fail
    // ("delete requires target_event_id") or post-marker recovery to skip the
    // tombstone outbox row. Gate strictly on action=="kick".
    //
    // For pre-migration kick rows (both columns NULL), we still re-derive and
    // pass the result as function parameters; the convergence gate in
    // drive_enforcement accepts those as a legacy-context fallback so these
    // stranded kicks can finalize without the persisted columns being populated.
    let (target_pubkey_opt, target_event_id_opt, channel_id) = if rec.action == "kick"
        && (rec.enforcement_target_pubkey.is_some() || rec.enforcement_channel_id.is_some())
    {
        // Persisted kick context available — use it unconditionally.
        (
            rec.enforcement_target_pubkey.clone(),
            None::<Vec<u8>>,
            rec.enforcement_channel_id,
        )
    } else {
        // Non-kick action, or pre-migration kick row with both columns NULL.
        // Non-kick actions: always re-derive from the report.
        // Pre-migration kick rows: re-derive so the convergence gate can use the
        // result as a legacy-context fallback (see report_resolution.rs).
        let report = match state.db.admin_get_report_for_enforcement(report_id).await {
            Ok(Some(r)) => r,
            Ok(None) => {
                warn!(action_id = %action_id, "Action recovery: report not found");
                return false;
            }
            Err(e) => {
                warn!(action_id = %action_id, "Action recovery: report lookup failed: {e}");
                return false;
            }
        };
        let (pk, eid) =
            match crate::handlers::report_resolution::derive_enforcement_target_pub(&report) {
                Ok(pair) => pair,
                Err(e) => {
                    warn!(
                        action_id = %action_id,
                        "Action recovery: target derive failed: {e:?}"
                    );
                    return false;
                }
            };
        (pk, eid, report.report.channel_id)
    };

    let timeout_until = rec.timeout_until;
    let action = rec.action.clone();
    let reason = rec.reason.clone();
    let actor_pubkey = rec.actor_pubkey.clone();

    match crate::handlers::report_resolution::drive_enforcement_pub(
        state,
        &tenant,
        community_id,
        report_id,
        &action,
        reason.as_deref(),
        timeout_until,
        &actor_pubkey,
        target_pubkey_opt.as_deref(),
        target_event_id_opt.as_deref(),
        channel_id,
        rec,
        Some(claim.lease_token), // hold the batch-claim lease
    )
    .await
    {
        Ok(_) => {
            info!(action_id = %action_id, "Action recovery worker: action converged");
            true
        }
        Err(e) => {
            warn!(
                action_id = %action_id,
                "Action recovery worker: re-drive failed: {e:?}"
            );
            false
        }
    }
}
