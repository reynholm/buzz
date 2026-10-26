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
    let output = tokio::task::spawn_blocking(move || {
        crate::process_tree::bounded_output(
            discovery,
            std::time::Duration::from_secs(3),
            1024 * 1024,
        )
    })
    .await?
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
