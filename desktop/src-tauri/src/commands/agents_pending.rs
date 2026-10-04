//! Retention-queue helpers for managed-agent lifecycle events: pending
//! upserts, NIP-09 tombstones, and NIP-IA archive requests. Split from
//! `agents.rs` (which mounts this as `mod pending`) purely along the
//! retention seam; every function runs inside the
//! `managed_agents_store_lock`-held body and NEVER across an `.await`.

use tauri::AppHandle;

use crate::{app_state::AppState, managed_agents::ManagedAgentRecord};

/// Retain a freshly authored managed-agent event in the local store, flagged
/// for relay sync. MUST be called inside the `managed_agents_store_lock`-held
/// body after `save_managed_agents`, NEVER across an `.await`: it acquires
/// `state.keys` and a retention-db connection, both `std::sync` guards, and
/// drops them before returning.
///
/// Owner-authored, mirroring `commands::personas::retain_persona_pending`: the
/// owner keys sign, the d_tag is the agent's pubkey, so the coordinate is
/// `30177:<owner>:<agent_pubkey>`. The event content is the opt-IN
/// [`agent_event_content`] projection — the retention upsert's content-equality
/// guard compares this projection, so an operational start/stop that mutates
/// only runtime fields produces an identical row and never re-enqueues a
/// publish. Best-effort: a failure here is logged and swallowed so a retention
/// hiccup never blocks the disk-authoritative write.
pub(crate) fn retain_managed_agent_pending<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    record: &ManagedAgentRecord,
) {
    let result = retain_managed_agent_pending_with(
        app,
        state,
        record,
        crate::managed_agents::persona_device_view::load_device_policy_context,
    );
    if let Err(e) = result {
        eprintln!("buzz-desktop: agent-retain: {e}");
    }
}

/// Shared native head adapter; authorization precedes owner signing and database writes.
pub(crate) fn retain_managed_agent_pending_with<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    record: &ManagedAgentRecord,
    context: impl FnOnce(
        &AppHandle<R>,
        &AppState,
    ) -> Result<
        crate::managed_agents::persona_device_view::DevicePolicyContext,
        String,
    >,
) -> Result<(), String> {
    crate::managed_agents::device_authority::instance_phase_locked_with(
        app,
        state,
        &record.pubkey,
        None,
        crate::managed_agents::device_authority::InstanceAuthorityAction::PublishHead,
        context,
        |current, _, _| {
            let scope = crate::managed_agents::retention::active_retention_scope(app, state)?;
            let conn = crate::managed_agents::retention::open_retention_db(&scope.db_path)?;
            crate::managed_agents::reconcile::retain_agent_record(
                &conn,
                &scope.owner_keys,
                &current,
            )
            .map(|_| ())
        },
    )
}

/// Finish a durable deletion's pre-signed batch. Failure propagates with its intent intact.
/// The opaque authority was captured before removal, retaining the original scope.
pub(crate) fn tombstone_managed_agent_pending<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    permit: &crate::managed_agents::device_authority::DeletionAuthority,
    operation: &crate::managed_agents::device_home_operations::HomeOperation,
) -> Result<(), String> {
    use crate::managed_agents::device_authority::{
        validate_deletion_authority, InstanceAuthorityAction,
    };
    validate_deletion_authority(state, permit, InstanceAuthorityAction::Tombstone)?;
    validate_deletion_authority(state, permit, InstanceAuthorityAction::Archive)?;
    if operation.relay_url != permit.scope().relay_url
        || operation.owner_pubkey != permit.scope().owner_pubkey
    {
        return Err("device_home_sync_stale_session".into());
    }
    crate::managed_agents::device_home_operations::delete::complete_home_delete_locked(
        app, operation,
    )
}

