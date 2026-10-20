use super::*;
use crate::pool::compaction::*;
use crate::session_controls::{AdmittedSessionControl, CompactionTarget};
use buzz_core::session_control::{ControlOrigin, SessionCommand};

fn job(
    ctx: &PromptContext,
    scope: &SessionScope,
    target: CompactionTarget,
) -> AdmittedSessionControl {
    let event = EventBuilder::new(Kind::Custom(9), "!compact")
        .sign_with_keys(&Keys::generate())
        .unwrap();
    AdmittedSessionControl {
        job: crate::session_controls::ControlJob {
            command: SessionCommand::Compact,
            origin: ControlOrigin {
                command_event_id: event.id.to_hex(),
                actor_pubkey: event.pubkey.to_hex(),
            },
            scope: scope.clone(),
            dispatch_epoch: Uuid::nil(),
            admitted_at: nostr::Timestamp::now().as_secs(),
            event,
        },
        dispatch: ctx.session_controls.capture(scope),
        target,
        deadline: tokio::time::Instant::now() + Duration::from_secs(120),
    }
}

async fn run_compact(
    status: &str,
    rpc_error: bool,
) -> (
    ControlTaskResult,
    Fixture,
    Arc<PromptContext>,
    std::path::PathBuf,
) {
    run_compact_observed(status, rpc_error, None).await
}

async fn run_compact_observed(
    status: &str,
    rpc_error: bool,
    observer: Option<crate::observer::ObserverHandle>,
) -> (
    ControlTaskResult,
    Fixture,
    Arc<PromptContext>,
    std::path::PathBuf,
) {
    let fixture = Fixture::new();
    let scope = scope();
    let mut ctx = ctx();
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    fixture.departed_record(&scope, &ctx, &["delivered"]);
    let target = CompactionTarget::Disk {
        lease: fixture.record(&scope).live.unwrap().lease,
        session_id: "old-1".into(),
    };
    let job = job(&ctx, &scope, target.clone());
    ctx.session_controls
        .queue_compaction(job.job.clone(), Ok(target))
        .unwrap();
    let admitted = ctx.session_controls.pending_compactions().pop().unwrap();
    let prompts = fixture.dir.path().join("compact-prompts.jsonl");
    let prompt_path = prompts.display().to_string();
    let root = fixture.dir.path().join("store").display().to_string();
    let mut env = vec![
        ("RESUME_CAP", "1"),
        ("COMPACT_STATUS", status),
        ("PROMPTS", prompt_path.as_str()),
        ("CHECK_STARTED_ROOT", root.as_str()),
        ("COMMAND_ID", admitted.job.origin.command_event_id.as_str()),
    ];
    if rpc_error {
        env.push(("FAIL_PROMPT", "1"));
    }
    let mut agent = fixture.agent(&env).await;
    agent.acp.set_observer(observer, 0);
    agent
        .acp
        .set_observer_context(crate::observer::ObserverContext {
            channel_id: Some("previous-channel".into()),
            session_id: Some("previous-session".into()),
            turn_id: Some("previous-turn".into()),
            started_at: None,
        });
    let (_tx, rx) = tokio::sync::oneshot::channel();
    let ctx = Arc::new(ctx);
    let result = run_compaction_task(agent, admitted, ctx.clone(), rx).await;
    (result, fixture, ctx, prompts)
}

#[tokio::test]
async fn compact_exact_prompt_and_durable_started_preserve_delivery_and_turns() {
    let (mut result, fixture, ctx, prompts) = run_compact("completed", false).await;
    assert_eq!(result.outcome, Ok(()));
    assert_eq!(result.agent.state.turn_counts[&scope()], 1);
    assert!(result.agent.state.deliveries[&scope()]
        .delivered_event_ids
        .contains("delivered"));
    let bodies: Vec<serde_json::Value> = std::fs::read_to_string(prompts)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        bodies,
        vec![serde_json::json!({"sessionId":"old-1","prompt":[{"type":"text","text":"/compact"}]})]
    );
    assert!(!fixture.methods().contains(&"session/new".into()));
    ctx.session_controls
        .finish_compaction_job(&result.job, result.outcome);
    let receipt = fixture.record(&scope()).control_receipts.pop().unwrap();
    assert_eq!(
        receipt.state,
        crate::session_store::ControlReceiptState::Finished(Ok(()))
    );
    result.agent.acp.shutdown().await;
}

