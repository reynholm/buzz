//! Portable controller/store/pool gates, with real fake-ACP workers on every OS.
use super::*;
use crate::acp::TerminationReport;
use crate::session_controls::{ControlJob, SessionControlRuntime};
use crate::session_store::SessionStore;
use buzz_core::session_control::{
    ControlFailure, ControlOrigin, ControlResult, RestartDepth, SessionCommand,
};
use nostr::{EventBuilder, Keys, Kind, Tag};

const ADAPTER: &str = r#"
import json,sys
for line in sys.stdin:
    request=json.loads(line)
    if 'id' in request:
        print(json.dumps({'jsonrpc':'2.0','id':request['id'],
            'result':{'protocolVersion':2,'agentCapabilities':{}}}),flush=True)
"#;

struct Fixture {
    _dir: tempfile::TempDir,
    controls: SessionControlRuntime,
    scope: SessionScope,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::open(
            dir.path().join("store"),
            "wss://relay.example",
            &Keys::generate().public_key().to_hex(),
        )
        .unwrap();
        Self {
            _dir: dir,
            controls: SessionControlRuntime::new(Some(Arc::new(store))),
            scope: SessionScope::Conversation {
                channel_id: Uuid::new_v4(),
            },
        }
    }

    // These tests start at the controller boundary. Listener authorization is
    // covered separately; production never constructs jobs from unsigned input.
    fn job(&self, depth: RestartDepth) -> ControlJob {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(9), format!("!restart {}", depth.as_wire()))
            .tags([Tag::parse(["nonce", &Uuid::new_v4().to_string()]).unwrap()])
            .sign_with_keys(&keys)
            .unwrap();
        ControlJob {
            command: SessionCommand::Restart(depth),
            origin: ControlOrigin {
                command_event_id: event.id.to_hex(),
                actor_pubkey: keys.public_key().to_hex(),
            },
            scope: self.scope.clone(),
            dispatch_epoch: Uuid::new_v4(),
            admitted_at: nostr::Timestamp::now().as_secs(),
            event,
        }
    }

    async fn agent(&self, index: usize) -> OwnedAgent {
        let python = if cfg!(windows) { "python" } else { "python3" };
        let mut acp = AcpClient::spawn(
            python,
            &["-u".into(), "-c".into(), ADAPTER.into()],
            &[],
            false,
        )
        .await
        .unwrap();
        acp.initialize().await.unwrap();
        OwnedAgent {
            index,
            acp,
            state: SessionState::default(),
            model_capabilities: None,
            desired_model: None,
            model_overridden: false,
            desired_model_request_id: None,
            desired_model_pending_ack: false,
            startup_effort: None,
            agent_name: "portable-fake".into(),
            goose_system_prompt_supported: None,
            protocol_version: 2,
            background_context: None,
        }
    }
}

#[tokio::test]
async fn portable_retirement_all_targets_must_stop_before_gate_opens() {
    let f = Fixture::new();
    let mut a = f.agent(0).await;
    let mut b = f.agent(1).await;
    let job = f.job(RestartDepth::None);
    let id = job.origin.command_event_id.clone();
    assert!(f.controls.restart(job).unwrap());
    f.controls.plan_retirement(&f.scope, [0, 1]);
    assert!(f.controls.retirement_pending(&f.scope));
    let report = a.acp.terminate().await;
    assert_eq!(report, TerminationReport::Clean);
    f.controls.retirement_report(0, report);
    assert!(f.controls.retirement_pending(&f.scope));
    assert!(f.controls.take_publications().is_empty());
    let report = b.acp.terminate().await;
    assert_eq!(report, TerminationReport::Clean);
    f.controls.retirement_report(1, report);
    assert!(!f.controls.retirement_pending(&f.scope));
    assert_eq!(
        f.controls.prior_result(&f.scope, &id),
        Some(ControlResult::Terminal(Ok(())))
    );
    assert_eq!(
        f.controls.restart_intent(&f.scope).unwrap().depth,
        RestartDepth::None
    );
}

