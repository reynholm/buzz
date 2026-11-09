//! Actual ACP pipes + original CLI + signature-checking mock relay journeys.
use super::*;
use crate::queue::QueuedEvent;
use std::path::Path;
const PEER: &str = r#"
import json,os,sys,subprocess,time
prompt_count=0
for line in sys.stdin:
 r=json.loads(line); m=r.get('method'); v={}
 if 'id' not in r: continue
 if os.environ.get('LOG'):
  with open(os.environ['LOG'],'a') as f: f.write(json.dumps({'method':m,'params':r.get('params')})+'\n')
 if m=='initialize': v={'protocolVersion':1,'agentInfo':{'name':'qwen-code','version':'0.26.7'},'agentCapabilities':{'sessionCapabilities':{'resume':{}}}}
 elif m=='session/new':
  if os.environ.get('CASE')=='cancel_setup':time.sleep(.5)
  v={'sessionId':'new-session'}
 elif m=='session/resume': v={'sessionId':r['params']['sessionId']}
 elif m=='session/prompt':
  prompt_count+=1
  case=os.environ.get('CASE','plain')
  if case.startswith('human'):
   p={'sessionId':r['params']['sessionId'],'toolCall':{'toolCallId':'tool-1'},'approvalRequired':True,'options':[{'kind':'allow_once','optionId':'allow'}]}
   print(json.dumps({'jsonrpc':'2.0','id':'approval-1','method':'session/request_permission','params':p}),flush=True)
   ack=json.loads(sys.stdin.readline())
   assert ack['result']['outcome']['outcome']=='cancelled'
   if case=='human-eof':sys.exit(0)
   if case=='human-idle':time.sleep(30)
  if case=='background' and prompt_count==1:
   child="import pathlib,sys,time\nwhile not pathlib.Path(sys.argv[1]).exists():time.sleep(.02)"
   subprocess.Popen([sys.executable,'-c',child,os.environ['LOG']+'.finish'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
  updates=[]
  def text(value,meta=None):
   u={'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':value}}
   if meta is not None:u['_meta']=meta
   return u
  if case=='segmented':
   updates.extend([text('intermediate'),{'sessionUpdate':'tool_call','toolCallId':'one','status':'pending'}, {'sessionUpdate':'tool_call_update','toolCallId':'one','status':'completed'}, {'sessionUpdate':'agent_thought_chunk','content':{'type':'text','text':'private'}}])
  if case!='missing':updates.append(text(os.environ.get('ANSWER','Which option?')))
  updates.extend([text('child',{'parentToolCallId':'child','agent':{'agentName':'/root/child','depth':1,'kind':'subagent'}}), {'sessionUpdate':'tool_call','toolCallId':'child','_meta':{'parentToolCallId':'child'}}, text('',{'executionLifecycle':{'v':1,'kind':'request','phase':'ended','outcome':'success'}}), text('',{'usage':{}})])
  for u in updates:print(json.dumps({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':r['params']['sessionId'],'update':u}}),flush=True)
  if case in ('error','human-error'):
   print(json.dumps({'jsonrpc':'2.0','id':r['id'],'error':{'code':-32603,'message':'synthetic provider failure'}}),flush=True);continue
  v={'stopReason':'cancelled' if case=='cancel' else 'end_turn'}
 print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':v}),flush=True)
"#;

fn checked_cli() -> PathBuf {
    let cli = binaries().join("buzz");
    let program = crate::managed_work::delivery_command();
    let resolved = if program.is_file() {
        program
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|dir| dir.join(&program))
            .find(|path| path.is_file())
            .unwrap()
    };
    assert_eq!(
        resolved.canonicalize().unwrap(),
        cli.canonicalize().unwrap(),
        "put the rebuilt CLI on PATH before this opt-in journey"
    );
    cli
}

async fn worker(case: &str, log: &Path, observer: &crate::observer::ObserverHandle) -> OwnedAgent {
    worker_with_peer(case, log, observer, PEER).await
}

async fn worker_with_peer(
    case: &str,
    log: &Path,
    observer: &crate::observer::ObserverHandle,
    peer: &str,
) -> OwnedAgent {
    let private = case == "authorized-provider-background";
    let (command, args) = if private {
        use std::os::unix::fs::PermissionsExt;
        let wrapper = log.parent().unwrap().join("b24-cowork-code-cli");
        std::fs::write(&wrapper, format!("#!/usr/bin/env python3\n{peer}")).unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(log.parent().unwrap().join("authorization-fixture"), "").unwrap();
        (wrapper.to_string_lossy().into_owned(), vec![])
    } else {
        (
            "python3".into(),
            vec!["-u".into(), "-c".into(), peer.into()],
        )
    };
    let mut acp = AcpClient::spawn(
        &command,
        &args,
        &[
            ("CASE".into(), case.into()),
            ("LOG".into(), log.to_string_lossy().into_owned()),
            (
                "ANSWER".into(),
                if case.starts_with("scope-") {
                    case.into()
                } else {
                    "Which option?".into()
                },
            ),
        ],
        false,
    )
    .await
    .unwrap();
    acp.initialize().await.unwrap();
    acp.set_observer(Some(observer.clone()), 0);
    OwnedAgent {
        index: 0,
        acp,
        state: SessionState::default(),
        model_capabilities: None,
        desired_model: None,
        model_overridden: false,
        desired_model_request_id: None,
        desired_model_pending_ack: false,
        startup_effort: None,
        agent_name: "qwen-code".into(),
        goose_system_prompt_supported: None,
        protocol_version: 1,
        background_context: None,
    }
}

fn context(relay: &MockRelay, dir: &Path) -> PromptContext {
    let mut ctx = tests::make_prompt_context_no_owner();
    ctx.cwd = dir.to_string_lossy().into_owned();
    ctx.memory_enabled = false;
    ctx.context_message_limit = 0;
    ctx.max_turn_duration = Duration::from_secs(10);
    ctx.rest_client.base_url = relay.url.clone();
    ctx.rest_client.keys = relay.keys.clone();
    ctx.reply_contract = Some(crate::config::ReplyContract::b24());
    let version = dir.join("VERSION");
    std::fs::write(&version, "0.26.7-b24.8\n").unwrap();
    ctx.reply_build_file = Some(version);
    ctx.channel_info = ChannelInfoResolver::new(
        HashMap::from([(
            relay.channel,
            crate::relay::ChannelInfo {
                name: "test".into(),
                channel_type: "stream".into(),
                description: None,
            },
        )]),
        ctx.rest_client.clone(),
    );
    ctx
}

fn queue(relay: &MockRelay, dir: &Path) -> (EventQueue, SessionScope) {
    let scope = SessionScope::Thread {
        channel_id: relay.channel,
        root_event_id: relay.parent.id.to_hex(),
    };
    let mut queue = EventQueue::new(crate::config::DedupMode::Queue)
        .with_runtime_delivery(dir.to_str().unwrap(), 60);
    queue.push(QueuedEvent {
        channel_id: relay.channel,
        scope: scope.clone(),
        event: relay.parent.clone(),
        prompt_tag: "test".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
    (queue, scope)
}

fn states(observer: &crate::observer::ObserverHandle) -> Vec<String> {
    observer
        .snapshot()
        .into_iter()
        .filter(|event| event.kind == "work_state")
        .filter_map(|event| event.payload["state"].as_str().map(str::to_owned))
        .collect()
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn provider_background_runtime_final_waits_for_all_acknowledged_results_before_delivery() {
    checked_cli();
    for scenario in [
        "normal",
        "missing-ack",
        "crash-after-ack",
        "stale-first",
        "shutdown",
    ] {
        provider_background_delivery_scenario(scenario).await;
    }
}

async fn provider_background_delivery_scenario(scenario: &str) {
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("requests");
    let observer = crate::observer::ObserverHandle::in_process();
    let private = ["stale-first", "shutdown"].contains(&scenario);
    let peer_case = if private {
        "authorized-provider-background"
    } else {
        "provider-background"
    };
    let agent = worker_with_peer(
        peer_case,
        &log,
        &observer,
        crate::pool::background_work_tests::provider_background_fixture::ADAPTER,
    )
    .await;
    let ctx = Arc::new(context(&relay, dir.path()));
    let (mut queue, scope) = queue(&relay, dir.path());
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    pool.set_session_store(Some(Arc::new(
        crate::session_store::SessionStore::open(
            dir.path().join("sessions"),
            &relay.url.replace("http://", "ws://"),
            &relay.keys.public_key().to_hex(),
        )
        .unwrap(),
    )));
    if scenario == "stale-first" {
        std::fs::write(dir.path().join("block-prompt"), "").unwrap();
    }
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        1
    );
    if scenario == "stale-first" {
        timeout(Duration::from_secs(5), async {
            while !dir.path().join("prompt-started").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let input = EventBuilder::new(
            Kind::Custom(9),
            "Use the revised scope for existing results",
        )
        .tag(nostr::Tag::parse(["e", &relay.parent.id.to_hex(), "", "reply"]).unwrap())
        .sign_with_keys(&relay.keys)
        .unwrap();
        pool.accept_authorization_input(
            &scope,
            crate::provider_background::SavedEvent {
                event: input,
                edit: None,
            },
        )
        .unwrap();
        std::fs::remove_file(dir.path().join("block-prompt")).unwrap();
    }
    let result = timeout(Duration::from_secs(15), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    if scenario == "stale-first" {
        assert!(matches!(
            result.outcome,
            PromptOutcome::Error(AcpError::BackgroundCollectionPending)
        ));
        assert!(result.batch.as_ref().unwrap().provider_collection());
        assert_eq!(result.agent.state.sessions[&scope], "original-session");
    } else {
        assert!(matches!(
            result.outcome,
            PromptOutcome::Ok(StopReason::EndTurn)
        ));
    }
    background_work_tests::apply_result(
        &mut pool,
        &mut queue,
        result,
        Some(&ctx.rest_client),
        &HashSet::new(),
    );
    assert!(pool.has_runtime_hold());
    assert!(
        relay.readback().await.is_empty(),
        "end_turn with live tasks must not publish missing_final"
    );
    if scenario == "shutdown" {
        let original = pool.agents[0]
            .as_ref()
            .unwrap()
            .acp
            .provider_background_record()
            .unwrap()
            .clone();
        pool.revoke_authorization_for_shutdown();
        pool.agents[0].as_mut().unwrap().acp.shutdown().await;
        drop(pool);
        drop(queue);
        let store = Arc::new(
            crate::session_store::SessionStore::open(
                dir.path().join("sessions"),
                &relay.url.replace("http://", "ws://"),
                &relay.keys.public_key().to_hex(),
            )
            .unwrap(),
        );
        let pending = store.pending_provider_work(&[scope.channel_id()]).unwrap();
        assert_eq!(pending.len(), 1, "shutdown lost running work");
        assert_eq!(pending[0].identity, original.identity);
        assert_eq!(pending[0].deadline_unix, original.deadline_unix);
        assert_eq!(
            pending[0]
                .authorization
                .as_ref()
                .unwrap()
                .snapshot(&pending[0])["state"],
            "revoked"
        );
        queue = EventQueue::new(crate::config::DedupMode::Queue)
            .with_runtime_delivery(dir.path().to_str().unwrap(), 60);
        queue.restore_provider_background(pending[0].clone(), dir.path().to_str().unwrap());
        let agent = worker_with_peer(
            peer_case,
            &log,
            &observer,
            crate::pool::background_work_tests::provider_background_fixture::ADAPTER,
        )
        .await;
        pool = AgentPool::from_slots(vec![Some(agent)]);
        pool.set_session_store(Some(store));
        assert_eq!(
            background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
            1
        );
        let result = timeout(Duration::from_secs(15), pool.result_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            result.outcome,
            PromptOutcome::Ok(StopReason::EndTurn)
        ));
        background_work_tests::apply_result(
            &mut pool,
            &mut queue,
            result,
            Some(&ctx.rest_client),
            &HashSet::new(),
        );
        assert_eq!(
            std::fs::read_to_string(&log)
                .unwrap()
                .lines()
                .filter(|line| line.contains("session/prompt"))
                .count(),
            1,
            "running-only restore issued a model prompt"
        );
    }
    for index in 0..3 {
        std::fs::write(dir.path().join(format!("child-{index}")), "done").unwrap();
    }
    timeout(Duration::from_secs(5), async {
        loop {
            pool.queue_finished_background_work(&mut queue);
            if pool.agents[0]
                .as_ref()
                .unwrap()
                .acp
                .provider_background_record()
                .is_some_and(|record| record.pending().len() == 3)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    if scenario == "missing-ack" {
        std::fs::write(dir.path().join("omit-ack"), "").unwrap();
        assert_eq!(
            background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
            1
        );
        let result = timeout(Duration::from_secs(15), pool.result_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            result.outcome,
            PromptOutcome::Error(AcpError::BackgroundCollectionPending)
        ));
        background_work_tests::apply_result(
            &mut pool,
            &mut queue,
            result,
            Some(&ctx.rest_client),
            &HashSet::new(),
        );
        assert!(relay.readback().await.is_empty());
        std::fs::remove_file(dir.path().join("omit-ack")).unwrap();
        // Wait for the actual bounded collection retry, never run the original launch.
        tokio::time::sleep(Duration::from_secs(7)).await;
    }
    if scenario == "crash-after-ack" {
        std::fs::write(dir.path().join("block-final"), "").unwrap();
        assert_eq!(
            background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
            1
        );
        let store = pool.store.clone().unwrap();
        timeout(Duration::from_secs(5), async {
            loop {
                if let crate::session_store::LoadOutcome::Found(saved) = store.load(&scope) {
                    if saved
                        .live
                        .as_ref()
                        .unwrap()
                        .provider_background
                        .as_ref()
                        .unwrap()
                        .tasks
                        .values()
                        .all(|task| task.state == crate::provider_background::State::Collected)
                    {
                        break;
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(relay.readback().await.is_empty());
        assert!(!dir
            .path()
            .join(".buzz-runtime-deliveries/journal.json")
            .exists());
        pool.join_set.abort_all();
        while pool.join_set.join_next().await.is_some() {}
        drop(pool);
        drop(store);
        drop(queue);
        let store = Arc::new(
            crate::session_store::SessionStore::open(
                dir.path().join("sessions"),
                &relay.url.replace("http://", "ws://"),
                &relay.keys.public_key().to_hex(),
            )
            .unwrap(),
        );
        let records = store.pending_provider_work(&[scope.channel_id()]).unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].parent_pending);
        assert!(!records[0].outstanding());
        queue = EventQueue::new(crate::config::DedupMode::Queue)
            .with_runtime_delivery(dir.path().to_str().unwrap(), 60);
        queue.restore_provider_background(records[0].clone(), dir.path().to_str().unwrap());
        let agent = worker_with_peer(
            "provider-background",
            &log,
            &observer,
            crate::pool::background_work_tests::provider_background_fixture::ADAPTER,
        )
        .await;
        pool = AgentPool::from_slots(vec![Some(agent)]);
        pool.set_session_store(Some(store));
        std::fs::remove_file(dir.path().join("block-final")).unwrap();
    }
    timeout(Duration::from_secs(10), async {
        while background_work_tests::dispatch(&mut pool, &mut queue, &ctx) == 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let result = timeout(Duration::from_secs(15), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(result.outcome, PromptOutcome::Ok(StopReason::EndTurn)),
        "scenario {scenario}: outcome={:?}, registry={:?}",
        match &result.outcome {
            PromptOutcome::Error(error) => error.to_string(),
            PromptOutcome::Ok(stop) => format!("{stop:?}"),
            _ => "other outcome".into(),
        },
        result.agent.acp.provider_background_record()
    );
    background_work_tests::apply_result(
        &mut pool,
        &mut queue,
        result,
        Some(&ctx.rest_client),
        &HashSet::new(),
    );
    let reports = relay.readback().await;
    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0].content,
        "All three existing results were collected."
    );
    let tags = buzz_core::nip10::parse_thread_markers_from_parts(
        reports[0].tags.iter().map(|tag| tag.as_slice()),
    );
    assert_eq!(
        tags.reply.as_deref(),
        Some(relay.parent.id.to_hex().as_str())
    );
    assert!(
        !pool.has_runtime_hold(),
        "scenario {scenario}: holds={:?}, registry={:?}",
        pool.runtime_holds,
        pool.agents[0]
            .as_ref()
            .unwrap()
            .acp
            .provider_background_record()
    );
    assert!(!queue.has_pending_scope(&scope));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    if private {
        let requests: Vec<serde_json::Value> = std::fs::read_to_string(&log)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let prompts: Vec<_> = requests
            .iter()
            .filter(|request| request["method"] == "session/prompt")
            .collect();
        assert_eq!(prompts.len(), 2);
        assert_eq!(
            prompts[0]["params"]["_meta"]["buzz"]["backgroundTasks"]["identity"],
            prompts[1]["params"]["_meta"]["buzz"]["backgroundTasks"]["identity"]
        );
        if scenario == "stale-first" {
            assert!(prompts[1].to_string().contains("Use the revised scope"));
        }
        if scenario == "shutdown" {
            assert_eq!(
                prompts[1]["params"]["_meta"]["buzz"]["authorizationContext"]["state"],
                "revoked"
            );
        }
    }
    if scenario == "normal" {
        // Recreate the crash window: the signed final is durable, but the host
        // has not saved parent completion. The old deadline is now in the past.
        let agent = pool.agents[0].as_ref().unwrap();
        let store = agent.state.store.clone().unwrap();
        let mut record = agent.acp.provider_background_record().unwrap().clone();
        record.parent_pending = true;
        record.terminal_reason = None;
        record.deadline_unix = 1;
        store
            .commit_background(&scope, agent.state.leases[&scope], record.clone())
            .unwrap();
        for agent in pool.agents.iter_mut().flatten() {
            agent.acp.shutdown().await;
        }
        drop(pool);
        drop(queue);
        drop(store);
        let store = Arc::new(
            crate::session_store::SessionStore::open(
                dir.path().join("sessions"),
                &relay.url.replace("http://", "ws://"),
                &relay.keys.public_key().to_hex(),
            )
            .unwrap(),
        );
        let pending = store.pending_provider_work(&[scope.channel_id()]).unwrap();
        assert_eq!(pending.len(), 1);
        queue = EventQueue::new(crate::config::DedupMode::Queue)
            .with_runtime_delivery(dir.path().to_str().unwrap(), 60);
        crate::recover_provider_background(
            &store,
            pending[0].clone(),
            dir.path().to_str().unwrap(),
            &ctx.rest_client,
            &mut queue,
        )
        .await
        .unwrap();
        assert!(!queue.has_pending_scope(&scope));
        let crate::session_store::LoadOutcome::Found(saved) = store.load(&scope) else {
            panic!("missing handoff registry");
        };
        assert!(
            !saved
                .live
                .unwrap()
                .provider_background
                .unwrap()
                .recoverable(),
            "journal handoff must terminalize the durable registry before a new request"
        );
        let agent = worker_with_peer(
            "provider-background",
            &log,
            &observer,
            crate::pool::background_work_tests::provider_background_fixture::ADAPTER,
        )
        .await;
        pool = AgentPool::from_slots(vec![Some(agent)]);
        pool.set_session_store(Some(store));
        let request = EventBuilder::new(
            Kind::Custom(9),
            "Start a new independent background request",
        )
        .tag(nostr::Tag::parse(["e", &relay.parent.id.to_hex(), "", "reply"]).unwrap())
        .sign_with_keys(&relay.keys)
        .unwrap();
        queue.push(QueuedEvent {
            channel_id: scope.channel_id(),
            scope: scope.clone(),
            event: request,
            received_at: std::time::Instant::now(),
            prompt_tag: "mention".into(),
            edit: None,
        });
        assert_eq!(
            background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
            1
        );
        let result = timeout(Duration::from_secs(15), pool.result_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(result.outcome, PromptOutcome::Ok(StopReason::EndTurn)),
            "new request must reach its own prompt after the old deadline"
        );
        let next = result.agent.acp.provider_background_record().unwrap();
        assert_ne!(next.identity, record.identity);
        assert!(next.deadline_unix > crate::provider_background::now_unix());
        background_work_tests::apply_result(
            &mut pool,
            &mut queue,
            result,
            Some(&ctx.rest_client),
            &HashSet::new(),
        );
        assert_eq!(relay.readback().await.len(), 1, "no duplicate old final");
    }
    for mut agent in pool.agents.into_iter().flatten() {
        agent.acp.shutdown().await;
    }
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn runtime_final_pool_delivers_only_final_question_with_typed_receipt() {
    checked_cli();
    for case in [
        "plain",
        "segmented",
        "missing",
        "error",
        "cancel",
        "human",
        "human-error",
        "human-eof",
        "human-idle",
    ] {
        let relay = MockRelay::start().await;
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("provider.jsonl");
        let observer = crate::observer::ObserverHandle::in_process();
        let agent = worker(case, &log, &observer).await;
        let mut context = context(&relay, dir.path());
        if case == "human-idle" {
            context.idle_timeout = Duration::from_millis(300);
        }
        let ctx = Arc::new(context);
        let (mut queue, scope) = queue(&relay, dir.path());
        let mut pool = AgentPool::from_slots(vec![Some(agent)]);
        assert_eq!(
            background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
            1
        );
        let result = timeout(Duration::from_secs(15), pool.result_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            result.batch.is_none(),
            "runtime final must consume failed work, without task replay"
        );
        if case.starts_with("human") {
            let old_pid = result.agent.acp.process_id();
            let mut config = crate::build_mcp_servers_tests::test_config();
            config.agent_command = "python3".into();
            config.agent_args = vec!["-u".into(), "-c".into(), PEER.into()];
            config.persona_env_vars = vec![
                ("CASE".into(), "plain".into()),
                (
                    "LOG".into(),
                    dir.path()
                        .join("replacement.jsonl")
                        .to_string_lossy()
                        .into_owned(),
                ),
            ];
            let mut history = vec![crate::SlotCircuit {
                crash_times: vec![],
                open_until: None,
                respawn_in_flight: false,
            }];
            let (tx, mut rx) = mpsc::channel(8);
            let mut respawns = tokio::task::JoinSet::new();
            crate::handle_prompt_result(
                &mut pool,
                &mut queue,
                &config,
                result,
                &mut false,
                &HashSet::new(),
                &mut history,
                &tx,
                &mut respawns,
                None,
                Some(&ctx.rest_client),
                &mut tokio::time::Instant::now(),
            );
            assert!(
                pool.agents[0].is_none(),
                "{case}: blocked/failed worker returned to the available pool"
            );
            assert_eq!(respawns.len(), 1, "{case}: worker was not retired");
            timeout(Duration::from_secs(10), respawns.join_next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let (mut replacement, _, _) = rx.recv().await.unwrap().result.unwrap();
            assert_ne!(replacement.process_id(), old_pid);
            // The replacement must accept unrelated work without the old prompt.
            replacement
                .session_new(dir.path().to_str().unwrap(), vec![], None, None)
                .await
                .unwrap();
            assert!(matches!(
                replacement
                    .session_prompt_with_idle_timeout(
                        "new-session",
                        "Independent next request",
                        Duration::from_secs(2),
                        Duration::from_secs(5)
                    )
                    .await,
                Ok(StopReason::EndTurn)
            ));
            replacement.shutdown().await;
        } else {
            background_work_tests::apply_result(
                &mut pool,
                &mut queue,
                result,
                Some(&ctx.rest_client),
                &HashSet::new(),
            );
        }
        assert_eq!(queue.queued_event_count(&scope), 0);
        assert_eq!(
            std::fs::read_to_string(&log)
                .unwrap()
                .lines()
                .filter(|line| line.contains("session/prompt"))
                .count(),
            1
        );
        let reports = relay.readback().await;
        if case == "cancel" {
            assert!(reports.is_empty());
            assert!(states(&observer).iter().any(|state| state == "cancelled"));
        } else {
            assert_eq!(reports.len(), 1);
            assert_eq!(reports[0].pubkey, relay.keys.public_key());
            assert!(
                reports[0]
                    .tags
                    .iter()
                    .any(|tag| tag.as_slice() == ["p", relay.parent.pubkey.to_hex().as_str()]),
                "the accepted request author must receive the structured callback"
            );
            let tags = buzz_core::nip10::parse_thread_markers_from_parts(
                reports[0].tags.iter().map(|tag| tag.as_slice()),
            );
            assert_eq!(
                tags.reply.as_deref(),
                Some(relay.parent.id.to_hex().as_str())
            );
            if ["plain", "segmented"].contains(&case) {
                assert_eq!(reports[0].content, "Which option?");
                assert!(states(&observer)
                    .iter()
                    .any(|state| state == "answer_confirmed"));
            } else if case.starts_with("human") {
                assert!(reports[0].content.contains("A human decision is required"));
                assert!(!reports[0].content.contains("Which option?"));
                assert!(states(&observer)
                    .iter()
                    .any(|state| state == "waiting_human"));
                assert!(!states(&observer)
                    .iter()
                    .any(|state| state == "missing_final"));
                assert!(!states(&observer)
                    .iter()
                    .any(|state| state == "answer_confirmed"));
                if case != "human" {
                    assert!(reports[0].content.contains("worker also failed"));
                }
            } else {
                assert!(reports[0]
                    .content
                    .contains("did not produce a verified final answer"));
                assert!(!reports[0].content.contains("Which option?"));
                assert!(states(&observer)
                    .iter()
                    .any(|state| state == "missing_final"));
                assert!(states(&observer)
                    .iter()
                    .any(|state| state == "failure_notice_confirmed"));
            }
            assert!(
                !states(&observer).iter().any(|state| state == "completed"),
                "answer delivery does not certify task completion"
            );
        }
        for agent in pool.agents.iter_mut().flatten() {
            agent.acp.shutdown().await;
        }
    }
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn runtime_final_restart_replays_saved_bytes_without_a_provider_prompt() {
    let cli = checked_cli();
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("provider.jsonl");
    let observer = crate::observer::ObserverHandle::in_process();
    let agent = worker("plain", &log, &observer).await;
    let ctx = Arc::new(context(&relay, dir.path()));
    let (mut queue, _) = queue(&relay, dir.path());
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    relay.losses.store(3, Ordering::SeqCst);
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        1
    );
    let mut result = timeout(Duration::from_secs(15), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(states(&observer)
        .iter()
        .any(|state| state == "delivery_pending"));
    assert_eq!(relay.events.lock().unwrap().len(), 1);
    result.agent.acp.shutdown().await;
    let prompts = std::fs::read(&log).unwrap();
    // Provider upgrade/unavailability cannot suppress already-saved delivery.
    std::fs::write(ctx.reply_build_file.as_ref().unwrap(), "0.26.7-b24.9").unwrap();
    assert_eq!(
        crate::runtime_delivery::replay_pending_using(
            &cli,
            &ctx.rest_client,
            &dir.path().join(".buzz-runtime-deliveries")
        )
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        std::fs::read(&log).unwrap(),
        prompts,
        "restart recovery must not prompt a provider"
    );
    let bodies = relay.bodies.lock().unwrap().clone();
    assert!(bodies.len() >= 2);
    assert!(bodies.iter().all(|body| body == &bodies[0]));
    assert_eq!(relay.readback().await.len(), 1);
    assert_eq!(
        crate::runtime_delivery::replay_pending_using(
            &cli,
            &ctx.rest_client,
            &dir.path().join(".buzz-runtime-deliveries")
        )
        .await
        .unwrap(),
        0
    );
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn runtime_final_parallel_scopes_keep_text_key_and_original_route_separate() {
    checked_cli();
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let observer = crate::observer::ObserverHandle::in_process();
    let ctx = Arc::new(context(&relay, dir.path()));
    let (mut queue, first_scope) = queue(&relay, dir.path());
    let second_parent = EventBuilder::new(Kind::Custom(9), "second task")
        .tag(Tag::parse(["h", &relay.channel.to_string()]).unwrap())
        .sign_with_keys(&relay.keys)
        .unwrap();
    relay.events.lock().unwrap().push(second_parent.clone());
    let second_scope = SessionScope::Thread {
        channel_id: relay.channel,
        root_event_id: second_parent.id.to_hex(),
    };
    queue.push(QueuedEvent {
        channel_id: relay.channel,
        scope: second_scope.clone(),
        event: second_parent.clone(),
        prompt_tag: "test".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
    let first = worker("scope-a", &dir.path().join("a.jsonl"), &observer).await;
    let mut second = worker("scope-b", &dir.path().join("b.jsonl"), &observer).await;
    second.index = 1;
    second.acp.set_observer(Some(observer.clone()), 1);
    let mut pool = AgentPool::from_slots(vec![Some(first), Some(second)]);
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        2
    );
    let expected: HashMap<_, _> = pool
        .task_map
        .values()
        .map(|meta| {
            let batch = meta.recoverable_batch.as_ref().unwrap();
            (
                batch.work.as_ref().unwrap().delivery_key,
                batch.scope.root_event_id().unwrap().to_string(),
            )
        })
        .collect();
    for _ in 0..2 {
        let result = timeout(Duration::from_secs(15), pool.result_rx.recv())
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
    }
    let reports: Vec<_> = relay
        .readback()
        .await
        .into_iter()
        .filter(|event| event.content.starts_with("scope-"))
        .collect();
    assert_eq!(reports.len(), 2);
    assert_ne!(reports[0].content, reports[1].content);
    for event in reports {
        let key = buzz_core::managed_delivery::delivery_key(&event).unwrap();
        let tags = buzz_core::nip10::parse_thread_markers_from_parts(
            event.tags.iter().map(|tag| tag.as_slice()),
        );
        assert_eq!(tags.reply.as_ref(), expected.get(&key));
    }
    assert_eq!(queue.queued_event_count(&first_scope), 0);
    assert_eq!(queue.queued_event_count(&second_scope), 0);
    for agent in pool.agents.iter_mut().flatten() {
        agent.acp.shutdown().await;
    }
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn runtime_final_legacy_cached_and_durable_sessions_receive_fresh_contract() {
    checked_cli();
    for cached in [false, true] {
        let relay = MockRelay::start().await;
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("provider.jsonl");
        let observer = crate::observer::ObserverHandle::in_process();
        let mut agent = worker("plain", &log, &observer).await;
        let ctx = Arc::new(context(&relay, dir.path()));
        let (mut queue, scope) = queue(&relay, dir.path());
        let store = Arc::new(
            crate::session_store::SessionStore::open(
                dir.path().join("sessions"),
                &relay.url.replacen("http", "ws", 1),
                &relay.keys.public_key().to_hex(),
            )
            .unwrap(),
        );
        let legacy = crate::session_store::LiveSession {
            provider_background: None,
            reply_contract: None,
            lease: Uuid::nil(),
            adapter: "qwen-code".into(),
            session_id: "legacy-manual".into(),
            cwd: ctx.cwd.clone(),
            model: None,
            instance: store.instance(),
            worker_index: 0,
            departed: None,
            turns: 1,
            standing_context_sent: true,
            delivered_event_ids: vec![],
            hydrated_thread_roots: vec![],
        };
        store
            .claim(&scope, &crate::session_store::Expect::Missing, legacy)
            .unwrap();
        if cached {
            agent
                .state
                .sessions
                .insert(scope.clone(), "legacy-manual".into());
        }
        let mut pool = AgentPool::from_slots(vec![Some(agent)]);
        pool.set_session_store(Some(store.clone()));
        assert_eq!(
            background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
            1
        );
        let result = timeout(Duration::from_secs(15), pool.result_rx.recv())
            .await
            .unwrap()
            .unwrap();
        let lines = std::fs::read_to_string(&log).unwrap();
        assert!(lines.contains("session/new"));
        assert!(!lines.contains("session/resume"));
        let prompts: Vec<serde_json::Value> = lines
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .filter(|value: &serde_json::Value| value["method"] == "session/prompt")
            .collect();
        let text = prompts[0]["params"]["prompt"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Buzz publishes your final user-facing prose"));
        assert!(!text.contains("Final report command"));
        let crate::session_store::LoadOutcome::Found(record) = store.load(&scope) else {
            panic!("durable session")
        };
        assert_eq!(record.live.unwrap().reply_contract, ctx.reply_contract);
        assert!(states(&observer)
            .iter()
            .any(|state| state == "answer_confirmed"));
        let mut result = result;
        result.agent.acp.shutdown().await;
    }
}

#[tokio::test]
#[ignore = "requires rebuilt buzz-acp and buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn runtime_final_harness_startup_replays_before_unavailable_upgraded_provider() {
    checked_cli();
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(&relay, dir.path());
    let (mut queue, _) = queue(&relay, dir.path());
    let work = queue.flush_next().unwrap().work.unwrap();
    relay.losses.store(3, Ordering::SeqCst);
    assert!(crate::managed_work::publish_runtime_using(
        &binaries().join("buzz"),
        &ctx.rest_client,
        &work,
        "Saved answer",
        "answer"
    )
    .await
    .is_err());
    let wrapper = dir.path().join("b24-cowork-code-cli");
    let calls = dir.path().join("wrapper-called");
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\necho initialized > '{}'\nexit 1\n",
            calls.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(ctx.reply_build_file.as_ref().unwrap(), "0.26.7-b24.9").unwrap();
    let mut command = tokio::process::Command::new(binaries().join("buzz-acp"));
    command
        .current_dir(dir.path())
        .env_remove("BUZZ_AUTH_TAG")
        .env_remove("BUZZ_ACP_ALLOWED_RESPOND_TO")
        .env_remove("BUZZ_ACP_SESSION_POLICY")
        .env_remove("BUZZ_ACP_LAZY_POOL")
        .env_remove("BUZZ_ACP_IDLE_POOL_SLEEP")
        .args([
            "--relay-url",
            &relay.url,
            "--private-key",
            &relay.keys.secret_key().to_secret_hex(),
            "--agent-command",
            wrapper.to_str().unwrap(),
            "--reply-delivery",
            "runtime_final",
            "--reply-profile",
            "b24_0_26_7_8",
            "--reply-build-file",
            ctx.reply_build_file.as_ref().unwrap().to_str().unwrap(),
            "--agents",
            "1",
        ])
        .kill_on_drop(true);
    let output = timeout(Duration::from_secs(20), command.output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        !output.status.success(),
        "unavailable provider must still fail startup after replay"
    );
    assert!(
        calls.is_file(),
        "test must reach the provider-initialize failure after delivery replay"
    );
    let reports = relay.readback().await;
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].content, "Saved answer");
    let bodies = relay.bodies.lock().unwrap().clone();
    assert!(bodies.len() >= 2);
    assert!(bodies.iter().all(|body| body == &bodies[0]));
    assert!(
        crate::managed_work::confirmed_report(&ctx.rest_client, &work)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn runtime_final_confirmed_receipt_suppresses_provider_and_missing_final_notice() {
    checked_cli();
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("provider.jsonl");
    let observer = crate::observer::ObserverHandle::in_process();
    let agent = worker("plain", &log, &observer).await;
    let ctx = Arc::new(context(&relay, dir.path()));
    let (mut queue, _) = queue(&relay, dir.path());
    let batch = queue.flush_next().unwrap();
    let work = batch.work.as_ref().unwrap();
    crate::managed_work::publish_runtime_using(
        &binaries().join("buzz"),
        &ctx.rest_client,
        work,
        "Already delivered",
        "answer",
    )
    .await
    .unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    run_prompt_task(
        agent,
        Some(batch),
        None,
        ctx,
        tx,
        None,
        "confirmed-turn".into(),
        crate::queue::PromptRouting::default(),
    )
    .await;
    let mut result = rx.recv().await.unwrap();
    assert!(!std::fs::read_to_string(&log)
        .unwrap()
        .contains("session/prompt"));
    assert!(
        !states(&observer)
            .iter()
            .any(|state| state == "missing_final"),
        "a verified saved answer is not a missing final"
    );
    assert!(states(&observer)
        .iter()
        .any(|state| state == "answer_confirmed"));
    assert_eq!(relay.readback().await.len(), 1);
    assert_eq!(relay.bodies.lock().unwrap().len(), 1);
    result.agent.acp.shutdown().await;
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn runtime_final_cancel_during_pending_publish_persists_revocation_before_replay() {
    let cli = checked_cli();
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(&relay, dir.path());
    let (mut queue, scope) = queue(&relay, dir.path());
    let work = queue.flush_next().unwrap().work.unwrap();
    relay.hold.store(true, Ordering::SeqCst);
    let (rest, saved, program) = (ctx.rest_client.clone(), work.clone(), cli.clone());
    let publishing = tokio::spawn(async move {
        crate::runtime_delivery::complete_using(
            &program,
            &rest,
            &saved,
            &PromptOutcome::Ok(StopReason::EndTurn),
            Ok("Final answer".into()),
        )
        .await
    });
    timeout(Duration::from_secs(5), relay.accepted.notified())
        .await
        .unwrap();
    assert!(queue.cancel_logical_work(&scope));
    assert_eq!(
        timeout(Duration::from_secs(5), publishing)
            .await
            .unwrap()
            .unwrap(),
        crate::runtime_delivery::DeliveryOutcome::Cancelled
    );
    relay.hold.store(false, Ordering::SeqCst);
    let journal: serde_json::Value =
        serde_json::from_slice(&std::fs::read(work.journal_dir.join("journal.json")).unwrap())
            .unwrap();
    let record = journal["entries"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(record["runtime"]["revoked"], true);
    assert_eq!(record["attempts"], 1);
    assert_eq!(
        crate::runtime_delivery::replay_pending_using(&cli, &ctx.rest_client, &work.journal_dir)
            .await
            .unwrap(),
        0
    );
    assert_eq!(relay.bodies.lock().unwrap().len(), 1);
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn runtime_final_publication_identity_does_not_apply_kimi_work_deadline() {
    checked_cli();
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("provider.jsonl");
    let observer = crate::observer::ObserverHandle::in_process();
    let agent = worker("plain", &log, &observer).await;
    let ctx = Arc::new(context(&relay, dir.path()));
    let (_, scope) = queue(&relay, dir.path());
    let mut queue = EventQueue::new(crate::config::DedupMode::Queue)
        .with_runtime_delivery(dir.path().to_str().unwrap(), 0);
    queue.push(QueuedEvent {
        channel_id: relay.channel,
        scope,
        event: relay.parent.clone(),
        prompt_tag: "test".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        1
    );
    let mut result = timeout(Duration::from_secs(15), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(result.outcome, PromptOutcome::Ok(StopReason::EndTurn)),
        "runtime publication must use ordinary prompt bounds"
    );
    assert!(states(&observer)
        .iter()
        .any(|state| state == "answer_confirmed"));
    result.agent.acp.shutdown().await;
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn runtime_final_question_preserves_background_collection_and_its_own_delivery_key() {
    checked_cli();
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("provider.jsonl");
    let _finish_on_drop = FinishMarker(log.with_file_name("provider.jsonl.finish"));
    let observer = crate::observer::ObserverHandle::in_process();
    let agent = worker("background", &log, &observer).await;
    let ctx = Arc::new(context(&relay, dir.path()));
    let (_, scope) = queue(&relay, dir.path());
    let mut queue = EventQueue::new(crate::config::DedupMode::Queue)
        .with_runtime_delivery(dir.path().to_str().unwrap(), 0);
    queue.push(QueuedEvent {
        channel_id: relay.channel,
        scope: scope.clone(),
        event: relay.parent.clone(),
        prompt_tag: "test".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        1
    );
    let result = timeout(Duration::from_secs(15), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(result.agent.has_background_collection());
    let first_reports = relay.readback().await;
    assert_eq!(
        first_reports.len(),
        1,
        "a clarification must be delivered while the child waits"
    );
    assert_eq!(first_reports[0].content, "Which option?");
    let collection_key = result
        .agent
        .background_context
        .as_ref()
        .unwrap()
        .continuation
        .as_ref()
        .unwrap()
        .delivery_key;
    assert_ne!(
        buzz_core::managed_delivery::delivery_key(&first_reports[0]),
        Some(collection_key)
    );
    background_work_tests::apply_result(
        &mut pool,
        &mut queue,
        result,
        Some(&ctx.rest_client),
        &HashSet::new(),
    );
    pool.protect_background_work(&mut queue);
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        0,
        "an expired Kimi-style work deadline must not start collection before the child exits"
    );
    std::fs::write(log.with_file_name("provider.jsonl.finish"), "finish").unwrap();
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
    let mut result = timeout(Duration::from_secs(15), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(states(&observer)
        .iter()
        .any(|state| state == "answer_confirmed"));
    let reports = relay.readback().await;
    assert_eq!(reports.len(), 2);
    for report in &reports {
        assert!(
            report
                .tags
                .iter()
                .any(|tag| tag.as_slice() == ["p", relay.parent.pubkey.to_hex().as_str()]),
            "existing collection must retain its original callback author"
        );
    }
    assert!(reports
        .iter()
        .any(|event| buzz_core::managed_delivery::delivery_key(event) == Some(collection_key)));
    assert_eq!(
        std::fs::read_to_string(&log)
            .unwrap()
            .lines()
            .filter(|line| line.contains("session/prompt"))
            .count(),
        2,
        "only original work and existing result collection may prompt"
    );
    result.agent.acp.shutdown().await;
}

struct FinishMarker(PathBuf);
impl Drop for FinishMarker {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.0, "finish");
    }
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn runtime_final_isolated_journal_preserves_legacy_v2_bytes_through_replay() {
    let cli = checked_cli();
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(&relay, dir.path());
    let legacy_dir = dir.path().join(".buzz-deliveries");
    let legacy = relay
        .command(&cli, &legacy_dir, Uuid::new_v4(), "Legacy answer")
        .output()
        .await
        .unwrap();
    assert!(legacy.status.success());
    let original_legacy = std::fs::read(legacy_dir.join("journal.json")).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&original_legacy).unwrap()["version"],
        2
    );
    let (mut queue, _) = queue(&relay, dir.path());
    let work = queue.flush_next().unwrap().work.unwrap();
    assert_eq!(
        work.journal_dir,
        dir.path().join(".buzz-runtime-deliveries"),
        "runtime schema must not upgrade the shared legacy journal"
    );
    relay.losses.store(3, Ordering::SeqCst);
    assert!(crate::managed_work::publish_runtime_using(
        &cli,
        &ctx.rest_client,
        &work,
        "Runtime answer",
        "answer"
    )
    .await
    .is_err());
    assert_eq!(
        crate::runtime_delivery::replay_pending_using(&cli, &ctx.rest_client, &work.journal_dir)
            .await
            .unwrap(),
        1
    );
    let direct = tokio::process::Command::new(&cli)
        .current_dir(dir.path())
        .env_remove("BUZZ_AUTH_TAG")
        .env_remove("BUZZ_DELIVERY_JOURNAL_DIR")
        .env("BUZZ_RELAY_URL", &relay.url)
        .env("BUZZ_PRIVATE_KEY", relay.keys.secret_key().to_secret_hex())
        .args([
            "messages",
            "deliver",
            "--runtime-purpose",
            "answer",
            "--delivery-key",
            &Uuid::new_v4().to_string(),
            "--channel",
            &relay.channel.to_string(),
            "--reply-to",
            &relay.parent.id.to_hex(),
            "--content",
            "Default runtime directory",
        ])
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(direct.status.success());
    assert_eq!(
        std::fs::read(legacy_dir.join("journal.json")).unwrap(),
        original_legacy,
        "old CLI/harness readers must retain identical v2 bytes"
    );
    let runtime: serde_json::Value =
        serde_json::from_slice(&std::fs::read(work.journal_dir.join("journal.json")).unwrap())
            .unwrap();
    assert_eq!(runtime["version"], 3);
    assert_eq!(relay.readback().await.len(), 3);
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI on PATH and BUZZ_TEST_BIN_DIR"]
async fn runtime_final_revocation_during_session_setup_prevents_late_provider_prompt() {
    checked_cli();
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("provider.jsonl");
    let observer = crate::observer::ObserverHandle::in_process();
    let agent = worker("cancel_setup", &log, &observer).await;
    let ctx = Arc::new(context(&relay, dir.path()));
    let (mut queue, scope) = queue(&relay, dir.path());
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        1
    );
    timeout(Duration::from_secs(3), async {
        loop {
            if std::fs::read_to_string(&log)
                .unwrap()
                .contains("session/new")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(queue.cancel_logical_work(&scope));
    let mut result = timeout(Duration::from_secs(5), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        !std::fs::read_to_string(&log)
            .unwrap()
            .contains("session/prompt"),
        "revoked accepted work cannot start a later provider prompt after setup"
    );
    assert!(result.batch.is_none());
    assert!(relay.readback().await.is_empty());
    result.agent.acp.shutdown().await;
}

#[path = "runtime_final_review_tests.rs"]
mod review_tests;