/// Scope-free core of [`tombstone_managed_agent_pending`], so the atomic
/// purge-and-enqueue and its future-dated-head domination can be asserted
/// directly against a retention database (mirrors
/// `personas::tombstone_persona_at`).
///
/// Enqueues TWO durable effects for the deleted agent in ONE transaction: the
/// NIP-09 kind:5 tombstone AND the NIP-IA kind:9035 archive request that stops
/// the identity appearing in member pickers. They were previously two
/// independent best-effort calls — a crash between them could tombstone the
/// 30177 head while leaving the identity live, with no boot path to reconstruct
/// the archive. The archive's `persona_id` payload is derived from the retained
/// 30177 head's content (where it lives as owner-signed historical alias data),
/// NOT the deleted record. Unlike personas/teams, managed agents are NOT
/// re-enqueued by the boot deletion sweep ([`crate::event_sync`]) — a retained
/// 30177 head with no local record is the normal cross-device state, so a crash
/// after the disk-authoritative record is removed but before this
/// tombstone+archive transaction commits leaves agent deletion-retry a
/// pre-existing gap owned by this direct delete path alone.
#[cfg(test)]
fn tombstone_managed_agent_at(
    db_path: &std::path::Path,
    keys: &nostr::Keys,
    agent_pubkey: &str,
) -> Result<(), String> {
    let mut conn = crate::managed_agents::retention::open_retention_db(db_path)?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let events = prepare_agent_delete_events(&tx, keys, agent_pubkey, None)?;
    enqueue_agent_delete_events(
        &tx,
        keys.public_key().to_hex().as_str(),
        agent_pubkey,
        &events,
    )?;
    tx.commit()
        .map_err(|e| format!("managed-agent deletion commit: {e}"))
}

/// Sign the existing tombstone/archive pair without starting a transaction.
/// Caller owns the store lock and keeps these exact events for crash recovery.
pub(crate) fn prepare_agent_delete_events(
    conn: &rusqlite::Connection,
    keys: &nostr::Keys,
    agent_pubkey: &str,
    persona_id: Option<&str>,
) -> Result<Vec<nostr::Event>, String> {
    use crate::managed_agents::{
        agent_events::build_agent_delete, persona_events::monotonic_created_at,
        retention::get_retained_event,
    };
    let owner = keys.public_key().to_hex();
    let prior = get_retained_event(conn, 30177, &owner, agent_pubkey)?;
    let tombstone = build_agent_delete(agent_pubkey, &owner)?
        .custom_created_at(monotonic_created_at(prior.as_ref().map(|r| r.created_at)))
        .sign_with_keys(keys)
        .map_err(|e| e.to_string())?;
    let alias = prior
        .as_ref()
        .and_then(|r| persona_id_from_head(&r.content));
    let archive = build_agent_archive_request(keys, agent_pubkey, persona_id.or(alias.as_deref()))?;
    Ok(vec![tombstone, archive])
}

/// Enqueue a pre-signed pair inside the caller's transaction; failures propagate.
pub(crate) fn enqueue_agent_delete_events(
    conn: &rusqlite::Connection,
    owner: &str,
    target: &str,
    events: &[nostr::Event],
) -> Result<(), String> {
    use crate::managed_agents::retention::{
        delete_retained_event, get_retained_event, retain_event, tombstone_retention_d_tag,
        RetainedEvent,
    };
    use nostr::JsonUtil;
    if events.len() != 2 {
        return Err("invalid delete event count".into());
    }
    for (event, kind) in events.iter().zip([5, 9035]) {
        event
            .verify()
            .map_err(|e| format!("delete signature: {e}"))?;
        if event.kind.as_u16() != kind || event.pubkey.to_hex() != owner {
            return Err("invalid delete signed scope".into());
        }
    }
    let coordinate = format!("30177:{owner}:{target}");
    if !events[0].tags.iter().any(|t| {
        t.as_slice().first().map(String::as_str) == Some("a")
            && t.as_slice().get(1) == Some(&coordinate)
    }) {
        return Err("invalid delete coordinate".into());
    }
    if !events[1].tags.iter().any(|t| {
        t.as_slice().first().map(String::as_str) == Some("p")
            && t.as_slice().get(1).map(String::as_str) == Some(target)
    }) {
        return Err("invalid archive target".into());
    }
    if get_retained_event(conn, 30177, owner, target)?
        .is_none_or(|r| r.created_at <= events[0].created_at.as_secs() as i64)
    {
        delete_retained_event(conn, 30177, owner, target)?;
    }
    for (event, kind, d_tag) in [
        (&events[0], 5, tombstone_retention_d_tag(30177, target)),
        (&events[1], 9035, target.to_string()),
    ] {
        retain_event(
            conn,
            &RetainedEvent {
                kind,
                pubkey: owner.into(),
                d_tag,
                content: event.content.clone(),
                created_at: event.created_at.as_secs() as i64,
                raw_event: event.as_json(),
                pending_sync: true,
            },
        )?;
    }
    Ok(())
}

