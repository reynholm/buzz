//! Keep native service provenance across adapter-generated thread configuration.
use super::*;
use serde_json::{Map, Value};

#[derive(serde::Serialize, serde::Deserialize)]
struct Spec {
    adapter: String,
    original_codex_path: Option<String>,
    servers: Map<String, Value>,
}

pub(super) fn install(
    registry: &ServiceRegistry,
    adapter: &str,
    launch: &mut tokio::process::Command,
    servers: Map<String, Value>,
) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if servers.is_empty() {
        return Ok(());
    }
    let original_codex_path = match launch
        .as_std()
        .get_envs()
        .find(|(key, _)| *key == "CODEX_PATH")
    {
        Some((_, value)) => value.map(|value| value.to_string_lossy().into_owned()),
        None => std::env::var("CODEX_PATH").ok(),
    };
    let spec = registry.directory.path().join("codex.launch");
    std::fs::write(
        &spec,
        serde_json::to_vec(&Spec {
            adapter: adapter.into(),
            original_codex_path,
            servers,
        })?,
    )?;
    std::fs::set_permissions(&spec, std::fs::Permissions::from_mode(0o600))?;
    let proxy = registry.directory.path().join("codex-service-proxy");
    let quote = |value: &Path| format!("'{}'", value.to_string_lossy().replace('\'', "'\\''"));
    std::fs::write(
        &proxy,
        format!(
            "#!/bin/sh\nexec {} __codex-service-proxy {} \"$@\"\n",
            quote(&registry.executable),
            quote(&spec)
        ),
    )?;
    std::fs::set_permissions(&proxy, std::fs::Permissions::from_mode(0o700))?;
    launch.env("CODEX_PATH", proxy);
    Ok(())
}

fn apply_servers(request: &mut Value, servers: &Map<String, Value>) -> Result<()> {
    if !matches!(
        request["method"].as_str(),
        Some("thread/start" | "thread/resume" | "thread/fork")
    ) {
        return Ok(());
    }
    let config = &mut request["params"]["config"];
    if config.is_null() {
        *config = serde_json::json!({});
    }
    let incoming = &mut config["mcp_servers"];
    if incoming.is_null() {
        *incoming = serde_json::json!({});
    }
    let incoming = incoming
        .as_object_mut()
        .context("invalid thread MCP configuration")?;
    for (name, registered) in servers {
        let mut merged = registered.clone();
        if let Some(value) = incoming.get(name) {
            super::codex::merge_config(&mut merged, value);
        }
        // Injected servers already passed through this worker's controlled
        // wrapper keep their own launch. Raw native definitions cannot undo it.
        let controlled = merged.get("command") == registered.get("command")
            && merged
                .get("args")
                .and_then(Value::as_array)
                .and_then(|args| args.first())
                .and_then(Value::as_str)
                == Some("__mcp-service");
        if !controlled {
            for key in ["command", "args"] {
                if let Some(value) = registered.get(key) {
                    merged[key] = value.clone();
                }
            }
        }
        incoming.insert(name.clone(), merged);
    }
    Ok(())
}

pub(crate) async fn run(args: Vec<String>) -> Result<()> {
    use futures_util::StreamExt;
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    use tokio_util::codec::{FramedRead, LinesCodec};
    let path = args
        .first()
        .context("native Codex launch specification missing")?;
    let spec: Spec = serde_json::from_slice(&std::fs::read(path)?)?;
    let mut command = tokio::process::Command::new(&spec.adapter);
    if crate::config::normalize_agent_command_identity(&spec.adapter) != "codex" {
        command.arg("cli");
    }
    if let Some(path) = spec.original_codex_path {
        command.env("CODEX_PATH", path);
    } else {
        command.env_remove("CODEX_PATH");
    }
    let mut child = command
        .args(&args[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    let mut input = FramedRead::new(
        tokio::io::stdin(),
        LinesCodec::new_with_max_length(32 * 1024 * 1024),
    );
    let mut child_input = child.stdin.take().context("Codex stdin unavailable")?;
    let mut child_output = child.stdout.take().context("Codex stdout unavailable")?;
    let input_task = async {
        while let Some(line) = input.next().await {
            let mut request: Value = serde_json::from_str(&line?)?;
            apply_servers(&mut request, &spec.servers)?;
            let bytes = serde_json::to_vec(&request)?;
            child_input.write_all(&bytes).await?;
            child_input.write_all(b"\n").await?;
            child_input.flush().await?;
        }
        Ok::<(), anyhow::Error>(())
    };
    let output_task = async {
        tokio::io::copy(&mut child_output, &mut tokio::io::stdout())
            .await
            .map(|_| ())
            .map_err(anyhow::Error::from)
    };
    let result = tokio::select! { result = input_task => result, result = output_task => result };
    drop(child_input);
    if tokio::time::timeout(std::time::Duration::from_secs(2), child.wait())
        .await
        .is_err()
    {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn partial_native_environment_override_keeps_other_native_values() {
        let registered = json!({"native":{"command":"wrapper", "args":["__mcp-service","registry"], "env":{"KEEP":"kept", "REPLACE":"old"}}}).as_object().unwrap().clone();
        let mut request = json!({"method":"thread/start","params":{"config":{"mcp_servers":{"native":{"env":{"REPLACE":"new"}}}}}});
        apply_servers(&mut request, &registered).unwrap();
        assert_eq!(
            request["params"]["config"]["mcp_servers"]["native"]["env"],
            json!({"KEEP":"kept","REPLACE":"new"})
        );
    }

    #[test]
    fn native_provenance_survives_adapter_replacement_and_raw_native_overrides() {
        let registered = json!({"native":{"command":"wrapper", "args":["__mcp-service","registry"], "cwd":"/tmp/service", "env":{"TOKEN":"secret"}, "tool_timeout_sec":42}}).as_object().unwrap().clone();
        for incoming in [
            json!({"injected":{"command":"wrapper", "args":["__mcp-service","another"]}}),
            json!({"native":{"command":"npm","args":["start"]}}),
        ] {
            let mut request = json!({"id":1,"method":"thread/start","params":{"cwd":"/tmp/task", "config":{"mcp_servers":incoming}}});
            apply_servers(&mut request, &registered).unwrap();
            let native = &request["params"]["config"]["mcp_servers"]["native"];
            assert_eq!(native, &registered["native"]);
            assert_eq!(request["params"]["cwd"], "/tmp/task");
        }
    }
}
