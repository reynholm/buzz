//! Review regressions at production control, pending-delivery and heartbeat seams.
use super::*;
const CONTROL_PEER: &str = r#"
import json,os,sys,pathlib,subprocess
pending=None
count=0
for line in sys.stdin:
 r=json.loads(line);m=r.get('method');v={}
 with open(os.environ['LOG'],'a') as f:f.write(json.dumps({'method':m,'params':r.get('params')})+'\n')
 if m=='session/cancel':
  if pending is not None:
   print(json.dumps({'jsonrpc':'2.0','id':pending,'result':{'stopReason':'cancelled'}}),flush=True);pending=None
  continue
 if 'id' not in r:continue
 if m=='initialize':v={'protocolVersion':1,'agentInfo':{'name':'qwen-code','version':'99.0.0-b24.99'},'_meta':{'buzz':{'replyDelivery':{'version':1,'mode':'runtime_final'}}}}
 elif m=='session/new':v={'sessionId':'control-session','models':{'currentModelId':'old','availableModels':[{'modelId':'old'},{'modelId':'new'}]}}
 elif m=='session/prompt':
  count+=1
  if os.environ.get('CASE')=='background_control' and count==1:
   child="import pathlib,sys,time\nwhile not pathlib.Path(sys.argv[1]).exists():time.sleep(.02)"
   subprocess.Popen([sys.executable,'-c',child,os.environ['LOG']+'.finish'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
  print(json.dumps({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':r['params']['sessionId'],'update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'Controlled final'}}}}),flush=True)
  if pathlib.Path(os.environ['LOG']+'.block').exists():pending=r['id'];continue
  v={'stopReason':'end_turn'}
 print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':v}),flush=True)
"#;

