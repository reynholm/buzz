//! Cancellation must revoke local ownership even when the exact record is absent.
use super::*;
use std::path::PathBuf;

struct UnavailableRecord {
    record: PathBuf,
    moved: PathBuf,
    backup: PathBuf,
    bytes: Vec<u8>,
    directory: bool,
}

impl UnavailableRecord {
    fn hide(fixture: &Fixture, scope: &SessionScope, fault: &str) -> Self {
        let dir = fixture.store.channel_dir(scope.channel_id());
        let record = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.extension().is_some_and(|ext| ext == "json"))
            .unwrap();
        let bytes = std::fs::read(&record).unwrap();
        let directory = fault == "unreadable";
        let moved = if directory { dir } else { record.clone() };
        let backup = moved.with_extension("held");
        std::fs::rename(&moved, &backup).unwrap();
        if directory {
            // ENOTDIR makes both read and quarantine-lock creation fail even
            // for privileged test users; no permission-based false positive.
            std::fs::write(&moved, b"temporarily unavailable directory").unwrap();
            assert!(matches!(fixture.store.load(scope), LoadOutcome::Unreadable));
        } else {
            assert!(matches!(fixture.store.load(scope), LoadOutcome::Missing));
        }
        Self {
            record,
            moved,
            backup,
            bytes,
            directory,
        }
    }

    fn restore(self) {
        if self.directory {
            std::fs::remove_file(&self.moved).unwrap();
        }
        std::fs::rename(&self.backup, &self.moved).unwrap();
        assert_eq!(std::fs::read(self.record).unwrap(), self.bytes);
    }
}

fn queue_human(
    ctx: &PromptContext,
    human: &Keys,
    scope: &SessionScope,
) -> (crate::EventQueue, nostr::Event) {
    let event = event(
        human,
        scope.channel_id(),
        scope.root_event_id(),
        "Preserve this actual human task for the next ordinary turn",
        Some(&ctx.agent_keys),
        nostr::Timestamp::now().as_secs(),
    );
    let mut queue = crate::EventQueue::new(crate::DedupMode::Queue);
    queue.push(crate::QueuedEvent {
        scope: scope.clone(),
        channel_id: scope.channel_id(),
        event: event.clone(),
        received_at: std::time::Instant::now(),
        prompt_tag: "test".into(),
        edit: None,
    });
    (queue, event)
}

fn cancel_unconfirmed(
    ctx: &PromptContext,
    pool: &mut AgentPool,
    queue: &mut crate::EventQueue,
    scope: &SessionScope,
    source: &str,
) {
    let observer = crate::observer::ObserverHandle::in_process();
    if source == "owner" {
        assert_eq!(
            crate::handle_owner_cancel_for_scope(
                pool,
                queue,
                &ctx.session_controls,
                scope,
                Some(&observer)
            ),
            crate::session_controls::status::StatusCancellation::Unconfirmed
        );
    } else {
        crate::handle_cancel_turn_control_with_status(
            &serde_json::json!({"channelId":scope.channel_id().to_string(), "requestId":"unavailable-record"}),
            pool,
            queue,
            Some(&observer),
            Some(&ctx.session_controls),
        );
    }
    let events = observer.snapshot();
    let result = events.last().unwrap();
    assert_eq!(result.kind, "control_result");
    assert_eq!(result.payload["type"], "cancel_turn");
    assert_eq!(result.payload["status"], "unconfirmed");
}