#[tokio::test]
async fn prompt_end_without_terminal_is_unconfirmed() {
    let (mut result, fixture, ctx, _) = run_compact("missing", false).await;
    assert_eq!(
        result.outcome,
        Err(buzz_core::session_control::ControlFailure::Unconfirmed)
    );
    ctx.session_controls
        .finish_compaction_job(&result.job, result.outcome);
    assert!(fixture.record(&scope()).live.is_some());
    result.agent.acp.shutdown().await;
}

#[tokio::test]
async fn failed_terminal_with_end_turn_is_failure() {
    let (mut result, fixture, ctx, _) = run_compact("failed", false).await;
    assert_eq!(
        result.outcome,
        Err(buzz_core::session_control::ControlFailure::CompactFailed)
    );
    ctx.session_controls
        .finish_compaction_job(&result.job, result.outcome);
    assert!(fixture.record(&scope()).live.is_some());
    result.agent.acp.shutdown().await;
}

#[tokio::test]
async fn completed_terminal_with_rpc_error_is_unconfirmed_without_retry() {
    let (mut result, fixture, ctx, _) = run_compact("completed", true).await;
    assert_eq!(
        result.outcome,
        Err(buzz_core::session_control::ControlFailure::Unconfirmed)
    );
    ctx.session_controls
        .finish_compaction_job(&result.job, result.outcome);
    assert_eq!(
        fixture
            .methods()
            .iter()
            .filter(|method| *method == "session/prompt")
            .count(),
        1
    );
    result.agent.acp.shutdown().await;
}

#[tokio::test]
async fn compact_actual_signed_result_submission_matches_exact_command_and_safe_reason() {
    use tokio::io::AsyncWriteExt;
    for (status, expected) in [
        ("completed", Ok(())),
        (
            "failed",
            Err(buzz_core::session_control::ControlFailure::CompactFailed),
        ),
    ] {
        let (mut result, _fixture, ctx, _) = run_compact(status, false).await;
        ctx.session_controls
            .finish_compaction_job(&result.job, result.outcome);
        let publication = ctx
            .session_controls
            .take_publications()
            .pop_front()
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut rest = ctx.rest_client.clone();
        rest.base_url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let server = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let data = read_http_request(&mut socket).await;
                let n = data.len();
                if let Some(offset) = data[..n].windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    if let Ok(event) = serde_json::from_slice::<nostr::Event>(&data[offset + 4..n])
                    {
                        tx.send(event).unwrap();
                    }
                }
                let body = "{\"accepted\":true}";
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
        });
        crate::session_controls::publish::publish_result(&rest, &publication)
            .await
            .unwrap();
        let event = rx.recv().await.unwrap();
        event.verify().unwrap();
        assert_eq!(event.kind.as_u16(), 7);
        assert_eq!(event.pubkey, ctx.agent_keys.public_key());
        let tags: Vec<_> = event
            .tags
            .iter()
            .map(|tag| tag.as_slice().to_vec())
            .collect();
        let parsed =
            buzz_core::session_control::SessionResult::from_tags(&event.content, &tags).unwrap();
        assert_eq!(parsed.command_id, result.job.job.origin.command_event_id);
        assert_eq!(
            parsed.result,
            buzz_core::session_control::ControlResult::Terminal(expected)
        );
        result.agent.acp.shutdown().await;
        server.abort();
    }
}

#[tokio::test]
async fn compact_finished_receipt_prevents_redelivery_after_controller_restart() {
    let (mut result, fixture, ctx, _) = run_compact("completed", false).await;
    ctx.session_controls
        .finish_compaction_job(&result.job, result.outcome);
    let restarted =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    restarted
        .queue_compaction(
            result.job.job.clone(),
            Err(buzz_core::session_control::ControlFailure::NoSession),
        )
        .unwrap();
    assert!(restarted.pending_compactions().is_empty());
    assert_eq!(
        restarted.take_publications().pop_front().unwrap().result,
        buzz_core::session_control::ControlResult::Terminal(Ok(()))
    );
    assert_eq!(
        fixture
            .methods()
            .iter()
            .filter(|method| *method == "session/prompt")
            .count(),
        1
    );
    result.agent.acp.shutdown().await;
}