async fn wait_prompts(log: &Path, expected: usize) {
    timeout(Duration::from_secs(5), async {
        loop {
            if std::fs::read_to_string(log)
                .unwrap()
                .lines()
                .filter(|line| line.contains("session/prompt"))
                .count()
                >= expected
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn runtime_review_control_continuations_preserve_request_key_and_model_ack() {
    checked_cli();
    for signal in [
        ControlSignal::Steer,
        ControlSignal::Interrupt,
        ControlSignal::SwitchModel {
            model_id: "new".into(),
            request_id: Some("switch-request".into()),
        },
        ControlSignal::Cancel,
        ControlSignal::Rotate,
    ] {
        let terminal = matches!(signal, ControlSignal::Cancel | ControlSignal::Rotate);
        let switching = matches!(signal, ControlSignal::SwitchModel { .. });
        let relay = MockRelay::start().await;
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("control.jsonl");
        let observer = crate::observer::ObserverHandle::in_process();
        let agent = worker_with_peer("control", &log, &observer, CONTROL_PEER).await;
        let ctx = Arc::new(context(&relay, dir.path()));
        let (mut queue, scope) = queue(&relay, dir.path());
        let mut pool = AgentPool::from_slots(vec![Some(agent)]);
        std::fs::write(log.with_extension("jsonl.block"), "block").unwrap();
        assert_eq!(
            background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
            1
        );
        let key = pool
            .task_map
            .values()
            .next()
            .unwrap()
            .recoverable_batch
            .as_ref()
            .unwrap()
            .work
            .as_ref()
            .unwrap()
            .delivery_key;
        wait_prompts(&log, 1).await;
        assert!(crate::signal_in_flight_task_for_scope(
            &mut pool, &scope, signal
        ));
        let result = timeout(Duration::from_secs(10), pool.result_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result.outcome, PromptOutcome::Cancelled));
        if terminal {
            assert!(result.batch.is_none());
            assert!(relay.readback().await.is_empty());
            let mut result = result;
            result.agent.acp.shutdown().await;
            continue;
        }
        let continued = result
            .batch
            .as_ref()
            .expect("authorized control must retain original input");
        assert_eq!(continued.work.as_ref().unwrap().delivery_key, key);
        assert!(
            !continued.work.as_ref().unwrap().cancelled(),
            "control continuation must not finish or revoke its receipt"
        );
        assert!(
            relay.readback().await.is_empty(),
            "interrupted partial text must not publish"
        );
        background_work_tests::apply_result(
            &mut pool,
            &mut queue,
            result,
            Some(&ctx.rest_client),
            &HashSet::new(),
        );
        std::fs::remove_file(log.with_extension("jsonl.block")).unwrap();
        assert_eq!(
            background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
            1
        );
        let mut result = timeout(Duration::from_secs(10), pool.result_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            result.outcome,
            PromptOutcome::Ok(StopReason::EndTurn)
        ));
        if switching {
            assert_eq!(result.agent.desired_model.as_deref(), Some("new"));
            assert!(!result.agent.desired_model_pending_ack);
            assert!(std::fs::read_to_string(&log)
                .unwrap()
                .contains("session/set_model"));
        }
        let reports = relay.readback().await;
        assert_eq!(reports.len(), 1);
        assert_eq!(
            buzz_core::managed_delivery::delivery_key(&reports[0]),
            Some(key)
        );
        let requests: Vec<serde_json::Value> = std::fs::read_to_string(&log)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .filter(|r: &serde_json::Value| r["method"] == "session/prompt")
            .collect();
        assert!(requests[1]["params"]["prompt"]
            .to_string()
            .contains("original task"));
        result.agent.acp.shutdown().await;
    }
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn runtime_review_interrupted_collection_keeps_original_session_and_delivery_key() {
    checked_cli();
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("collection.jsonl");
    let _finish = FinishMarker(log.with_extension("jsonl.finish"));
    let observer = crate::observer::ObserverHandle::in_process();
    let agent = worker_with_peer("background_control", &log, &observer, CONTROL_PEER).await;
    let ctx = Arc::new(context(&relay, dir.path()));
    let (mut queue, scope) = queue(&relay, dir.path());
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        1
    );
    let result = timeout(Duration::from_secs(10), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    let collection_key = result
        .agent
        .background_context
        .as_ref()
        .unwrap()
        .continuation
        .as_ref()
        .unwrap()
        .delivery_key;
    background_work_tests::apply_result(
        &mut pool,
        &mut queue,
        result,
        Some(&ctx.rest_client),
        &HashSet::new(),
    );
    std::fs::write(log.with_extension("jsonl.block"), "block").unwrap();
    std::fs::write(log.with_extension("jsonl.finish"), "finish").unwrap();
    timeout(Duration::from_secs(5), async {
        loop {
            pool.queue_finished_background_work(&mut queue);
            if queue.has_flushable_work() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        1
    );
    wait_prompts(&log, 2).await;
    assert!(crate::signal_in_flight_task_for_scope(
        &mut pool,
        &scope,
        ControlSignal::Interrupt
    ));
    let result = timeout(Duration::from_secs(10), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    let continuation = result
        .batch
        .as_ref()
        .expect("collection continuation must survive");
    assert_eq!(
        continuation.work.as_ref().unwrap().delivery_key,
        collection_key
    );
    assert!(continuation.continuation.is_some());
    assert!(!continuation.work.as_ref().unwrap().cancelled());
    assert_eq!(
        result.agent.state.sessions.get(&scope).map(String::as_str),
        Some("control-session")
    );
    background_work_tests::apply_result(
        &mut pool,
        &mut queue,
        result,
        Some(&ctx.rest_client),
        &HashSet::new(),
    );
    std::fs::remove_file(log.with_extension("jsonl.block")).unwrap();
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        1
    );
    let mut result = timeout(Duration::from_secs(10), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    let reports = relay.readback().await;
    assert_eq!(reports.len(), 2);
    assert!(reports
        .iter()
        .any(|event| buzz_core::managed_delivery::delivery_key(event) == Some(collection_key)));
    result.agent.acp.shutdown().await;
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn runtime_review_late_owner_cancel_revokes_returned_pending_publication() {
    let cli = checked_cli();
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("pending.jsonl");
    let observer = crate::observer::ObserverHandle::in_process();
    let agent = worker("plain", &log, &observer).await;
    let ctx = Arc::new(context(&relay, dir.path()));
    let (mut queue, scope) = queue(&relay, dir.path());
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    relay.losses.store(3, Ordering::SeqCst);
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        1
    );
    let result = timeout(Duration::from_secs(15), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(states(&observer)
        .iter()
        .any(|state| state == "delivery_pending"));
    background_work_tests::apply_result(
        &mut pool,
        &mut queue,
        result,
        Some(&ctx.rest_client),
        &HashSet::new(),
    );
    assert!(pool.task_map.is_empty());
    // No strong execution work remains after the real result handler.
    assert!(!queue.cancel_logical_work(&scope));
    let journal_path = dir.path().join(".buzz-runtime-deliveries/journal.json");
    let bytes = std::fs::read(&journal_path).unwrap();
    let journal: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let record = journal["entries"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(record["runtime"]["scope"], "thread");
    assert_eq!(record["parent"], relay.parent.id.to_hex());
    std::fs::write(&journal_path, "invalid journal").unwrap();
    assert_eq!(
        crate::handle_owner_cancel_for_scope(
            &mut pool,
            &mut queue,
            &ctx.session_controls,
            &scope,
            Some(&observer),
            Some(&ctx.rest_client)
        )
        .await,
        crate::session_controls::status::StatusCancellation::Unconfirmed
    );
    assert!(observer
        .snapshot()
        .iter()
        .any(|event| event.kind == "control_result" && event.payload["status"] == "unconfirmed"));
    std::fs::write(&journal_path, &bytes).unwrap();
    assert_eq!(
        crate::handle_owner_cancel_for_scope(
            &mut pool,
            &mut queue,
            &ctx.session_controls,
            &scope,
            None,
            Some(&ctx.rest_client)
        )
        .await,
        crate::session_controls::status::StatusCancellation::Cancelled
    );
    let before = relay.bodies.lock().unwrap().len();
    let prompts = std::fs::read(&log).unwrap();
    assert_eq!(
        crate::runtime_delivery::replay_pending_using(
            &cli,
            &ctx.rest_client,
            &dir.path().join(".buzz-runtime-deliveries")
        )
        .await
        .unwrap(),
        0,
        "late cancellation must survive worker return and restart"
    );
    assert_eq!(relay.bodies.lock().unwrap().len(), before);
    assert_eq!(std::fs::read(&log).unwrap(), prompts);
    for agent in pool.agents.iter_mut().flatten() {
        agent.acp.shutdown().await;
    }
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn runtime_review_heartbeat_keeps_manual_contract_without_publication_authority() {
    checked_cli();
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("heartbeat.jsonl");
    let observer = crate::observer::ObserverHandle::in_process();
    let agent = worker("plain", &log, &observer).await;
    let mut context = context(&relay, dir.path());
    context.heartbeat_prompt = Some("Use messaging tools for outstanding mentions.".into());
    context.base_prompt = Some("Send replies using messaging tools.".into());
    let ctx = Arc::new(context);
    let (mut queue, _) = queue(&relay, dir.path());
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        1
    );
    let result = timeout(Duration::from_secs(10), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    background_work_tests::apply_result(
        &mut pool,
        &mut queue,
        result,
        Some(&ctx.rest_client),
        &HashSet::new(),
    );
    std::fs::write(ctx.reply_build_file.as_ref().unwrap(), "0.26.7-b24.9").unwrap();
    crate::dispatch_heartbeat(&mut pool, &ctx, &mut false);
    let mut result = timeout(Duration::from_secs(10), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result.source, PromptSource::Heartbeat));
    assert!(
        matches!(result.outcome, PromptOutcome::Ok(StopReason::EndTurn)),
        "heartbeat must not require runtime profile provenance"
    );
    let requests: Vec<serde_json::Value> = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|r: &serde_json::Value| r["method"] == "session/prompt")
        .collect();
    assert_eq!(requests.len(), 2);
    let heartbeat = requests[1]["params"]["prompt"].to_string();
    assert!(heartbeat.contains("Use messaging tools"));
    assert!(!heartbeat.contains("Buzz publishes your final"));
    assert!(
        result.agent.acp.take_runtime_final(true).is_err(),
        "heartbeat must not retain runtime capture"
    );
    assert_eq!(relay.readback().await.len(), 1);
    result.agent.acp.shutdown().await;
}
