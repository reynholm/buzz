//! Actual adapter and external CLI through the operator-wrapper contract.
use crate::acp::{AcpClient, EnvVar, McpServer};
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn trace(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

async fn version(program: &Path, argument: &str) -> String {
    let mut command = tokio::process::Command::new(program);
    command.arg(argument);
    tokio::task::spawn_blocking(move || {
        crate::process_tree::bounded_output(command, Duration::from_secs(10), 4096)
    })
    .await
    .unwrap()
    .unwrap()
    .trim()
    .to_owned()
}

#[tokio::test]
#[ignore = "Requires adapter 2.2.2, external Codex 0.164.0, npm and BUZZ_TEST_BIN_DIR"]
async fn real_launcher_wrapper_preserves_proxy_and_external_cli() {
    let adapter = PathBuf::from(std::env::var("BUZZ_TEST_CODEX_ADAPTER_222").unwrap());
    let external =
        std::fs::canonicalize(std::env::var("BUZZ_TEST_CODEX_EXTERNAL_0164").unwrap()).unwrap();
    assert!(
        std::env::var_os("CODEX_PATH").is_none(),
        "clear ambient CODEX_PATH for the fallback case"
    );
    assert_eq!(
        version(&adapter, "--version").await,
        "@agentclientprotocol/codex-acp 2.2.2"
    );
    assert_eq!(version(&external, "-V").await, "codex-cli 0.164.0");
    for explicit in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let initialized = dir.path().join("initialized");
        let trace_path = dir.path().join("launch.jsonl");
        let launcher = dir.path().join("codex-acp");
        let fallback = dir.path().join("external-default");
        let configured = dir.path().join("external-configured");
        let fixture_source = include_str!("../../tests/fixtures/codex_launcher.py");
        for path in [&launcher, &fallback, &configured] {
            std::fs::write(path, fixture_source).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/late_service");
        let mut config: serde_json::Value =
            serde_json::from_str(&std::env::var("CODEX_CONFIG").unwrap_or_else(|_| "{}".into()))
                .unwrap();
        config["mcp_servers"]["native_launcher"] = serde_json::json!({
            "command":"npm", "args":["--silent","--prefix",fixture,"start"],
            "env":{"STALL_TEST_INITIALIZED":initialized,"STALL_TEST_LABEL":"native"}
        });
        let mut launch = vec![
            ("CODEX_CONFIG".into(), config.to_string()),
            (
                "BUZZ_TEST_LAUNCHER_ADAPTER".into(),
                adapter.to_string_lossy().into_owned(),
            ),
            (
                "BUZZ_TEST_LAUNCHER_REAL_CLI".into(),
                external.to_string_lossy().into_owned(),
            ),
            (
                "BUZZ_TEST_LAUNCHER_FALLBACK".into(),
                fallback.to_string_lossy().into_owned(),
            ),
            (
                "BUZZ_TEST_LAUNCHER_TRACE".into(),
                trace_path.to_string_lossy().into_owned(),
            ),
        ];
        if explicit {
            launch.push((
                "CODEX_PATH".into(),
                configured.to_string_lossy().into_owned(),
            ));
        }
        let mut client =
            AcpClient::spawn_with_env(launcher.to_str().unwrap(), &[], &[], false, &launch)
                .await
                .unwrap();
        let initialized_adapter = client.initialize().await.unwrap();
        assert_eq!(initialized_adapter["agentInfo"]["version"], "2.2.2");
        let records = trace(&trace_path);
        let selected = records
            .iter()
            .find(|record| record["kind"] == "launcher" && record["args"] == serde_json::json!([]))
            .unwrap()["selected"]
            .as_str()
            .unwrap();
        assert!(
            Path::new(selected)
                .file_name()
                .is_some_and(|name| name == "codex-service-proxy"),
            "launcher must preserve the inherited MCP proxy; trace={records:?}"
        );
        assert!(std::fs::read_to_string(selected)
            .unwrap()
            .contains("__codex-service-proxy"));
        let app_server = records
            .iter()
            .find(|record| record["kind"] == "external" && record["args"][0] == "app-server")
            .unwrap();
        assert_eq!(app_server["target"], external.to_string_lossy().as_ref());
        let external_pid = app_server["pid"].as_u64().unwrap() as u32;
        let tree = crate::process_tree::snapshot().unwrap();
        let root = client.process_id().unwrap();
        let parents: Vec<_> = tree
            .descendant_pids(root)
            .into_iter()
            .chain([root])
            .collect();
        let proxy = parents
            .iter()
            .flat_map(|pid| tree.children(*pid))
            .find(|child| child.command.contains(" __codex-service-proxy "))
            .expect("actual proxy process must be live");
        assert!(
            tree.descendant_pids(proxy.pid).contains(&external_pid),
            "external CLI must execute below the actual MCP proxy"
        );
        assert!(
            parents
                .iter()
                .flat_map(|pid| tree.children(*pid))
                .any(|child| child.pid == external_pid
                    && child.command.starts_with(external.to_str().unwrap())),
            "recording wrapper must exec the real configured external binary"
        );
        let expected_cli = if explicit { &configured } else { &fallback };
        assert!(
            records.iter().any(|record| record["kind"] == "launcher"
                && record["args"][0] == "cli"
                && record["args"][1] == "app-server"
                && record["selected"] == expected_cli.to_string_lossy().as_ref()),
            "proxy must restore the configured CLI or wrapper fallback"
        );
        client.capture_startup_children();
        client.process_tree_snapshots.push_back(None);
        client
            .session_new(
                dir.path().to_str().unwrap(),
                vec![McpServer {
                    name: "injected_launcher".into(),
                    command: "npm".into(),
                    args: vec![
                        "--silent".into(),
                        "--prefix".into(),
                        fixture.to_string_lossy().into_owned(),
                        "start".into(),
                    ],
                    env: vec![
                        EnvVar {
                            name: "STALL_TEST_INITIALIZED".into(),
                            value: initialized.to_string_lossy().into_owned(),
                        },
                        EnvVar {
                            name: "STALL_TEST_LABEL".into(),
                            value: "injected".into(),
                        },
                    ],
                }],
                None,
                None,
            )
            .await
            .unwrap();
        let receipts = || {
            std::fs::read_to_string(&initialized)
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect::<HashSet<_>>()
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        let mut children = client.background_child_pids();
        while (receipts().len() < 2 || children.as_ref().is_none_or(|pids| !pids.is_empty()))
            && tokio::time::Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(30)).await;
            children = client.background_child_pids();
        }
        let observed = receipts();
        eprintln!("launcher explicit={explicit} adapter=2.2.2 external=0.164.0 proxy_pid={} external_pid={external_pid} receipts={observed:?} trace={records:?}", proxy.pid);
        client.shutdown().await;
        assert_eq!(
            observed,
            HashSet::from(["native".into(), "injected".into()])
        );
        assert_eq!(
            children,
            Some(HashSet::new()),
            "both real services must have provenance through the external-CLI wrapper"
        );
    }
}
