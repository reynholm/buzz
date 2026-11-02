//! Opt-in cgroup v2 ownership at the complete ACP worker boundary.
use super::AcpError;
use serde::Deserialize;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;

pub(super) const POLICY_ENV: &str = "BUZZ_ACP_RESOURCE_LIMITS";
const MAX_STATE: u64 = 4096;
const SUPERVISOR: &str = include_str!("../../assets/linux-resource-supervisor.py");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    slice: String,
    memory_high_bytes: u64,
    memory_max_bytes: u64,
}

fn guard_error(reason: &str) -> AcpError {
    AcpError::ResourceGuard(reason.to_owned())
}

fn read_small(path: &Path) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_STATE + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_STATE {
        return Err(std::io::Error::other("oversized cgroup state"));
    }
    String::from_utf8(bytes).map_err(std::io::Error::other)
}

fn limit(path: &Path) -> Result<u64, AcpError> {
    read_small(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .ok_or_else(|| guard_error("a finite cgroup memory limit is required"))
}

impl Policy {
    pub(super) fn parse(value: &str) -> Result<Self, AcpError> {
        let policy: Self = serde_json::from_str(value)
            .map_err(|_| guard_error("invalid BUZZ_ACP_RESOURCE_LIMITS policy"))?;
        let name = policy.slice.strip_suffix(".slice").unwrap_or("");
        if name.is_empty()
            || name.len() > 64
            || !name.bytes().all(|b| b.is_ascii_alphanumeric())
            || policy.memory_high_bytes == 0
            || policy.memory_max_bytes < policy.memory_high_bytes
        {
            return Err(guard_error("invalid worker memory limits or slice name"));
        }
        Ok(policy)
    }
}

/// Owns a unique scope, its private supervisor receipt and atomic subtree cleanup.
pub(super) struct WorkerScope {
    name: String,
    group: PathBuf,
    policy: Policy,
    state: PathBuf,
    _directory: tempfile::TempDir,
}

impl WorkerScope {
    pub(super) fn wrap(command: Command) -> Result<(Command, Option<Self>), AcpError> {
        let Some(value) = std::env::var_os(POLICY_ENV) else {
            return Ok((command, None));
        };
        let policy = Policy::parse(
            value
                .to_str()
                .ok_or_else(|| guard_error("invalid resource policy encoding"))?,
        )?;
        let membership = read_small(Path::new("/proc/self/cgroup"))
            .map_err(|_| guard_error("cgroup v2 membership unavailable"))?;
        let path = membership
            .trim()
            .strip_prefix("0::/")
            .ok_or_else(|| guard_error("cgroup v2 is required"))?;
        let components: Vec<_> = path.split('/').collect();
        let end = components
            .iter()
            .position(|s| s.starts_with("user@") && s.ends_with(".service"))
            .ok_or_else(|| guard_error("a user systemd manager is required"))?;
        let aggregate_relative = format!("/{}/{}", components[..=end].join("/"), policy.slice);
        if Path::new("/").join(path).starts_with(&aggregate_relative) {
            return Err(guard_error(
                "worker aggregate must not contain the harness or desktop",
            ));
        }
        let aggregate =
            Path::new("/sys/fs/cgroup").join(aggregate_relative.trim_start_matches('/'));
        let aggregate_max = limit(&aggregate.join("memory.max"))?;
        let aggregate_high = limit(&aggregate.join("memory.high"))?;
        if aggregate_high == 0
            || aggregate_high > aggregate_max
            || policy.memory_max_bytes > aggregate_max
            || limit(&aggregate.join("memory.swap.max"))? != 0
        {
            return Err(guard_error(
                "worker slice must have finite compatible memory limits and zero swap",
            ));
        }
        let directory = tempfile::Builder::new()
            .prefix("buzz-resource-")
            .tempdir()?;
        let script = directory.path().join("supervisor.py");
        fs::write(&script, SUPERVISOR)?;
        let state = directory.path().join("state.json");
        let name = format!("buzzacp-{}.scope", uuid::Uuid::new_v4().simple());
        let relative = format!("{aggregate_relative}/{name}");
        let group = Path::new("/sys/fs/cgroup").join(relative.trim_start_matches('/'));
        let mut wrapped = Command::new("/usr/bin/systemd-run");
        wrapped
            .args(["--user", "--scope", "--quiet", "--no-ask-password"])
            .arg(format!("--unit={name}"))
            .arg(format!("--slice={}", policy.slice))
            .arg(format!(
                "--property=MemoryHigh={}",
                policy.memory_high_bytes
            ))
            .arg(format!("--property=MemoryMax={}", policy.memory_max_bytes))
            .arg("--property=MemorySwapMax=0")
            .arg("--")
            .arg("/usr/bin/python3")
            .arg("-I")
            .arg(script)
            .arg(&state)
            .arg(&relative)
            .arg(policy.memory_high_bytes.to_string())
            .arg(policy.memory_max_bytes.to_string())
            .arg(command.as_std().get_program())
            .args(command.as_std().get_args());
        // The launch seam calls this before applying any environment or stdio.
        Ok((
            wrapped,
            Some(Self {
                name,
                group,
                policy,
                state,
                _directory: directory,
            }),
        ))
    }

    fn state(&self) -> Result<Option<String>, AcpError> {
        match read_small(&self.state) {
            Ok(body) => {
                let value: serde_json::Value = serde_json::from_str(&body)
                    .map_err(|_| guard_error("resource supervisor receipt is invalid"))?;
                value
                    .get("state")
                    .and_then(|s| s.as_str())
                    .map(|s| Some(s.to_owned()))
                    .ok_or_else(|| guard_error("resource supervisor receipt is invalid"))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(guard_error("resource supervisor receipt is unavailable")),
        }
    }

    pub(super) async fn ready(&self, child: &mut tokio::process::Child) -> Result<(), AcpError> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return Err(guard_error("resource scope launch exited before startup"));
            }
            match self.state()?.as_deref() {
                Some("ready") => {
                    for (file, expected) in [
                        ("memory.max", self.policy.memory_max_bytes),
                        ("memory.high", self.policy.memory_high_bytes),
                        ("memory.swap.max", 0),
                    ] {
                        if limit(&self.group.join(file))? != expected {
                            return Err(guard_error("effective worker limits differ from policy"));
                        }
                    }
                    tracing::info!(scope = %self.name, memory_max_bytes = self.policy.memory_max_bytes,
                        "ACP worker resource containment verified");
                    return Ok(());
                }
                Some("oom") => {
                    return Err(AcpError::ResourceLimit {
                        scope: self.name.clone(),
                    })
                }
                Some(_) => {
                    return Err(guard_error(
                        "resource supervisor failed before worker startup",
                    ))
                }
                None if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(10)).await
                }
                None => return Err(guard_error("resource supervisor startup deadline exceeded")),
            }
        }
    }

    pub(super) fn failure(&self) -> Option<AcpError> {
        match self.state() {
            Ok(Some(state)) if state == "oom" => Some(AcpError::ResourceLimit {
                scope: self.name.clone(),
            }),
            Ok(Some(state)) if state == "ready" || state == "exited" => {
                // Also catch a final result racing the supervisor's 50ms poll.
                match read_small(&self.group.join("memory.events")) {
                    Ok(events)
                        if events.lines().any(|line| {
                            line.strip_prefix("oom_kill ")
                                .and_then(|n| n.parse::<u64>().ok())
                                .is_some_and(|n| n > 0)
                        }) =>
                    {
                        Some(AcpError::ResourceLimit {
                            scope: self.name.clone(),
                        })
                    }
                    Err(_) if state == "ready" => {
                        Some(guard_error("worker cgroup monitoring became unavailable"))
                    }
                    _ => None,
                }
            }
            Err(error) => Some(error),
            _ => Some(guard_error(
                "resource supervisor stopped or lost its receipt",
            )),
        }
    }

    pub(super) fn kill(&self) -> std::io::Result<()> {
        match fs::write(self.group.join("cgroup.kill"), "1") {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }

    pub(super) async fn empty(&self) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            match read_small(&self.group.join("cgroup.events")) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return true,
                Ok(events) if events.lines().any(|line| line == "populated 0") => return true,
                _ if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(25)).await
                }
                _ => return false,
            }
        }
    }
}

impl Drop for WorkerScope {
    fn drop(&mut self) {
        if let Err(error) = self.kill() {
            tracing::error!(scope = %self.name, %error, "could not retire ACP worker cgroup");
        }
    }
}

#[cfg(test)]
impl WorkerScope {
    pub(super) fn unverified_fixture() -> (Self, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let group = directory.path().join("group");
        fs::create_dir_all(group.join("cgroup.kill")).unwrap();
        fs::write(group.join("cgroup.events"), "populated 1\n").unwrap();
        let state = directory.path().join("state.json");
        fs::write(&state, r#"{"state":"ready"}"#).unwrap();
        let scope = Self {
            name: "fixture.scope".into(),
            group: group.clone(),
            policy: Policy {
                slice: "fixture.slice".into(),
                memory_high_bytes: 1,
                memory_max_bytes: 2,
            },
            state,
            _directory: directory,
        };
        (scope, group)
    }
}