#[allow(clippy::too_many_arguments)]
async fn assert_no_status_and_keep_human(
    fixture: &Fixture,
    ctx: &Arc<PromptContext>,
    relay: &Relay,
    pool: &mut AgentPool,
    mut queue: crate::EventQueue,
    human: &nostr::Event,
    scope: &SessionScope,
    held_worker: OwnedAgent,
) {
    assert!(!ctx.session_controls.status_pending(scope));
    assert!(ctx.session_controls.pending_statuses().is_empty());
    pool.return_agent(held_worker);
    let (tx, mut rx) = mpsc::unbounded_channel();
    crate::status_tasks::dispatch(pool, ctx, &tx);
    assert!(pool.join_set.is_empty());
    assert!(rx.try_recv().is_err());
    let mut worker = pool.try_claim(Some(scope)).unwrap();
    worker.acp.shutdown().await;
    assert!(!fixture
        .methods()
        .iter()
        .any(|method| method == "session/new" || method == "session/prompt"));
    assert!(relay.posts.lock().unwrap().is_empty());
    let batch = queue.flush_next().unwrap();
    assert_eq!(batch.scope, *scope);
    assert_eq!(batch.events.len(), 1);
    assert_eq!(&batch.events[0].event, human);
    assert!(batch.cancelled_events.is_empty());
    assert!(!queue.has_undispatched_work());
}

#[tokio::test]
async fn unavailable_pending_record_cancel_retires_local_fence_before_a_worker_is_free() {
    for fault in ["missing", "unreadable"] {
        for source in ["owner", "observer"] {
            timeout(Duration::from_secs(15), async {
                let fixture = Fixture::new();
                let human = Keys::generate();
                let scope = scope();
                let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
                let restart = admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
                ctx.session_controls.plan_retirement(&scope, [7]);
                assert!(ctx.session_controls.pending_statuses().is_empty());
                ctx.session_controls
                    .retirement_report(7, crate::acp::TerminationReport::Clean);
                let job = ctx.session_controls.pending_statuses().pop().unwrap();
                assert_eq!(job.saved.phase, RestartStatusPhase::Pending);
                let captured = ctx.session_controls.capture(&scope);
                let ctx = Arc::new(ctx);
                let worker = fixture
                    .agent(&[("STATUS_TEXT", "Must never dispatch")])
                    .await;
                let mut pool = AgentPool::from_slots(vec![None]);
                let (mut queue, queued) = queue_human(&ctx, &human, &scope);
                let (tx, _rx) = mpsc::unbounded_channel();
                crate::status_tasks::dispatch(&mut pool, &ctx, &tx);
                assert!(pool.join_set.is_empty());
                let unavailable = UnavailableRecord::hide(&fixture, &scope, fault);
                cancel_unconfirmed(&ctx, &mut pool, &mut queue, &scope, source);
                assert!(!captured.current());
                assert!(ctx.session_controls.pending_statuses().is_empty());
                unavailable.restore();
                assert!(tokio::time::Instant::now() < job.deadline);
                let record = fixture.record(&scope);
                assert_eq!(
                    record.restart_status.unwrap().phase,
                    RestartStatusPhase::Pending
                );
                assert!(record
                    .control_receipts
                    .iter()
                    .any(
                        |receipt| receipt.command_id == restart.origin.command_event_id
                            && receipt.state
                                == crate::session_store::ControlReceiptState::Finished(Ok(()))
                    ));
                assert_eq!(record.pending_restart.unwrap().token, job.saved.token);
                assert_no_status_and_keep_human(
                    &fixture, &ctx, &relay, &mut pool, queue, &queued, &scope, worker,
                )
                .await;
            })
            .await
            .unwrap();
        }
    }
}