#[tokio::test]
async fn compact_publication_failure_is_observable_and_redelivery_never_reexecutes() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (mut result, fixture, ctx, _) = run_compact("completed", false).await;
    ctx.session_controls
        .finish_compaction_job(&result.job, result.outcome);
    let publication = ctx
        .session_controls
        .take_publications()
        .pop_front()
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut rest = ctx.rest_client.clone();
    rest.base_url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut data = [0; 8192];
            let _ = socket.read(&mut data).await.unwrap();
            socket.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
        }
    });
    assert!(
        crate::session_controls::publish::publish_result(&rest, &publication)
            .await
            .is_err()
    );
    ctx.session_controls
        .queue_compaction(
            result.job.job.clone(),
            Err(buzz_core::session_control::ControlFailure::NoSession),
        )
        .unwrap();
    assert!(ctx.session_controls.pending_compactions().is_empty());
    assert_eq!(
        fixture
            .methods()
            .iter()
            .filter(|method| *method == "session/prompt")
            .count(),
        1
    );
    assert_eq!(
        ctx.session_controls
            .take_publications()
            .pop_front()
            .unwrap()
            .result,
        buzz_core::session_control::ControlResult::Terminal(Ok(()))
    );
    result.agent.acp.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn compact_never_calls_session_new_memory_or_disk_success() {
    for memory in [true, false] {
        let fixture = Fixture::new();
        let scope = scope();
        let mut ctx = ctx();
        ctx.session_controls =
            crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
        fixture.departed_record(&scope, &ctx, &["delivered"]);
        let mut agent = fixture.agent(&[("RESUME_CAP", "1")]).await;
        let target = if memory {
            agent.state.store = None;
            agent.state.sessions.insert(scope.clone(), "old-1".into());
            agent.state.turn_counts.insert(scope.clone(), 7);
            agent.state.set_scope_owner_generation(scope.clone(), 5);
            CompactionTarget::Memory {
                worker: 0,
                generation: 5,
                session_id: "old-1".into(),
            }
        } else {
            CompactionTarget::Disk {
                lease: fixture.record(&scope).live.unwrap().lease,
                session_id: "old-1".into(),
            }
        };
        let job = job(&ctx, &scope, target);
        let existing = acquire_existing_session_for_compaction(&mut agent, &job, &ctx).await;
        assert!(existing.is_ok());
        assert_eq!(agent.state.sessions[&scope], "old-1");
        assert_eq!(agent.state.turn_counts[&scope], if memory { 7 } else { 1 });
        assert!(!fixture
            .methods()
            .iter()
            .any(|method| method == "session/new" || method == "session/prompt"));
        if !memory {
            assert!(agent.state.deliveries[&scope]
                .delivered_event_ids
                .contains("delivered"));
        }
        agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn compact_never_calls_session_new_failure_matrix() {
    for mode in [
        "missing",
        "corrupt",
        "poison",
        "conflict",
        "config",
        "unsupported",
        "resume_error",
        "live_elsewhere",
        "timeout",
    ] {
        let fixture = Fixture::new();
        let scope = scope();
        let mut ctx = ctx();
        ctx.session_controls =
            crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
        if mode != "missing" {
            fixture.departed_record(&scope, &ctx, &[]);
        }
        let lease = if mode == "missing" {
            Uuid::new_v4()
        } else {
            fixture.record(&scope).live.unwrap().lease
        };
        let mut env = vec![("RESUME_CAP", "1")];
        if mode == "unsupported" {
            env.clear();
        }
        if mode == "resume_error" {
            env.push(("RESUME_ERROR", "1"));
        }
        if mode == "timeout" {
            env.push(("DELAY", "0.1"));
            ctx.max_turn_duration = Duration::from_millis(20);
        }
        let mut agent = fixture.agent(&env).await;
        if mode == "config" {
            agent.desired_model = Some("changed".into());
        }
        if mode == "live_elsewhere" {
            fixture
                .store
                .commit(&scope, lease, |live| {
                    live.departed = None;
                    live.worker_index = 7;
                })
                .unwrap();
        }
        if mode == "corrupt" {
            let path = std::fs::read_dir(fixture.store.channel_dir(scope.channel_id()))
                .unwrap()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .find(|path| {
                    path.extension()
                        .is_some_and(|extension| extension == "json")
                })
                .unwrap();
            std::fs::write(path, "corrupt").unwrap();
        }
        if mode == "poison" {
            fixture.store.fail_writes(true);
            assert!(fixture.store.commit(&scope, lease, |_| {}).is_err());
            fixture.store.fail_writes(false);
            assert!(fixture.store.is_poisoned(&scope));
        }
        if mode == "conflict" {
            let current = fixture.store.load(&scope);
            let live = fixture.record(&scope).live.unwrap();
            fixture
                .store
                .claim(&scope, &Expect::from_outcome(&current), live)
                .unwrap();
        }
        let job = job(
            &ctx,
            &scope,
            CompactionTarget::Disk {
                lease,
                session_id: "old-1".into(),
            },
        );
        assert!(
            acquire_existing_session_for_compaction(&mut agent, &job, &ctx)
                .await
                .is_err(),
            "{mode}"
        );
        assert!(
            !fixture
                .methods()
                .iter()
                .any(|method| method == "session/new" || method == "session/prompt"),
            "{mode}"
        );
        agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn compact_unknown_runtime_without_advertised_command_is_unsupported() {
    let (mut result, fixture, _ctx, _) = run_compact("", false).await;
    assert_eq!(
        result.outcome,
        Err(buzz_core::session_control::ControlFailure::Unsupported)
    );
    assert!(!fixture.methods().contains(&"session/prompt".into()));
    result.agent.acp.shutdown().await;
}

#[tokio::test]
async fn compact_started_write_failure_sends_no_prompt_and_releases_tracker() {
    let fixture = Fixture::new();
    let scope = scope();
    let mut ctx = ctx();
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let (state, _) = owned_state(&fixture, &scope, "old-1");
    let mut agent = fixture.agent(&[]).await;
    agent.state = state;
    agent.state.set_scope_owner_generation(scope.clone(), 1);
    agent.acp.ensure_compaction_route("old-1");
    agent.acp.advertise_compaction("old-1");
    let target = CompactionTarget::Memory {
        worker: 0,
        generation: 1,
        session_id: "old-1".into(),
    };
    let admitted = job(&ctx, &scope, target.clone());
    ctx.session_controls
        .queue_compaction(admitted.job, Ok(target))
        .unwrap();
    let admitted = ctx.session_controls.pending_compactions().pop().unwrap();
    fixture.store.fail_writes(true);
    let (_tx, rx) = tokio::sync::oneshot::channel();
    let mut result = run_compaction_task(agent, admitted, Arc::new(ctx), rx).await;
    assert_eq!(
        result.outcome,
        Err(buzz_core::session_control::ControlFailure::StoreUnavailable)
    );
    assert!(!fixture.methods().contains(&"session/prompt".into()));
    assert_eq!(
        result
            .agent
            .acp
            .begin_compaction("old-1", result.job.job.origin.clone()),
        Ok(())
    );
    result.agent.acp.finish_compaction("old-1").ok();
    result.agent.acp.shutdown().await;
}

#[tokio::test]
async fn compact_terminal_write_retries_after_repair_without_reexecuting_effect() {
    let (mut result, fixture, ctx, _) = run_compact("completed", false).await;
    let mut reconciliation = crate::session_controls::OrphanReconciliation::default();
    reconciliation.drive(&ctx.session_controls, 128);
    reconciliation.wake().await;
    reconciliation.drive(&ctx.session_controls, 128);
    fixture.store.fail_writes(true);
    ctx.session_controls
        .finish_compaction_job(&result.job, result.outcome);
    assert_eq!(
        fixture.record(&scope()).control_receipts[0].state,
        crate::session_store::ControlReceiptState::Started
    );
    assert_eq!(ctx.session_controls.receipt_write_count(), 1);
    reconciliation.drive(&ctx.session_controls, 128);
    tokio::time::timeout(Duration::from_secs(3), reconciliation.wake())
        .await
        .expect("new local receipt work must wake completed reconciliation");
    reconciliation.drive(&ctx.session_controls, 128);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), reconciliation.wake())
            .await
            .is_err(),
        "failed receipt writes use the shared backoff rather than spinning"
    );
    fixture.store.fail_writes(false);
    let repaired = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            reconciliation.drive(&ctx.session_controls, 128);
            tokio::select! { _ = reconciliation.wake() => {}, _ = tokio::time::sleep(Duration::from_millis(10)) => {} }
            if fixture.record(&scope()).control_receipts[0].state != crate::session_store::ControlReceiptState::Started { break; }
        }
    }).await;
    result.agent.acp.shutdown().await;
    assert!(
        repaired.is_ok(),
        "known-finalized local Started must repair after persistence recovers"
    );
    assert_eq!(
        fixture.record(&scope()).control_receipts[0].state,
        crate::session_store::ControlReceiptState::Finished(Err(
            buzz_core::session_control::ControlFailure::StoreUnavailable
        ))
    );
    ctx.session_controls
        .queue_compaction(
            result.job.job.clone(),
            Err(buzz_core::session_control::ControlFailure::NoSession),
        )
        .unwrap();
    assert!(ctx.session_controls.pending_compactions().is_empty());
    assert_eq!(ctx.session_controls.receipt_write_count(), 0);
    let publication = ctx
        .session_controls
        .take_publications()
        .pop_front()
        .unwrap();
    let signed = submit_one_result(&ctx, &publication).await;
    signed.verify().unwrap();
    assert_eq!(signed.kind.as_u16(), 7);
    assert_eq!(signed.content, "❌");
    let tags: Vec<_> = signed
        .tags
        .iter()
        .map(|tag| tag.as_slice().to_vec())
        .collect();
    let parsed =
        buzz_core::session_control::SessionResult::from_tags(&signed.content, &tags).unwrap();
    assert_eq!(parsed.command_id, result.job.job.origin.command_event_id);
    assert_eq!(
        parsed.result,
        buzz_core::session_control::ControlResult::Terminal(Err(
            buzz_core::session_control::ControlFailure::StoreUnavailable
        ))
    );
    assert_eq!(
        fixture
            .methods()
            .iter()
            .filter(|method| *method == "session/prompt")
            .count(),
        1
    );
}