#[tokio::test]
async fn portable_retirement_failure_retry_and_supersession_keep_gate_and_intent() {
    let f = Fixture::new();
    let mut a = f.agent(0).await;
    let first = f.job(RestartDepth::None);
    let first_id = first.origin.command_event_id.clone();
    assert!(f.controls.restart(first).unwrap());
    f.controls.plan_retirement(&f.scope, [0]);
    let second = f.job(RestartDepth::Recent);
    assert!(f.controls.restart(second).unwrap());
    f.controls.plan_retirement(&f.scope, [0]);
    assert_eq!(
        f.controls.prior_result(&f.scope, &first_id),
        Some(ControlResult::Terminal(Err(ControlFailure::Superseded)))
    );
    f.controls
        .retirement_report(0, TerminationReport::Unverified);
    assert!(f.controls.retirement_pending(&f.scope));
    assert!(f.controls.retiring_workers().is_empty());
    let retry = f.job(RestartDepth::All);
    let id = retry.origin.command_event_id.clone();
    assert!(f.controls.restart(retry).unwrap());
    f.controls.plan_retirement(&f.scope, []);
    assert_eq!(f.controls.retiring_workers().len(), 1);
    assert!(f.controls.retirement_pending(&f.scope));
    let report = a.acp.terminate().await;
    assert_eq!(report, TerminationReport::Clean);
    f.controls.retirement_report(0, report);
    assert!(!f.controls.retirement_pending(&f.scope));
    assert_eq!(
        f.controls.prior_result(&f.scope, &id),
        Some(ControlResult::Terminal(Ok(())))
    );
    assert_eq!(
        f.controls.restart_intent(&f.scope).unwrap().depth,
        RestartDepth::All
    );
}

#[tokio::test]
async fn portable_retirement_lost_evidence_rejects_late_clean_and_explicit_retry() {
    let f = Fixture::new();
    assert!(f.controls.restart(f.job(RestartDepth::None)).unwrap());
    f.controls.plan_retirement(&f.scope, [0]);
    f.controls.lose_retirement_evidence(0);
    f.controls.retirement_report(0, TerminationReport::Clean);
    let retry = f.job(RestartDepth::All);
    let id = retry.origin.command_event_id.clone();
    assert!(f.controls.restart(retry).unwrap());
    f.controls.plan_retirement(&f.scope, []);
    f.controls.retirement_report(0, TerminationReport::Clean);
    assert!(f.controls.retirement_evidence_lost(0));
    assert!(f.controls.retirement_pending(&f.scope));
    assert!(f.controls.retiring_workers().is_empty());
    assert_eq!(
        f.controls.prior_result(&f.scope, &id),
        Some(ControlResult::Terminal(Err(ControlFailure::Unconfirmed)))
    );
}

#[tokio::test]
async fn portable_retirement_foreign_foreground_reservation_waits_for_return() {
    let f = Fixture::new();
    let a = f.agent(0).await;
    let mut pool = AgentPool::from_slots(vec![Some(a)]);
    pool.record_scope_owner(f.scope.clone(), 0);
    pool.mark_retire_on_return(0);
    assert!(pool.try_claim(None).is_none());
    let foreign = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let handle = pool.join_set.spawn(std::future::pending::<()>());
    pool.task_map.insert(
        handle.id(),
        TaskMeta {
            agent_index: 0,
            channel_id: Some(foreign.channel_id()),
            scope: Some(foreign),
            turn_id: "foreign-foreground".into(),
            recoverable_batch: None,
            control_tx: None,
            steer_tx: None,
            control_dispatch: None,
            successful_steer_deliveries: Default::default(),
        },
    );
    let scopes = HashSet::from([f.scope.clone()]);
    assert!(pool.take_retirement_target(0, &scopes, true).is_none());
    pool.task_map.clear();
    handle.abort();
    let agent = pool.agents[0].take().unwrap();
    assert!(pool.needs_retirement(&agent));
    pool.return_for_retirement(agent);
    assert!(pool.try_claim(None).is_none());
    let mut agent = pool.take_retirement_target(0, &scopes, true).unwrap();
    assert_eq!(agent.acp.terminate().await, TerminationReport::Clean);
    pool.finish_worker_retirement(0);
    assert!(pool.retirement_targets(&f.scope).is_empty());
}