#[tokio::test]
async fn unavailable_cancel_during_retirement_cannot_recreate_the_followup_after_clean_exit() {
    for fault in ["missing", "unreadable", "write"] {
        for source in ["owner", "observer"] {
            timeout(Duration::from_secs(15), async {
                let fixture = Fixture::new();
                let human = Keys::generate();
                let scope = scope();
                let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
                let restart = admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
                ctx.session_controls.plan_retirement(&scope, [7]);
                let captured = ctx.session_controls.capture(&scope);
                let old_token = captured.intent.as_ref().unwrap().token;
                assert_eq!(
                    fixture.record(&scope).restart_status.unwrap().phase,
                    RestartStatusPhase::WaitingRetirement
                );
                let ctx = Arc::new(ctx);
                let worker = fixture
                    .agent(&[("STATUS_TEXT", "Must never dispatch")])
                    .await;
                let mut pool = AgentPool::from_slots(vec![None]);
                let (mut queue, queued) = queue_human(&ctx, &human, &scope);
                let unavailable = if fault == "write" {
                    fixture.store.fail_writes(true);
                    None
                } else {
                    Some(UnavailableRecord::hide(&fixture, &scope, fault))
                };
                cancel_unconfirmed(&ctx, &mut pool, &mut queue, &scope, source);
                assert!(!captured.current());
                if let Some(unavailable) = unavailable {
                    unavailable.restore();
                }
                fixture.store.fail_writes(false);
                ctx.session_controls
                    .retirement_report(7, crate::acp::TerminationReport::Clean);
                assert!(!ctx.session_controls.retirement_pending(&scope));
                let record = fixture.record(&scope);
                assert_eq!(record.pending_restart.unwrap().token, old_token);
                assert!(record
                    .control_receipts
                    .iter()
                    .any(
                        |receipt| receipt.command_id == restart.origin.command_event_id
                            && receipt.state
                                == crate::session_store::ControlReceiptState::Finished(Ok(()))
                    ));
                let status = record.restart_status.unwrap();
                assert_eq!(status.phase, RestartStatusPhase::Pending);
                assert!(!status.notice_confirmed);
                assert!(status.prepared.is_none() && status.notice.is_none());
                assert!(ctx
                    .session_controls
                    .take_publications()
                    .iter()
                    .any(
                        |publication| publication.command_id == restart.origin.command_event_id
                            && publication.result
                                == buzz_core::session_control::ControlResult::Terminal(Ok(()))
                    ));
                assert_no_status_and_keep_human(
                    &fixture, &ctx, &relay, &mut pool, queue, &queued, &scope, worker,
                )
                .await;
            })
            .await
            .unwrap();
        }
    }
}

#[tokio::test]
async fn unavailable_record_cancel_fences_already_scheduled_workerless_expiry_feedback() {
    for fault in ["missing", "unreadable"] {
        for source in ["owner", "observer"] {
            // Budget includes the explicit 121-second virtual-time advance.
            timeout(Duration::from_secs(150), async {
                let fixture = Fixture::new();
                let human = Keys::generate();
                let scope = scope();
                let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
                admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
                ctx.session_controls.plan_retirement(&scope, [7]);
                ctx.session_controls
                    .retirement_report(7, crate::acp::TerminationReport::Clean);
                let captured = ctx.session_controls.capture(&scope);
                let ctx = Arc::new(ctx);
                let worker = fixture.agent(&[]).await;
                let mut pool = AgentPool::from_slots(vec![None]);
                let (mut queue, queued) = queue_human(&ctx, &human, &scope);
                let (tx, _rx) = mpsc::unbounded_channel();
                tokio::time::pause();
                tokio::time::advance(Duration::from_secs(121)).await;
                crate::status_tasks::dispatch(&mut pool, &ctx, &tx);
                assert_eq!(pool.join_set.len(), 1);
                let unavailable = UnavailableRecord::hide(&fixture, &scope, fault);
                // No await since spawn: the feedback future is still held.
                cancel_unconfirmed(&ctx, &mut pool, &mut queue, &scope, source);
                assert!(!captured.current());
                unavailable.restore();
                tokio::time::resume();
                timeout(Duration::from_secs(8), pool.join_set.join_next())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                assert!(fixture
                    .record(&scope)
                    .restart_status
                    .unwrap()
                    .notice
                    .is_none());
                assert_no_status_and_keep_human(
                    &fixture, &ctx, &relay, &mut pool, queue, &queued, &scope, worker,
                )
                .await;
            })
            .await
            .unwrap();
        }
    }
}