async fn submit_one_result(
    ctx: &PromptContext,
    publication: &crate::session_controls::Publication,
) -> nostr::Event {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut rest = ctx.rest_client.clone();
    rest.base_url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut socket).await;
        let n = request.len();
        let offset = request[..n]
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap();
        let event = serde_json::from_slice::<nostr::Event>(&request[offset + 4..n]).unwrap();
        let body = "{\"accepted\":true}";
        socket
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        event
    });
    crate::session_controls::publish::publish_result(&rest, publication)
        .await
        .unwrap();
    server.await.unwrap()
}

async fn read_http_request(socket: &mut tokio::net::TcpStream) -> Vec<u8> {
    use tokio::io::AsyncReadExt;
    let mut request = Vec::new();
    loop {
        let mut chunk = [0; 4096];
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0, "HTTP request ended before its complete body");
        request.extend_from_slice(&chunk[..n]);
        assert!(request.len() <= 65536);
        if let Some(offset) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&request[..offset]).unwrap();
            let size = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if request.len() >= offset + 4 + size {
                return request;
            }
        }
    }
}

#[tokio::test]
async fn compact_finalized_started_survives_both_locked_finalization_calls() {
    finalized_started_survives_unavailable_store("locked").await;
}

#[tokio::test]
async fn compact_finalized_started_survives_unreadable_finalization_record() {
    finalized_started_survives_unavailable_store("unreadable").await;
}

