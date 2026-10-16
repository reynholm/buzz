//! Opt-in provider checks: the remembered value must survive adapter death.

use std::time::Duration;

use crate::acp::AcpClient;
use crate::observer::ObserverHandle;

async fn spawn_adapter(command: &str, observer: &ObserverHandle) -> AcpClient {
    // Provider authentication stays in the normal runtime environment. These
    // tests never grant the model a managed Buzz identity or relay credentials.
    let mut args = Vec::new();
    for key in ["BUZZ_PRIVATE_KEY", "BUZZ_AUTH_TAG", "BUZZ_RELAY_URL"] {
        args.extend(["-u".to_owned(), key.to_owned()]);
    }
    args.push(command.to_owned());
    let mut acp = AcpClient::spawn("/usr/bin/env", &args, &[], false)
        .await
        .expect("spawn installed adapter");
    acp.set_observer(Some(observer.clone()), 0);
    acp.initialize().await.expect("initialize real adapter");
    assert!(acp.resume_supported(), "adapter must advertise resume");
    acp
}

async fn real_adapter_remembers(variable: &str, default_command: &str) {
    let command = std::env::var(variable).unwrap_or_else(|_| default_command.to_owned());
    let workspace = tempfile::tempdir().expect("temporary provider workspace");
    let cwd = workspace.path().to_str().expect("UTF-8 temporary path");
    let word = format!("GRANITE_{}", uuid::Uuid::new_v4().simple());
    let observer = ObserverHandle::in_process();
    let mut first = spawn_adapter(&command, &observer).await;
    let sid = first
        .session_new(cwd, vec![], None, None)
        .await
        .expect("create provider session");
    first
        .session_prompt_with_idle_timeout(
            &sid,
            &format!("Remember the code word {word}. Reply only ACK. Do not use tools."),
            Duration::from_secs(90),
            Duration::from_secs(180),
        )
        .await
        .expect("first provider turn");
    // End the actual adapter and its process group before constructing another.
    first.shutdown().await;
    drop(first);

    let mut second = spawn_adapter(&command, &observer).await;
    second
        .session_resume(&sid, cwd, vec![], None, None)
        .await
        .expect("resume in a fresh adapter process");
    let after_resume = observer.snapshot().last().map_or(0, |event| event.seq);
    let result = second
        .session_prompt_with_idle_timeout(
            &sid,
            "What code word did I give you in the previous message? Reply only with that code word. Do not read files or use tools.",
            Duration::from_secs(90),
            Duration::from_secs(180),
        )
        .await;
    second.shutdown().await;
    result.expect("second provider turn");
    let answer: String = observer
        .snapshot()
        .iter()
        .filter(|event| event.seq > after_resume && event.kind == "acp_read")
        .map(|event| &event.payload["params"]["update"])
        .filter(|update| update["sessionUpdate"] == "agent_message_chunk")
        .filter_map(|update| update["content"]["text"].as_str())
        .collect();
    assert!(
        answer.contains(&word),
        "resumed answer lost the random code word: {answer}"
    );
}

#[tokio::test]
#[ignore = "requires an installed authenticated Claude adapter; makes provider calls"]
async fn real_claude_resume_remembers_previous_turn() {
    real_adapter_remembers("BUZZ_TEST_CLAUDE_ACP", "claude-agent-acp").await;
}

#[tokio::test]
#[ignore = "requires an installed authenticated Codex adapter; makes provider calls"]
async fn real_codex_resume_remembers_previous_turn() {
    real_adapter_remembers("BUZZ_TEST_CODEX_ACP", "codex-acp").await;
}
