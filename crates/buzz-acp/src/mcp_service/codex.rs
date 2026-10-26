//! Preserve native Codex MCP configuration while replacing its launch boundary.
use super::*;
use serde_json::{json, Value};

pub(super) fn merge_config(base: &mut Value, overlay: &Value) {
    if let (Some(base), Some(overlay)) = (base.as_object_mut(), overlay.as_object()) {
        for (key, value) in overlay {
            merge_config(base.entry(key.clone()).or_insert(Value::Null), value);
        }
    } else {
        *base = overlay.clone();
    }
}

/// Enumerate effective native servers using the adapter's own bundled CLI.
/// Only command/args change; native cwd, env, timeouts and tool policy survive.
pub(crate) async fn wrap_native_config(
    registry: &ServiceRegistry,
    adapter: &str,
    launch: &mut tokio::process::Command,
) -> Result<()> {
    let overrides: Vec<_> = launch
        .as_std()
        .get_envs()
        .map(|(key, value)| (key.to_owned(), value.map(ToOwned::to_owned)))
        .collect();
    let configured = overrides
        .iter()
        .find(|(key, _)| key == "CODEX_CONFIG")
        .and_then(|(_, value)| value.as_ref())
        .cloned()
        .or_else(|| std::env::var_os("CODEX_CONFIG"));
    let mut config: Value = match configured {
        Some(value) => serde_json::from_str(&value.to_string_lossy())?,
        None => json!({}),
    };
    config
        .as_object()
        .context("Codex configuration must be an object")?;
    let mut args = vec![
        "cli".to_owned(),
        "mcp".into(),
        "list".into(),
        "--json".into(),
    ];
    if crate::config::normalize_agent_command_identity(adapter) == "codex" {
        args.remove(0);
    }
    let mut discovery = crate::acp::launch::command(adapter, &args)?;
    for (key, value) in overrides {
        if let Some(value) = value {
            discovery.env(key, value);
        } else {
            discovery.env_remove(key);
        }
    }
    let output = super::discovery::output(discovery)
        .await
        .context("effective native MCP configuration discovery failed")?;
    let servers: Vec<Value> = serde_json::from_str(&output)?;
    let registered = rewrite_native_servers(registry, &mut config, &servers)?;
    super::codex_proxy::install(registry, adapter, launch, registered)
}

fn rewrite_native_servers(
    registry: &ServiceRegistry,
    config: &mut Value,
    servers: &[Value],
) -> Result<serde_json::Map<String, Value>> {
    // The CLI reads native files and project layers. CODEX_CONFIG belongs to
    // the ACP adapter; merge its MCP overrides here, without putting env values
    // or provider credentials on a discovery process's command line.
    let mut effective = serde_json::Map::new();
    for server in servers {
        let name = server["name"].as_str().context("native MCP name missing")?;
        let mut value = server["transport"].clone();
        if let Some(object) = value.as_object_mut() {
            object.remove("type");
            object.retain(|_, value| !value.is_null());
            object.insert("enabled".into(), server["enabled"].clone());
            for key in ["startup_timeout_sec", "tool_timeout_sec"] {
                if let Some(value) = server.get(key).filter(|value| !value.is_null()) {
                    object.insert(key.into(), value.clone());
                }
            }
            effective.insert(name.to_owned(), value);
        }
    }
    if let Some(overrides) = config["mcp_servers"].as_object() {
        for (name, value) in overrides {
            let entry = effective
                .entry(name.clone())
                .or_insert_with(|| json!({"enabled":true}));
            merge_config(entry, value);
        }
    }
    let mut registered = serde_json::Map::new();
    for (name, mut server) in effective {
        if server["enabled"] == false || server.get("url").is_some() {
            continue;
        }
        let Some(command) = server["command"].as_str() else {
            continue;
        };
        let original: Vec<String> = match server.get("args") {
            Some(args) => serde_json::from_value(args.clone())?,
            None => vec![],
        };
        let mut args = registry.wrapper_args(Uuid::new_v4());
        args.push(command.into());
        args.extend(original);
        config["mcp_servers"][&name]["command"] = json!(registry.executable);
        config["mcp_servers"][&name]["args"] = json!(args);
        server["command"] = json!(registry.executable);
        server["args"] = json!(args);
        registered.insert(name, server);
    }
    Ok(registered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelling_native_discovery_kills_its_descendant() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::Duration;
        let directory = tempfile::tempdir().unwrap();
        let adapter = directory.path().join("codex-acp");
        let marker = directory.path().join("pid");
        std::fs::write(&adapter, r#"#!/usr/bin/env python3
import os, pathlib, subprocess, time
child = subprocess.Popen(['/bin/sleep', '60'], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
pathlib.Path(os.environ['DISCOVERY_CHILD']).write_text(str(child.pid))
time.sleep(60)
"#).unwrap();
        std::fs::set_permissions(&adapter, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut launch = tokio::process::Command::new(&adapter);
        launch.env("DISCOVERY_CHILD", &marker);
        let task = tokio::spawn(async move {
            let registry = ServiceRegistry::new("/bin/wrapper".into()).unwrap();
            wrap_native_config(&registry, adapter.to_str().unwrap(), &mut launch).await
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            while std::fs::read_to_string(&marker)
                .unwrap_or_default()
                .is_empty()
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let pid =
            nix::unistd::Pid::from_raw(std::fs::read_to_string(marker).unwrap().parse().unwrap());
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let gone = tokio::time::timeout(Duration::from_secs(2), async {
            while nix::sys::signal::kill(pid, None) != Err(nix::errno::Errno::ESRCH) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok();
        if !gone {
            let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
        }
        assert!(
            gone,
            "cancelling startup must kill the discovery descendant"
        );
    }

    #[test]
    fn native_server_overrides_preserve_environment_and_policy() {
        let registry = ServiceRegistry::new("/bin/wrapper".into()).unwrap();
        let mut config = json!({"model":"test", "mcp_servers":{"late":{"env":{"TOKEN":"secret"},"tool_timeout_sec":42}}});
        let original = config.clone();
        rewrite_native_servers(&registry, &mut config, &[
            json!({"name":"late","enabled":true,"transport":{"type":"stdio","command":"npm","args":["start"]}}),
            json!({"name":"remote","enabled":true,"transport":{"type":"streamable_http"}}),
            json!({"name":"disabled","enabled":false,"transport":{"type":"stdio"}}),
        ]).unwrap();
        assert_eq!(config["model"], original["model"]);
        assert_eq!(
            config["mcp_servers"]["late"]["env"],
            original["mcp_servers"]["late"]["env"]
        );
        assert_eq!(config["mcp_servers"]["late"]["tool_timeout_sec"], 42);
        assert_eq!(config["mcp_servers"]["late"]["command"], "/bin/wrapper");
        assert_eq!(config["mcp_servers"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn explicit_native_config_wins_without_exposing_environment_as_cli_arguments() {
        let registry = ServiceRegistry::new("/bin/wrapper".into()).unwrap();
        let mut config = json!({"mcp_servers":{"late":{"command":"override", "args":["literal"], "env":{"TOKEN":"secret"}}}});
        rewrite_native_servers(&registry, &mut config, &[
            json!({"name":"late","enabled":true,"transport":{"type":"stdio","command":"npm","args":["start"]}}),
        ]).unwrap();
        assert_eq!(config["mcp_servers"]["late"]["args"][4], "override");
        assert_eq!(config["mcp_servers"]["late"]["args"][5], "literal");
    }
}