/// Extract `persona_id` from a retained kind:30177 head's content projection.
/// Absent (definition-less agent) or unparseable content yields `None`, so the
/// archive request falls back to an empty payload — exactly what the record's
/// `None` persona_id produced before this was derived from the head.
fn persona_id_from_head(content: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(content)
        .ok()?
        .get("persona_id")?
        .as_str()
        .map(str::to_owned)
}

/// Build an owner-authenticated NIP-IA `kind:9035` archive request for a deleted agent.
/// Definition-linked agents carry the persona id in `content`, where it survives the
/// kind:30177 tombstone as owner-signed historical alias data. The request uses the
/// same builder as the GUI Archive action and the NIP-IA `retired` reason.
pub(crate) fn build_agent_archive_request(
    keys: &nostr::Keys,
    agent_pubkey: &str,
    persona_id: Option<&str>,
) -> Result<nostr::Event, String> {
    let auth_tag = if keys
        .public_key()
        .to_hex()
        .eq_ignore_ascii_case(agent_pubkey)
    {
        None
    } else {
        let agent = nostr::PublicKey::from_hex(agent_pubkey)
            .map_err(|e| format!("invalid agent pubkey: {e}"))?;
        let tag_json = buzz_sdk_pkg::nip_oa::compute_auth_tag(keys, &agent, "")
            .map_err(|e| format!("failed to build owner auth tag: {e}"))?;
        let parts: Vec<String> = serde_json::from_str(&tag_json)
            .map_err(|e| format!("failed to parse owner auth tag: {e}"))?;
        Some(
            <[String; 4]>::try_from(parts)
                .map_err(|_| "owner auth tag must have four elements".to_string())?,
        )
    };
    let content = persona_id
        .filter(|id| !id.trim().is_empty())
        .map(|id| serde_json::json!({ "persona_id": id }).to_string())
        .unwrap_or_default();
    crate::events::build_archive_identity_request(
        agent_pubkey,
        &content,
        Some("retired"),
        None,
        auth_tag.as_ref(),
    )?
    .sign_with_keys(keys)
    .map_err(|e| format!("failed to sign archive request: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_agents::retention::{
        get_pending_sync, get_retained_event, open_retention_db, retain_event, RetainedEvent,
    };
    use buzz_core_pkg::kind::KIND_MANAGED_AGENT;

    // A valid 32-byte x-only pubkey hex — the folded archive request derives an
    // owner auth tag, which parses `agent_pubkey`, so it must be well-formed.
    const AGENT_PUBKEY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    /// Seed a retained 30177 agent head dated `created_at` seconds since epoch.
    /// The tombstone helper reads only the head's `created_at`, so the content
    /// need not be a full agent projection.
    fn seed_agent_head(db_path: &std::path::Path, owner: &str, created_at: i64) {
        seed_agent_head_content(db_path, owner, created_at, r#"{"name":"Agent"}"#);
    }

    /// Like [`seed_agent_head`] but with explicit head `content`, so the
    /// archive-payload derivation from the head can be asserted.
    fn seed_agent_head_content(
        db_path: &std::path::Path,
        owner: &str,
        created_at: i64,
        content: &str,
    ) {
        let conn = open_retention_db(db_path).unwrap();
        retain_event(
            &conn,
            &RetainedEvent {
                kind: KIND_MANAGED_AGENT,
                pubkey: owner.to_string(),
                d_tag: AGENT_PUBKEY.to_string(),
                content: content.to_string(),
                created_at,
                raw_event: r#"{"id":"seed"}"#.to_string(),
                pending_sync: false,
            },
        )
        .unwrap();
    }

    #[test]
    fn agent_tombstone_created_at_strictly_dominates_a_future_dated_head() {
        // The retained 30177 head may be future-dated (retain_agent_record
        // bumps a same-second re-publish past the prior head). The relay only
        // soft-deletes coordinate versions with created_at <= the tombstone's,
        // and the flush loop never re-reads the (purged) head — so a kind:5
        // signed at wall-clock `now` would leave the agent live forever once
        // its local retry witness is gone.
        let dir = tempfile::tempdir().unwrap();
        let keys = nostr::Keys::generate();
        let owner = keys.public_key().to_hex();
        let db_path = dir.path().join("retention.sqlite3");

        let future = nostr::Timestamp::now().as_secs() as i64 + 86_400;
        seed_agent_head(&db_path, &owner, future);

        tombstone_managed_agent_at(&db_path, &keys, AGENT_PUBKEY).unwrap();

        let conn = open_retention_db(&db_path).unwrap();
        let tombstone = get_pending_sync(&conn)
            .unwrap()
            .into_iter()
            .find(|row| row.kind == 5)
            .expect("a kind:5 agent tombstone is enqueued");
        assert!(
            tombstone.created_at > future,
            "tombstone created_at ({}) must strictly dominate the future-dated head ({future})",
            tombstone.created_at
        );
        assert!(
            get_retained_event(&conn, KIND_MANAGED_AGENT, &owner, AGENT_PUBKEY)
                .unwrap()
                .is_none(),
            "the 30177 head is purged so no stale edit can republish it"
        );
    }

    #[test]
    fn agent_tombstone_rolls_back_head_purge_when_enqueue_fails() {
        // The head purge and kind:5 enqueue run in one `BEGIN IMMEDIATE`
        // transaction. A `BEFORE INSERT` trigger blocks the enqueue (which
        // follows the head DELETE); the whole transaction must roll back so the
        // 30177 head survives with its local retry witness intact.
        let dir = tempfile::tempdir().unwrap();
        let keys = nostr::Keys::generate();
        let owner = keys.public_key().to_hex();
        let db_path = dir.path().join("retention.sqlite3");

        let future = nostr::Timestamp::now().as_secs() as i64 + 86_400;
        seed_agent_head(&db_path, &owner, future);

        let conn = open_retention_db(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER block_all_inserts BEFORE INSERT ON persona_events
             BEGIN
                 SELECT RAISE(ABORT, 'insert blocked by test trigger');
             END;",
        )
        .unwrap();
        drop(conn);

        let err = tombstone_managed_agent_at(&db_path, &keys, AGENT_PUBKEY)
            .expect_err("tombstone with INSERT trigger must fail");
        assert!(
            err.contains("insert blocked by test trigger") || err.contains("blocked"),
            "error must name the trigger cause; got: {err}"
        );

        let conn = open_retention_db(&db_path).unwrap();
        assert!(
            get_retained_event(&conn, KIND_MANAGED_AGENT, &owner, AGENT_PUBKEY)
                .unwrap()
                .is_some(),
            "the 30177 head must survive when the tombstone enqueue fails"
        );
    }

    #[test]
    fn agent_tombstone_enqueues_archive_with_persona_id_from_head_atomically() {
        // FOLD-4: the kind:5 tombstone and the NIP-IA kind:9035 archive request
        // are enqueued in ONE transaction, and the archive's `persona_id`
        // payload is derived from the retained 30177 head's content (not the
        // already-deleted record). Both rows must be present and pending after
        // a successful tombstone.
        use buzz_core_pkg::kind::KIND_IA_ARCHIVE_REQUEST;

        let dir = tempfile::tempdir().unwrap();
        let keys = nostr::Keys::generate();
        let owner = keys.public_key().to_hex();
        let db_path = dir.path().join("retention.sqlite3");

        let now = nostr::Timestamp::now().as_secs() as i64;
        seed_agent_head_content(
            &db_path,
            &owner,
            now,
            r#"{"name":"Agent","persona_id":"persona-abc"}"#,
        );

        tombstone_managed_agent_at(&db_path, &keys, AGENT_PUBKEY).unwrap();

        let conn = open_retention_db(&db_path).unwrap();
        let pending = get_pending_sync(&conn).unwrap();
        assert!(
            pending.iter().any(|row| row.kind == 5),
            "a kind:5 tombstone is enqueued"
        );
        let archive = pending
            .iter()
            .find(|row| row.kind == KIND_IA_ARCHIVE_REQUEST)
            .expect("a kind:9035 archive request is enqueued in the same transaction");
        assert!(
            archive.content.contains("persona-abc"),
            "archive payload derives persona_id from the retained head; got: {}",
            archive.content
        );
    }

    #[test]
    fn agent_tombstone_rolls_back_kind5_when_archive_enqueue_fails() {
        // FOLD-4 atomicity: the kind:5 tombstone and kind:9035 archive share one
        // `BEGIN IMMEDIATE`. A trigger blocks ONLY the 9035 insert (which
        // follows the kind:5 insert); the whole transaction must roll back so
        // NEITHER the tombstone nor a purged head is left behind. Splitting the
        // two enqueues into separate transactions turns this RED — the kind:5
        // would commit and the head would be gone while the archive is lost.
        use buzz_core_pkg::kind::KIND_MANAGED_AGENT;

        let dir = tempfile::tempdir().unwrap();
        let keys = nostr::Keys::generate();
        let owner = keys.public_key().to_hex();
        let db_path = dir.path().join("retention.sqlite3");

        let now = nostr::Timestamp::now().as_secs() as i64;
        seed_agent_head(&db_path, &owner, now);

        let conn = open_retention_db(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER block_archive_insert BEFORE INSERT ON persona_events
             WHEN NEW.kind = 9035
             BEGIN
                 SELECT RAISE(ABORT, 'archive insert blocked by test trigger');
             END;",
        )
        .unwrap();
        drop(conn);

        let err = tombstone_managed_agent_at(&db_path, &keys, AGENT_PUBKEY)
            .expect_err("tombstone must fail when the archive enqueue is blocked");
        assert!(
            err.contains("archive insert blocked") || err.contains("blocked"),
            "error must name the trigger cause; got: {err}"
        );

        let conn = open_retention_db(&db_path).unwrap();
        assert!(
            get_retained_event(&conn, KIND_MANAGED_AGENT, &owner, AGENT_PUBKEY)
                .unwrap()
                .is_some(),
            "the 30177 head must survive — the whole transaction rolls back"
        );
        assert!(
            get_pending_sync(&conn)
                .unwrap()
                .iter()
                .all(|row| row.kind != 5),
            "no kind:5 tombstone may be committed when the archive enqueue fails"
        );
    }
}

#[cfg(test)]
mod device_authoring_tests {
    use super::*;
    use crate::managed_agents::{
        definition_home::EvidenceReadiness,
        device_home_migration::tests::{app, context, records, write},
        device_home_sync,
        retention::*,
    };
    use tauri::Manager;
    #[test]
    fn copied_common_head_and_deletion_preparation_have_zero_kind0_30177_5_9035_effects() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let (mut raw, _) = records();
        raw[1].device_host_binding = Some("foreign".into());
        let base = crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap();
        write(&base, &raw);
        let path = scoped_retention_db_path(
            &base,
            "wss://test",
            &state.signing_keys().unwrap().public_key().to_hex(),
        );
        let context_provider = |_: &AppHandle<tauri::test::MockRuntime>, state: &AppState| {
            let mut c = context(EvidenceReadiness::Ready);
            c.scope = device_home_sync::capture_scope(state)?;
            Ok(c)
        };
        assert!(
            retain_managed_agent_pending_with(app.handle(), &state, &raw[1], context_provider)
                .is_err()
        );
        assert!(
            crate::managed_agents::device_authority::prepare_deletion_authority_locked_with(
                app.handle(),
                &state,
                &raw[1].pubkey,
                context_provider
            )
            .is_err()
        );
        assert!(get_pending_sync(&open_retention_db(&path).unwrap())
            .unwrap()
            .is_empty());
    }
    #[test]
    fn prepared_shared_deletion_can_enqueue_after_removal_but_never_in_changed_scope() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let (mut raw, _) = records();
        raw[0].share_across_devices = Some(true);
        let base = crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap();
        write(&base, &raw);
        let permit =
            crate::managed_agents::device_authority::prepare_deletion_authority_locked_with(
                app.handle(),
                &state,
                &raw[1].pubkey,
                |_, _| panic!("shared proof read"),
            )
            .unwrap();
        let operation=crate::managed_agents::device_home_operations::delete::prepare_home_delete_authorized_locked(app.handle(),&permit).unwrap();
        let operation=crate::managed_agents::device_home_operations::delete::commit_home_delete_snapshot_locked(app.handle(),&operation).unwrap();
        raw.pop();
        tombstone_managed_agent_pending(app.handle(), &state, &permit, &operation).unwrap();
        let path = scoped_retention_db_path(
            &base,
            "wss://test",
            &state.signing_keys().unwrap().public_key().to_hex(),
        );
        let pending = get_pending_sync(&open_retention_db(&path).unwrap()).unwrap();
        assert_eq!(
            pending
                .iter()
                .map(|r| r.kind)
                .collect::<std::collections::HashSet<_>>(),
            [5, 9035].into_iter().collect()
        );
        *state.keys.lock().unwrap() = nostr::Keys::generate();
        *state.relay_url_override.lock().unwrap() = Some("wss://other".into());
        assert!(
            tombstone_managed_agent_pending(app.handle(), &state, &permit, &operation).is_err()
        );
        let path = scoped_retention_db_path(
            &base,
            "wss://other",
            &state.signing_keys().unwrap().public_key().to_hex(),
        );
        assert!(
            !path.exists(),
            "post-removal tombstone redirected into changed scope"
        );
    }
}