#[tokio::test]
async fn stale_token_cleanup_cannot_fence_newer_pending_or_retiring_work_when_reads_fail() {
    for phase in ["waiting", "pending"] {
        for fault in ["missing", "unreadable", "readable"] {
            timeout(Duration::from_secs(15), async {
                let fixture = Fixture::new();
                let human = Keys::generate();
                let scope = scope();
                let (mut ctx, _relay) = setup(vec![], scope.channel_id()).await;
                admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
                ctx.session_controls.plan_retirement(&scope, []);
                let old_token = ctx.session_controls.restart_intent(&scope).unwrap().token;
                let later = super::super::control_tests::authorized_control(
                    &ctx,
                    &human,
                    &scope,
                    "!restart all",
                )
                .await;
                assert!(ctx.session_controls.restart(later).unwrap());
                ctx.session_controls.plan_retirement(&scope, [7]);
                if phase == "pending" {
                    ctx.session_controls
                        .retirement_report(7, crate::acp::TerminationReport::Clean);
                }
                let captured = ctx.session_controls.capture(&scope);
                let new_token = captured.intent.as_ref().unwrap().token;
                assert_ne!(old_token, new_token);
                let unavailable =
                    (fault != "readable").then(|| UnavailableRecord::hide(&fixture, &scope, fault));
                assert!(!ctx
                    .session_controls
                    .cancel_status_for_token(&scope, old_token));
                assert!(captured.current());
                assert_eq!(
                    ctx.session_controls.cancellable_status_scopes(),
                    vec![scope.clone()]
                );
                if let Some(unavailable) = unavailable {
                    unavailable.restore();
                }
                assert_eq!(
                    ctx.session_controls.restart_intent(&scope).unwrap().token,
                    new_token
                );
                if phase == "waiting" {
                    ctx.session_controls
                        .retirement_report(7, crate::acp::TerminationReport::Clean);
                }
                assert_eq!(
                    ctx.session_controls
                        .pending_statuses()
                        .pop()
                        .unwrap()
                        .saved
                        .token,
                    new_token
                );
                assert!(captured.current());
            })
            .await
            .unwrap();
        }
    }
}

#[tokio::test]
async fn cancellation_preserves_a_confirmed_terminal_status_even_when_the_busy_cache_lags() {
    timeout(Duration::from_secs(15), async {
        let fixture = Fixture::new();
        let human = Keys::generate();
        let scope = scope();
        let (mut ctx, _relay) = setup(vec![], scope.channel_id()).await;
        admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
        ctx.session_controls.plan_retirement(&scope, []);
        let mut job = ctx.session_controls.pending_statuses().pop().unwrap();
        assert!(ctx.session_controls.start_status(&job));
        let captured = ctx.session_controls.capture(&scope);
        ctx.session_controls
            .transition_status(&mut job, RestartStatusPhase::Started, None, false, false)
            .unwrap();
        let reply = event(
            &ctx.agent_keys,
            scope.channel_id(),
            scope.root_event_id(),
            "Actual confirmed status",
            None,
            nostr::Timestamp::now().as_secs(),
        );
        ctx.session_controls
            .transition_status(
                &mut job,
                RestartStatusPhase::Prepared,
                Some(reply.clone()),
                false,
                false,
            )
            .unwrap();
        let mut completed = job.saved.clone();
        completed.phase = RestartStatusPhase::Completed;
        ctx.session_controls
            .persist_status(&mut job, completed, true)
            .unwrap();
        assert_eq!(
            ctx.session_controls.cancel_status_outcome(&scope),
            crate::session_controls::status::StatusCancellation::None
        );
        assert!(!captured.current());
        assert!(ctx.session_controls.cancellable_status_scopes().is_empty());
        let saved = fixture.record(&scope);
        assert!(saved.pending_restart.is_none());
        let status = saved.restart_status.unwrap();
        assert_eq!(status.phase, RestartStatusPhase::Completed);
        assert_eq!(status.prepared, Some(reply));
    })
    .await
    .unwrap();
}
