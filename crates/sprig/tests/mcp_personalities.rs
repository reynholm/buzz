#![cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::test]
async fn multicall_target_runs_injected_mcp_transport() {
    assert_internal_transport(false).await;
}

#[tokio::test]
async fn multicall_target_runs_native_codex_proxy() {
    assert_internal_transport(true).await;
}

async fn assert_internal_transport(native: bool) {
    let directory = tempfile::tempdir().unwrap();
    let adapter = directory.path().join("adapter");
    std::fs::write(
        &adapter,
        r#"#!/usr/bin/env python3
import json, sys
for line in sys.stdin:
    req = json.loads(line)
    print(json.dumps({'jsonrpc':'2.0', 'id':req['id'], 'result':req.get('params', {})}), flush=True)
"#,
    )
    .unwrap();
    std::fs::set_permissions(&adapter, std::fs::Permissions::from_mode(0o700)).unwrap();
    let spec = directory.path().join("codex.launch");
    std::fs::write(&spec, serde_json::to_vec(&serde_json::json!({
        "adapter":adapter, "original_codex_path":null,
        "servers":{"native":{"command":"/bin/registered-service", "args":["__mcp-service","registry"], "env":{"KEEP":"kept"}}}
    })).unwrap()).unwrap();
    // Linux current_exe() resolves buzz-acp -> sprig to this exact target.
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_sprig"));
    if native {
        command
            .arg("__codex-service-proxy")
            .arg(&spec)
            .arg("app-server");
    } else {
        command
            .arg("__mcp-service")
            .arg(directory.path())
            .args([
                "00000000-0000-4000-8000-000000000001",
                "00000000-0000-4000-8000-000000000002",
            ])
            .arg(&adapter);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(10), async {
            let request = if native {
                serde_json::json!({"jsonrpc":"2.0", "id":1, "method":"thread/start", "params":{"config":{"mcp_servers":{}}}})
            } else {
                serde_json::json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{"probe":"injected"}})
            };
            let mut input = child.stdin.take().unwrap();
            input.write_all(format!("{request}\n").as_bytes()).await.unwrap();
            let mut output = BufReader::new(child.stdout.take().unwrap()).lines();
            let line = output.next_line().await.unwrap().expect("Sprig must run the internal ACP transport");
            let response: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(response["id"], 1);
            if native {
                assert_eq!(response["result"]["config"]["mcp_servers"]["native"]["command"], "/bin/registered-service");
                assert_eq!(response["result"]["config"]["mcp_servers"]["native"]["env"]["KEEP"], "kept");
            } else {
                assert_eq!(response["result"]["probe"], "injected");
                assert!(std::fs::read_dir(directory.path()).unwrap().filter_map(Result::ok).any(|entry| entry.path().extension().is_some_and(|ext| ext == "json")), "registration must precede the initialize response");
            }
            drop(input);
            assert!(child.wait().await.unwrap().success());
        }).await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    result.unwrap();
}
