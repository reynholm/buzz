//! Mutation-capable periodic relay work and deployment lifecycle ownership.
use crate::state::AppState;
use std::{sync::Arc, time::Duration};
use tracing::{error, info, warn};

/// Archive due ephemeral channels and finish their publications.
pub fn run_channel_reaper(
    state: Arc<AppState>,
    interval_secs: u64,
) -> impl std::future::Future<Output = ()> {
    let worker = state.deployment_worker("channel-reaper");
    async move {
        while worker
            .idle(tokio::time::sleep(Duration::from_secs(interval_secs)))
            .await
            .is_some()
        {
            if worker.batch(reap_channels(&state)).await.is_none() {
                break;
            }
        }
        worker.finish();
    }
}

async fn reap_channels(reaper_state: &Arc<AppState>) -> Result<(), ()> {
    let expired = match reaper_state.db.reap_expired_ephemeral_channels().await {
        Ok(ids) => ids,
        Err(e) => {
            error!("Ephemeral reaper tick failed: {e}");
            return Err(());
        }
    };

    if expired.is_empty() {
        return Ok(());
    }

    info!(count = expired.len(), "Ephemeral reaper archived channels");

    let mut complete = true;
    for channel in &expired {
        // Per-row tenant: the reaper crosses communities, so each
        // archived channel carries its own server-resolved
        // `(community, host)` from the DB RETURNING. Build the
        // `TenantContext` from that row — never a default tenant.
        let tenant =
            buzz_core::tenant::TenantContext::resolved(channel.community_id, channel.host.clone());
        let channel_id = channel.channel_id;
        // Emit a system message so members see why the channel was archived.
        if let Err(e) = crate::handlers::side_effects::emit_system_message(
            &tenant,
            reaper_state,
            channel_id,
            serde_json::json!({ "type": "channel_auto_archived" }),
            chrono::Utc::now(),
        )
        .await
        {
            error!(channel = %channel_id, "reaper system message failed: {e}");
            complete = false;
        }

        // Update NIP-29 discovery events so clients see the archived state.
        if let Err(e) = crate::handlers::side_effects::emit_group_discovery_events(
            &tenant,
            reaper_state,
            channel_id,
        )
        .await
        {
            error!(channel = %channel_id, "reaper discovery update failed: {e}");
            complete = false;
        }

        // Close live subscriptions so connected clients drop the
        // archived channel immediately (CLOSED is in the client's
        // drop-set → no reconnect storm). Offline clients are caught
        // by the archived=true skip in discover_channels on reconnect.
        crate::handlers::side_effects::evict_all_channel_subscriptions(
            &tenant,
            reaper_state,
            channel_id,
        )
        .await;
    }
    if complete {
        Ok(())
    } else {
        Err(())
    }
}

/// Deliver due reminders through the existing claim and publication path.
pub fn run_reminders(
    state: Arc<AppState>,
    interval_secs: u64,
    batch_limit: i64,
) -> impl std::future::Future<Output = ()> {
    let worker = state.deployment_worker("reminder-scheduler");
    async move {
        while worker
            .idle(tokio::time::sleep(Duration::from_secs(interval_secs)))
            .await
            .is_some()
        {
            if worker
                .batch(deliver_reminders(&state, batch_limit))
                .await
                .is_none()
            {
                break;
            }
        }
        worker.finish();
    }
}

async fn deliver_reminders(
    scheduler_state: &Arc<AppState>,
    scheduler_batch_limit: i64,
) -> Result<(), ()> {
    let now_secs = chrono::Utc::now().timestamp();
    let due = match scheduler_state
        .db
        .query_due_reminders(now_secs, scheduler_batch_limit)
        .await
    {
        Ok(reminders) => reminders,
        Err(e) => {
            error!("Reminder scheduler tick failed: {e}");
            return Err(());
        }
    };

    if due.is_empty() {
        return Ok(());
    }

    info!(count = due.len(), "Reminder scheduler: due reminders found");

    let mut complete = true;
    for reminder in due {
        let event = match reminder_to_event(&reminder) {
            Ok(event) => event,
            Err(error) => {
                warn!(%error, "Invalid stored reminder");
                complete = false;
                continue;
            }
        };
        // Claim before side effect (§5c: claim-before-publish). A
        // unique per-attempt stamp lets a failed publish roll back
        // exactly this pod's claim via compare-and-clear, without a
        // racing pod's later claim being clobbered. `delivered_at`
        // is only ever read as a NULL/non-NULL sentinel (the
        // due-reminder query guard and the partial index), never as
        // a wall-clock value, so an opaque stamp is safe to store.
        let reminder_tenant = buzz_core::tenant::TenantContext::resolved(
            reminder.community_id,
            reminder.host.clone(),
        );
        let delivery_stamp = chrono::Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| chrono::Utc::now().timestamp())
            ^ rand::random::<i64>();

        match scheduler_state
            .db
            .claim_due_reminder_with_stamp(
                reminder.community_id,
                &reminder.id,
                reminder.created_at,
                delivery_stamp,
            )
            .await
        {
            Ok(true) => {}         // We won the claim — proceed to publish.
            Ok(false) => continue, // Another pod claimed it; no side effect here.
            Err(e) => {
                warn!(
                    event_id = hex::encode(&reminder.id),
                    "Reminder scheduler: claim failed, skipping publish: {e}"
                );
                complete = false;
                continue;
            }
        }

        // Publish the single side effect. On failure, release our
        // claim so the next tick (this pod or another) can retry —
        // the stamp guard ensures we only clear our own claim.
        if let Err(e) = scheduler_state
            .pubsub
            .publish_event(&reminder_tenant, buzz_pubsub::EventTopic::Global, &event)
            .await
        {
            error!(
                event_id = hex::encode(&reminder.id),
                "Reminder scheduler: Redis publish failed after claim, releasing: {e}"
            );
            if let Err(release_err) = scheduler_state
                .db
                .release_due_reminder(
                    reminder.community_id,
                    &reminder.id,
                    reminder.created_at,
                    delivery_stamp,
                )
                .await
            {
                complete = false;
                warn!(
                    event_id = hex::encode(&reminder.id),
                    "Reminder scheduler: release after failed publish errored \
                                 (reminder stays claimed, will not retry): {release_err}"
                );
            }
        }
    }
    if complete {
        Ok(())
    } else {
        Err(())
    }
}