async fn finalized_started_survives_unavailable_store(mode: &str) {
    use crate::session_store::{ControlReceiptState, STORE_LOCK_WAIT};
    use buzz_core::session_control::{ControlFailure, ControlResult, SessionResult};
    let (mut result, fixture, ctx, _) = run_compact("completed", false).await;
    let record = fixture.record(&scope());
    let expected = record.control_receipts[0].clone();
    assert_eq!(expected.state, ControlReceiptState::Started);
    let path = std::fs::read_dir(fixture.store.channel_dir(scope().channel_id()))
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "json"))
        .unwrap();
    let saved = std::fs::read(&path).unwrap();
    let lock_path = std::path::PathBuf::from(format!("{}.lock", path.display()));
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path)
        .unwrap();
    if mode == "locked" {
        fs2::FileExt::lock_exclusive(&lock).unwrap();
    } else {
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
    }
    let started = std::time::Instant::now();
    ctx.session_controls
        .finish_compaction_job(&result.job, result.outcome);
    let elapsed = started.elapsed();
    if mode == "locked" {
        fs2::FileExt::unlock(&lock).unwrap();
        assert!(
            elapsed >= STORE_LOCK_WAIT * 2,
            "both real record-lock attempts must fail: {elapsed:?}"
        );
    } else {
        if path.is_dir() {
            std::fs::remove_dir(&path).unwrap();
        }
        std::fs::write(&path, saved).unwrap();
    }
    assert_eq!(ctx.session_controls.receipt_write_count(), 1, "exact locally Started intent must survive failed finalization without a fresh successful read");
    let error = Err(ControlFailure::StoreUnavailable);
    let publication = ctx
        .session_controls
        .take_publications()
        .pop_front()
        .unwrap();
    assert_eq!(publication.result, ControlResult::Terminal(error));
    let mut reconciliation = crate::session_controls::OrphanReconciliation::default();
    fixture.store.fail_writes(true);
    reconciliation.drive(&ctx.session_controls, 128);
    reconciliation.wake().await;
    reconciliation.drive(&ctx.session_controls, 128);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), reconciliation.wake())
            .await
            .is_err(),
        "repair must retain shared backoff"
    );
    fixture.store.fail_writes(false);
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            reconciliation.drive(&ctx.session_controls, 128);
            if ctx.session_controls.receipt_write_count() == 0 { break; }
            tokio::select! { _ = reconciliation.wake() => {}, _ = tokio::time::sleep(Duration::from_millis(10)) => {} }
        }
    }).await.expect("known finalized receipt must repair after storage recovers");
    let repaired = fixture.record(&scope()).control_receipts[0].clone();
    assert_eq!(repaired.owner_instance, expected.owner_instance);
    assert_eq!(repaired.owner_generation, expected.owner_generation);
    assert_eq!(repaired.state, ControlReceiptState::Finished(error));
    ctx.session_controls
        .queue_compaction(result.job.job.clone(), Err(ControlFailure::NoSession))
        .unwrap();
    assert!(ctx.session_controls.pending_compactions().is_empty());
    let publication = ctx
        .session_controls
        .take_publications()
        .pop_front()
        .unwrap();
    let signed = submit_one_result(&ctx, &publication).await;
    signed.verify().unwrap();
    assert_eq!(signed.kind.as_u16(), 7);
    let tags: Vec<_> = signed
        .tags
        .iter()
        .map(|tag| tag.as_slice().to_vec())
        .collect();
    let parsed = SessionResult::from_tags(&signed.content, &tags).unwrap();
    assert_eq!(parsed.command_id, result.job.job.origin.command_event_id);
    assert_eq!(parsed.result, ControlResult::Terminal(error));
    assert_eq!(
        fixture
            .methods()
            .iter()
            .filter(|method| *method == "session/prompt")
            .count(),
        1
    );
    result.agent.acp.shutdown().await;
}

#[tokio::test]
async fn compact_observer_routes_resume_prompt_and_updates_to_admitted_scope() {
    let observer = crate::observer::ObserverHandle::in_process();
    let (mut result, _, _, _) =
        run_compact_observed("completed", false, Some(observer.clone())).await;
    assert_eq!(result.outcome, Ok(()));
    let events = observer.snapshot();
    for method in ["session/resume", "session/prompt", "session/update"] {
        let frames: Vec<_> = events
            .iter()
            .filter(|event| event.payload["method"] == method)
            .collect();
        assert!(!frames.is_empty(), "missing {method} observation");
        for frame in frames {
            assert_eq!(
                frame.channel_id.as_deref(),
                Some(scope().channel_id().to_string().as_str()),
                "{method} must route to admitted channel"
            );
            assert_eq!(frame.session_id.as_deref(), Some("old-1"));
            assert_eq!(
                frame.turn_id.as_deref(),
                Some(result.job.job.origin.command_event_id.as_str())
            );
            assert!(frame.started_at.is_some());
        }
    }
    result.agent.acp.shutdown().await;
}
