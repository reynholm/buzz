//! Explicit service provenance at the stdio MCP launch/initialize boundary.
use crate::{
    acp::McpServer,
    process_tree::{ProcessInstance, ProcessTree},
};
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const MAX_REGISTRATIONS: usize = 128;
const MAX_RECORD_BYTES: u64 = 256 * 1024;

pub(crate) struct ServiceRegistry {
    directory: tempfile::TempDir,
    generation: Uuid,
    executable: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Registration {
    generation: Uuid,
    root: u32,
    instances: HashMap<u32, ProcessInstance>,
}

impl ServiceRegistry {
    pub fn for_current_executable() -> Result<Self> {
        #[cfg(test)]
        if let Some(directory) = std::env::var_os("BUZZ_TEST_BIN_DIR") {
            return Self::new(PathBuf::from(directory).join("buzz-acp"));
        }
        Self::new(std::env::current_exe()?)
    }

    pub fn new(executable: PathBuf) -> Result<Self> {
        Ok(Self {
            directory: tempfile::tempdir()?,
            generation: Uuid::new_v4(),
            executable,
        })
    }

    pub fn wrap_servers(&self, servers: Vec<McpServer>) -> Vec<McpServer> {
        servers
            .into_iter()
            .map(|mut server| {
                let mut args = self.wrapper_args(Uuid::new_v4());
                args.push(server.command);
                args.extend(server.args);
                server.command = self.executable.to_string_lossy().into_owned();
                server.args = args;
                server
            })
            .collect()
    }

    pub(super) fn wrapper_args(&self, registration: Uuid) -> Vec<String> {
        vec![
            "__mcp-service".into(),
            self.directory.path().to_string_lossy().into_owned(),
            self.generation.to_string(),
            registration.to_string(),
        ]
    }

    #[cfg(test)]
    fn register(&self, id: Uuid, root: u32, tree: &ProcessTree) -> Result<()> {
        register(self.directory.path(), self.generation, id, root, tree)
    }

    /// Run on the snapshot collector, never in the main relay loop.
    pub fn apply(&self, tree: &mut ProcessTree) {
        use std::io::Read;
        let Ok(entries) = std::fs::read_dir(self.directory.path()) else {
            return;
        };
        for entry in entries.take(MAX_REGISTRATIONS).flatten() {
            if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Ok(file) = std::fs::File::open(entry.path()) else {
                continue;
            };
            let mut bytes = Vec::new();
            if file
                .take(MAX_RECORD_BYTES + 1)
                .read_to_end(&mut bytes)
                .is_err()
                || bytes.len() as u64 > MAX_RECORD_BYTES
            {
                continue;
            }
            let Ok(record) = serde_json::from_slice::<Registration>(&bytes) else {
                continue;
            };
            if record.generation == self.generation {
                tree.register_service_instances(record.root, &record.instances);
            }
        }
    }
}

fn register(
    directory: &Path,
    generation: Uuid,
    id: Uuid,
    root: u32,
    tree: &ProcessTree,
) -> Result<()> {
    let instances = tree.service_instances(root);
    if !instances.contains_key(&root) || instances.len() > 512 {
        bail!("MCP service process identity is unavailable or exceeds the bound");
    }
    let record = Registration {
        generation,
        root,
        instances,
    };
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    serde_json::to_writer(temporary.as_file_mut(), &record)?;
    temporary.persist(directory.join(format!("{id}.json")))?;
    Ok(())
}

/// Relay MCP lines unchanged. Establish service instances before publishing the
/// successful initialize response, so tool work cannot be adopted as a service.
pub(crate) async fn run(args: Vec<String>) -> Result<()> {
    use futures_util::{SinkExt, StreamExt};
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    use tokio_util::codec::{FramedRead, LinesCodec};
    if args.len() < 4 {
        bail!("invalid internal MCP launch");
    }
    let directory = PathBuf::from(&args[0]);
    let generation: Uuid = args[1].parse()?;
    let _: Uuid = args[2].parse()?;
    // The same configured server can be instantiated by multiple sessions.
    let id = Uuid::new_v4();
    let mut child = tokio::process::Command::new(&args[3])
        .args(&args[4..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .context("starting registered MCP service")?;
    let mut child_stdin = child.stdin.take().context("MCP stdin unavailable")?;
    let child_stdout = child.stdout.take().context("MCP stdout unavailable")?;
    let mut input = FramedRead::new(
        tokio::io::stdin(),
        LinesCodec::new_with_max_length(32 * 1024 * 1024),
    );
    let mut output = FramedRead::new(
        child_stdout,
        LinesCodec::new_with_max_length(32 * 1024 * 1024),
    );
    let mut stdout = tokio_util::codec::FramedWrite::new(tokio::io::stdout(), LinesCodec::new());
    let (initialize_tx, initialize_rx) = tokio::sync::watch::channel(None::<serde_json::Value>);
    let mut initialized = false;
    // Poll both pumps independently: a blocked request write must not prevent
    // draining the server's previous response (and vice versa).
    let input_task = async {
        while let Some(line) = input.next().await {
            let line = line?;
            if let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) {
                if message["method"] == "initialize" {
                    initialize_tx.send_replace(message.get("id").cloned());
                }
            }
            child_stdin.write_all(line.as_bytes()).await?;
            child_stdin.write_all(b"\n").await?;
            child_stdin.flush().await?;
        }
        Ok::<(), anyhow::Error>(())
    };
    let output_task = async {
        while let Some(line) = output.next().await {
            let line = line?;
            if !initialized {
                let initialized_response = serde_json::from_str::<serde_json::Value>(&line)
                    .ok()
                    .is_some_and(|message| {
                        initialize_rx
                            .borrow()
                            .as_ref()
                            .is_some_and(|initialize_id| {
                                message.get("id") == Some(initialize_id)
                                    && message.get("result").is_some()
                            })
                    });
                if initialized_response {
                    let directory = directory.clone();
                    let registered = tokio::task::spawn_blocking(move || {
                        let tree = crate::process_tree::snapshot()
                            .context("MCP identity snapshot unavailable")?;
                        register(&directory, generation, id, std::process::id(), &tree)
                    })
                    .await?;
                    if let Err(error) = registered {
                        // Missing provenance keeps the worker conservatively held.
                        eprintln!("MCP service registration failed: {error}");
                    }
                    initialized = true;
                }
            }
            stdout.send(line).await?;
        }
        Ok::<(), anyhow::Error>(())
    };
    let outcome = tokio::select! {
        result = input_task => result,
        result = output_task => result,
    };
    // The enclosing ACP worker owns process-group retirement, including escaped
    // descendants. This wrapper reaps its direct child on every transport exit.
    drop(child_stdin);
    let _ = child.kill().await;
    let _ = child.wait().await;
    let _ = std::fs::remove_file(directory.join(format!("{id}.json")));
    outcome
}

mod codex;
pub(crate) mod codex_proxy;
pub(crate) use codex::wrap_native_config;

#[cfg(test)]
#[path = "mcp_service_tests.rs"]
mod tests;
