#![cfg(unix)]
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

struct ProcessGroup(Option<u32>);
impl ProcessGroup {
    fn kill(&mut self) {
        if let Some(pid) = self.0.take() {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
    }
}
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        self.kill();
    }
}

#[tokio::test]
async fn real_npm_service_registers_before_tools_and_excludes_actual_work() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("finish");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/late_service");
    let generation = uuid::Uuid::new_v4();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_buzz-acp"))
        .arg("__mcp-service")
        .arg(directory.path())
        .arg(generation.to_string())
        .arg(uuid::Uuid::new_v4().to_string())
        .args(["npm", "--prefix"])
        .arg(fixture)
        .arg("start")
        .env("STALL_TEST_MARKER", &marker)
        .env("npm_config_loglevel", "notice")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .process_group(0)
        .spawn()
        .unwrap();
    let group = child.id().unwrap();
    let mut group = ProcessGroup(Some(group));
    let result = async {
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap()).lines();
        input.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\"}}\n").await.unwrap();
        let mut startup_output = Vec::new();
        let response = loop {
            let line = output
                .next_line()
                .await
                .unwrap()
                .expect("MCP initialize response");
            if serde_json::from_str::<serde_json::Value>(&line).is_ok() {
                break line;
            }
            startup_output.push(line);
        };
        assert!(
            startup_output
                .iter()
                .any(|line| line.contains("node ./src/index.mjs")),
            "npm startup output must pass through unchanged"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&response).unwrap()["id"],
            1
        );
        let records: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|value| value == "json")
            })
            .collect();
        assert_eq!(
            records.len(),
            1,
            "registration must precede initialize delivery"
        );
        let record: serde_json::Value =
            serde_json::from_slice(&std::fs::read(records[0].path()).unwrap()).unwrap();
        assert_eq!(record["generation"], generation.to_string());
        let instances = record["instances"].as_object().unwrap();
        assert!(instances.values().any(|instance| instance["command"]
            .as_str()
            .unwrap()
            .contains("node ./src/index.mjs")));
        input.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"start_work\",\"arguments\":{}}}\n").await.unwrap();
        assert!(output.next_line().await.unwrap().is_some());
        while !marker.with_extension("started").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let task = std::fs::read_to_string(marker.with_extension("started")).unwrap();
        assert!(
            !instances.contains_key(task.trim()),
            "tool work must never enter service provenance"
        );
        std::fs::write(&marker, "finish").unwrap();
        while !marker.with_extension("result").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        drop(input);
    };
    let outcome = tokio::time::timeout(Duration::from_secs(15), result).await;
    group.kill();
    let _ = child.wait().await;
    outcome.unwrap();
}
