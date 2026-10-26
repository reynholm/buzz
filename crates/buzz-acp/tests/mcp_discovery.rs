#![cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

fn running(pid: i32) -> bool {
    let output = std::process::Command::new("/bin/ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let state = String::from_utf8_lossy(&output.stdout);
    !state.trim().is_empty() && !state.trim().starts_with('Z')
}

#[tokio::test]
async fn native_discovery_retires_descendants_on_every_output_exit() {
    for mode in ["timeout", "overflow", "failure", "invalid", "success"] {
        let dir = tempfile::tempdir().unwrap();
        let adapter = dir.path().join("codex-acp");
        let marker = dir.path().join("children");
        std::fs::write(&adapter, r#"#!/usr/bin/env python3
import json, os, pathlib, subprocess, sys, time
if len(sys.argv) > 1 and sys.argv[1] == 'cli':
    marker = pathlib.Path(os.environ['DISCOVERY_CHILDREN'])
    subprocess.Popen(['/bin/sh', '-c', 'sleep 60 & printf "%s %s" "$$" "$!" > "$1"; wait', 'fixture', str(marker)], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    while not marker.exists() or not marker.read_text(): time.sleep(0.001)
    mode = os.environ['DISCOVERY_MODE']
    if mode == 'timeout': time.sleep(60)
    if mode == 'overflow': print('x' * (1024 * 1024 + 1), flush=True); time.sleep(60)
    if mode == 'failure': sys.exit(1)
    print('[]' if mode == 'success' else 'invalid', flush=True)
else:
    for line in sys.stdin:
        req = json.loads(line)
        if 'id' not in req: continue
        result = {'protocolVersion':1, 'agentCapabilities':{}} if req['method'] == 'initialize' else {'sessionId':'fixture'}
        print(json.dumps({'jsonrpc':'2.0', 'id':req['id'], 'result':result}), flush=True)
"#).unwrap();
        std::fs::set_permissions(&adapter, std::fs::Permissions::from_mode(0o700)).unwrap();
        let output = tokio::time::timeout(
            Duration::from_secs(15),
            tokio::process::Command::new(env!("CARGO_BIN_EXE_buzz-acp"))
                .args(["models", "--agent-command"])
                .arg(&adapter)
                .arg("--json")
                .env("DISCOVERY_CHILDREN", &marker)
                .env("DISCOVERY_MODE", mode)
                .env_remove("CODEX_CONFIG")
                .env_remove("CODEX_PATH")
                .env_remove("BUZZ_ACP_LAUNCH_PREFIX")
                .env_remove("BUZZ_ACP_AGENT_ARGS")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        let pids: Vec<i32> = std::fs::read_to_string(marker)
            .unwrap()
            .split_whitespace()
            .map(|pid| pid.parse().unwrap())
            .collect();
        assert_eq!(
            pids.len(),
            2,
            "fixture must start both discovery descendants"
        );
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while pids.iter().any(|pid| running(*pid)) && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let leaked = pids.iter().any(|pid| running(*pid));
        // Always clean the fixture before reporting a failure on the old code.
        if leaked {
            for pid in pids.iter().rev() {
                let _ = nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(*pid),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
        }
        assert!(
            output.status.success(),
            "models failed in {mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !leaked,
            "native discovery left its descendant running after {mode}"
        );
    }
}
