#[tokio::test]
async fn initialize_reads_resume_capability() {
    let script = r#"
            read -t 2 _init
            echo '{"jsonrpc":"2.0","id":0,"result":{"protocolVersion":1,"agentCapabilities":{"sessionCapabilities":{"resume":{}}}}}'
            sleep 1
        "#;
    let mut client = spawn_script(script).await;
    client.initialize().await.expect("initialize");
    assert!(client.resume_supported());

    let script = r#"
            read -t 2 _init
            echo '{"jsonrpc":"2.0","id":0,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":false}}}'
            sleep 1
        "#;
    let mut client = spawn_script(script).await;
    client.initialize().await.expect("initialize");
    assert!(!client.resume_supported());
}

#[tokio::test]
async fn session_resume_sends_same_params_as_new() {
    let script = r#"
            read -t 2 _init
            echo '{"jsonrpc":"2.0","id":0,"result":{"protocolVersion":1,"agentCapabilities":{"sessionCapabilities":{"resume":{}}}}}'
            read -t 2 REQ
            echo '{"jsonrpc":"2.0","id":1,"result":{"_receivedRequest":'"$REQ"'}}'
            sleep 1
        "#;
    let mut client = spawn_script(script).await;
    client.initialize().await.expect("initialize");
    let resp = client
        .session_resume(
            "ses_prev",
            "/tmp",
            vec![],
            Some(SystemPromptTransport::ClaudeMeta("be brief")),
            Some("Fizz"),
        )
        .await
        .expect("session_resume");
    assert_eq!(resp.session_id, "ses_prev");
    let request = &resp.raw["_receivedRequest"];
    assert_eq!(request["method"], "session/resume");
    assert_eq!(request["params"]["sessionId"], "ses_prev");
    assert_eq!(request["params"]["cwd"], "/tmp");
    assert_eq!(request["params"]["_meta"]["systemPrompt"]["append"], "be brief");
    assert_eq!(request["params"]["_meta"]["sessionTitle"], "Fizz");
}

#[tokio::test]
async fn resume_not_found_maps_to_code() {
    let script = r#"
            read -t 2 _init
            echo '{"jsonrpc":"2.0","id":0,"result":{"protocolVersion":1,"agentCapabilities":{"sessionCapabilities":{"resume":{}}}}}'
            read -t 2 _REQ
            echo '{"jsonrpc":"2.0","id":1,"error":{"code":-32002,"message":"Resource not found: ses_gone"}}'
            sleep 1
        "#;
    let mut client = spawn_script(script).await;
    client.initialize().await.expect("initialize");
    let Err(err) = client
        .session_resume("ses_gone", "/tmp", vec![], None, None)
        .await
    else {
        panic!("unknown session must fail to resume");
    };
    assert_eq!(resume_failure_code(&err), "not_found");
    assert_eq!(
        resume_failure_code(&AcpError::AgentError {
            code: -32603,
            message: "internal".into()
        }),
        "rpc_-32603"
    );
    assert_eq!(
        resume_failure_code(&AcpError::Timeout(std::time::Duration::from_secs(60))),
        "timeout"
    );
    assert_eq!(
        resume_failure_code(&AcpError::Protocol("bad".into())),
        "protocol"
    );
}
