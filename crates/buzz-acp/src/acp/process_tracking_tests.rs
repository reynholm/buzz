//! Real ACP handshakes with inert local child processes; no provider or relay.
use super::*;

const ADAPTER: &str = r#"
import json, subprocess, sys
provider = None
provider_program = '''
import subprocess, sys
children = []
for line in sys.stdin:
    if line.strip() == 'start':
        child = subprocess.Popen(['/bin/sleep', '60'])
        children.append(child)
        print(child.pid, flush=True)
    elif line.strip() == 'stop':
        for child in children:
            child.kill()
            child.wait()
        children.clear()
        print('stopped', flush=True)
'''
children = []
providers = []
for line in sys.stdin:
    req = json.loads(line)
    method = req['method']
    if method == 'initialize':
        result = {'protocolVersion': 1, 'agentCapabilities': {}}
    elif method == 'session/new':
        provider = subprocess.Popen([sys.executable, '-u', '-c', provider_program], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        providers.append(provider)
        result = {'sessionId': 'local-test', 'providerPid': provider.pid}
    elif method == 'session/prompt':
        text = req['params']['prompt'][0]['text']
        if text == 'direct':
            child = subprocess.Popen(['/bin/sleep', '60'])
            children.append(child)
        elif text == 'nested':
            provider.stdin.write('start\n')
            provider.stdin.flush()
            provider.stdout.readline()
        elif text == 'stop':
            for child in children:
                child.kill()
                child.wait()
            children.clear()
            provider.stdin.write('stop\n')
            provider.stdin.flush()
            provider.stdout.readline()
        result = {'stopReason': 'end_turn'}
    else:
        continue
    print(json.dumps({'jsonrpc': '2.0', 'id': req['id'], 'result': result}), flush=True)
"#;

async fn local_adapter() -> AcpClient {
    let mut client = AcpClient::spawn(
        "python3",
        &["-u".into(), "-c".into(), ADAPTER.into()],
        &[],
        false,
    )
    .await
    .expect("spawn inert adapter");
    client.initialize().await.expect("initialize adapter");
    client
        .session_new("/tmp", vec![], None, None)
        .await
        .expect("create session");
    client
}

async fn prompt(client: &mut AcpClient, text: &str) {
    assert_eq!(
        client
            .session_prompt_with_idle_timeout(
                "local-test",
                text,
                std::time::Duration::from_secs(2),
                std::time::Duration::from_secs(5)
            )
            .await
            .expect("complete prompt"),
        StopReason::EndTurn
    );
}

#[tokio::test]
async fn session_new_provider_alone_does_not_keep_worker_awake() {
    let mut client = local_adapter().await;
    prompt(&mut client, "idle").await;
    let blocking = client
        .background_child_pids()
        .expect("bounded process snapshot");
    client.shutdown().await;
    assert!(
        blocking.is_empty(),
        "persistent session provider is not turn work: {blocking:?}"
    );
}

#[tokio::test]
async fn prompt_work_is_tracked_under_worker_and_resident_provider() {
    let mut client = local_adapter().await;
    prompt(&mut client, "direct").await;
    let direct = client
        .background_child_pids()
        .expect("bounded process snapshot");
    prompt(&mut client, "nested").await;
    let both = client
        .background_child_pids()
        .expect("bounded process snapshot");
    prompt(&mut client, "stop").await;
    let stopped = client
        .background_child_pids()
        .expect("bounded process snapshot");
    client.shutdown().await;
    assert_eq!(direct.len(), 1, "only direct turn work blocks: {direct:?}");
    assert_eq!(
        both.len(),
        2,
        "provider's own turn work must also block: {both:?}"
    );
    assert!(direct.is_subset(&both));
    assert!(
        stopped.is_empty(),
        "completed work releases the hold: {stopped:?}"
    );
}

#[tokio::test]
async fn another_session_does_not_adopt_preexisting_background_work() {
    let mut client = local_adapter().await;
    prompt(&mut client, "direct").await;
    prompt(&mut client, "nested").await;
    let before = client
        .background_child_pids()
        .expect("bounded process snapshot");
    client
        .session_new("/tmp", vec![], None, None)
        .await
        .expect("create another session");
    let after = client
        .background_child_pids()
        .expect("bounded process snapshot");
    client.shutdown().await;
    assert_eq!(before.len(), 2);
    assert_eq!(
        after, before,
        "a new session must preserve prior work ownership"
    );
}