/// Fence the cron loop before its database scan; each claimed run retains its
/// independent workflow lease through durable finalization.
pub fn run_workflow_scheduler(state: Arc<AppState>) -> impl std::future::Future<Output = ()> {
    let worker = state.deployment_worker("workflow-scheduler");
    async move {
        while worker
            .idle(tokio::time::sleep(Duration::from_secs(60)))
            .await
            .is_some()
        {
            if worker
                .batch(async {
                    state.workflow_engine.run_scheduled_tick().await;
                    Ok::<_, ()>(())
                })
                .await
                .is_none()
            {
                break;
            }
        }
        worker.finish();
    }
}

/// Reconstruct a `nostr::Event` from a [`DueReminder`] row for Redis pub/sub.
fn reminder_to_event(
    reminder: &buzz_db::event::DueReminder,
) -> Result<nostr::Event, serde_json::Error> {
    let event_json = serde_json::json!({
        "id": hex::encode(&reminder.id),
        "pubkey": hex::encode(&reminder.pubkey),
        "created_at": reminder.created_at.timestamp(),
        "kind": reminder.kind as u16,
        "tags": reminder.tags,
        "content": reminder.content,
        "sig": hex::encode(&reminder.sig),
    });

    serde_json::from_value(event_json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deployment_startup_work_owns_late_descendants_and_rejects_late_start() {
        let pool = sqlx::PgPool::connect_lazy("postgres://test:test@127.0.0.1:1/test").unwrap();
        pool.close().await;
        let base = crate::state::tests::test_state_with_database_pool(pool).await;
        let mut state = (*base).clone();
        Arc::make_mut(&mut state.config).deploy_control_token =
            Some(crate::config::DeployControlToken::from_secret("a".repeat(64)).unwrap());
        let child = state
            .run_startup_work(async {
                state
                    .deploy_drain
                    .admit_current_child(crate::deploy_drain::WorkKind::Background)
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(state.deploy_drain.snapshot().background_inflight, 1);
        let op = uuid::Uuid::new_v4();
        state.deploy_drain.begin(op).unwrap();
        assert!(state
            .run_startup_work(async { panic!("must not poll after begin") })
            .await
            .is_err());
        child.complete();
        let snapshot = state.deploy_drain.snapshot();
        assert_eq!(snapshot.background_inflight, 0);
        assert!(!snapshot
            .blocking_workers
            .iter()
            .any(|name| name.contains("unresolved")));
    }

    #[tokio::test]
    async fn deployment_periodic_mutators_register_before_spawn_and_do_not_claim_after_begin() {
        let pool = sqlx::PgPool::connect_lazy("postgres://test:test@127.0.0.1:1/test").unwrap();
        pool.close().await;
        let base = crate::state::tests::test_state_with_database_pool(pool).await;
        let mut state = (*base).clone();
        Arc::make_mut(&mut state.config).deploy_control_token =
            Some(crate::config::DeployControlToken::from_secret("a".repeat(64)).unwrap());
        let state = Arc::new(state);
        let reaper = run_channel_reaper(state.clone(), 86400);
        let reminders = run_reminders(state.clone(), 86400, 100);
        let cron = run_workflow_scheduler(state.clone());
        let names = state.deploy_drain.snapshot().blocking_workers;
        for name in ["channel-reaper", "reminder-scheduler", "workflow-scheduler"] {
            assert!(names.iter().any(|entry| entry == name), "missing {name}");
        }
        let op = uuid::Uuid::new_v4();
        let boot = state.deploy_drain.begin(op).unwrap().boot_id;
        state
            .deploy_drain
            .fence_http(op, boot, "test-retirement")
            .unwrap();
        tokio::time::timeout(Duration::from_millis(100), async {
            tokio::join!(reaper, reminders, cron);
        })
        .await
        .expect("idle timers must not delay drain or run another claim");
        let names = state.deploy_drain.snapshot().blocking_workers;
        for name in [
            "channel-reaper",
            "reminder-scheduler",
            "workflow-scheduler",
            "unresolved-background",
        ] {
            assert!(
                !names.iter().any(|entry| entry == name),
                "unresolved {name}"
            );
        }
    }
}
