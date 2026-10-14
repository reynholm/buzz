//! Bounded process snapshots for an ACP worker and its resident providers.
//!
//! Pool sleep kills the worker's process group. Children that are still
//! doing the turn's work (a subagent, a background shell) must keep the
//! worker alive. Long-lived helpers such as MCP servers must not, or a
//! lazy pool would never sleep.

use std::collections::{HashMap, HashSet};

/// One direct child, as reported by `ps`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChildProcess {
    pub pid: u32,
    /// Full command line (`ps` command column), with whitespace collapsed.
    pub command: String,
}

/// Whether the launched executable or interpreter script identifies an MCP helper.
///
/// Arguments of ordinary work must not turn it into a resident just because a
/// task mentions MCP, or a working directory happens to contain those letters.
pub fn is_resident_helper(command: &str) -> bool {
    fn helper_name(path: &str) -> bool {
        let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
        name == "mcp"
            || name.starts_with("mcp-")
            || name.starts_with("mcp_")
            || name.contains("-mcp")
            || name.contains("_mcp")
            || path.split('/').any(|part| part == "@modelcontextprotocol")
    }
    let mut parts = command.split_whitespace();
    let Some(executable) = parts.next() else {
        return false;
    };
    if helper_name(executable) {
        return true;
    }
    let name = executable.rsplit('/').next().unwrap_or(executable);
    if name == "node" || name.starts_with("python") {
        // The first non-option word is the script. Inline programs (-c/-e)
        // and interpreter flags with their own values are deliberately unknown.
        if let Some(script) = parts.next().filter(|part| !part.starts_with('-')) {
            return helper_name(script);
        }
    }
    false
}

/// Parse one `ps -axo pid=,ppid=,command=` line.
///
/// Returns `(pid, ppid, command)`.
#[cfg(any(unix, test))]
pub fn parse_ps_line(line: &str) -> Option<(u32, u32, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut parts = trimmed.split_whitespace();
    let pid = parts.next()?.parse().ok()?;
    let ppid = parts.next()?.parse().ok()?;
    let command = parts.collect::<Vec<_>>().join(" ");
    Some((pid, ppid, command))
}

/// Pids that should keep the worker out of idle-pool sleep.
///
/// `resident` holds children captured at worker start and helpers adopted
/// since (MCP). Everything else still parented by the worker is background
/// work: a subagent or a shell the turn left running.
pub fn blocking_pids(children: &[ChildProcess], resident: &HashSet<u32>) -> HashSet<u32> {
    children
        .iter()
        .filter(|child| !resident.contains(&child.pid) && !is_resident_helper(&child.command))
        .map(|child| child.pid)
        .collect()
}

/// A single process snapshot, shared across a complete classification pass.
#[derive(Default)]
pub struct ProcessTree {
    children: HashMap<u32, Vec<ChildProcess>>,
}

impl ProcessTree {
    /// Direct children in this snapshot.
    pub fn children(&self, parent: u32) -> &[ChildProcess] {
        self.children.get(&parent).map(Vec::as_slice).unwrap_or(&[])
    }

    /// All descendants of a worker; cycles in malformed input are bounded.
    pub fn descendant_pids(&self, parent: u32) -> HashSet<u32> {
        let mut descendants = HashSet::new();
        let mut pending = vec![parent];
        while let Some(parent) = pending.pop() {
            for child in self.children(parent) {
                if descendants.insert(child.pid) {
                    pending.push(child.pid);
                }
            }
        }
        descendants
    }
}

/// Snapshot live processes, bounded by time and captured output.
///
/// Failure, timeout, malformed output, and an exceeded output limit all return
/// `None`; callers considering pool sleep must retain the worker in that case.
#[cfg(unix)]
pub fn snapshot() -> Option<ProcessTree> {
    let mut command = tokio::process::Command::new("/bin/ps");
    command.args(["-axo", "pid=,ppid=,stat=,command="]);
    let output = bounded_output(
        command,
        std::time::Duration::from_millis(250),
        4 * 1024 * 1024,
    )?;
    parse_snapshot(&output)
}

#[cfg(unix)]
fn parse_snapshot(output: &str) -> Option<ProcessTree> {
    let mut tree = ProcessTree::default();
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let (pid, parent, tail) = parse_ps_line(line)?;
        let (state, command) = tail.split_once(' ')?;
        if state.starts_with('Z') {
            continue; // Exited children waiting for their parent to reap are not work.
        }
        tree.children.entry(parent).or_default().push(ChildProcess {
            pid,
            command: command.to_owned(),
        });
    }
    Some(tree)
}

#[cfg(unix)]
fn bounded_output(
    mut command: tokio::process::Command,
    timeout: std::time::Duration,
    limit: usize,
) -> Option<String> {
    use std::io::{ErrorKind, Read};
    use std::os::{fd::OwnedFd, unix::net::UnixStream};
    use std::process::Stdio;
    use std::time::Instant;

    // A nonblocking socket keeps reads bounded without a reader thread or an
    // unbounded temporary file. Tokio reaps killed children on error/drop.
    let _runtime = tokio::runtime::Handle::try_current().ok()?;
    let (mut reader, writer) = UnixStream::pair().ok()?;
    reader.set_nonblocking(true).ok()?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::from(OwnedFd::from(writer)))
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    // Command retains its Stdio handle after spawn; close that copy so EOF is
    // visible when the child closes stdout.
    drop(command);
    let deadline = Instant::now() + timeout;
    let mut output = Vec::new();
    let mut buffer = [0; 8192];
    let mut eof = false;
    loop {
        if Instant::now() >= deadline {
            return None;
        }
        match reader.read(&mut buffer) {
            Ok(0) => eof = true,
            Ok(count) => {
                if output.len().saturating_add(count) > limit {
                    return None;
                }
                output.extend_from_slice(&buffer[..count]);
                continue;
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return None,
        }
        if let Some(status) = child.try_wait().ok()? {
            if !status.success() {
                return None;
            }
            if eof {
                return String::from_utf8(output).ok();
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// Non-Unix builds have no process-group kill of the worker's children.
#[cfg(not(unix))]
pub fn snapshot() -> Option<ProcessTree> {
    Some(ProcessTree::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_launchers_are_resident_helpers() {
        assert!(is_resident_helper(
            "/usr/bin/python3 /Users/reynholm/.codex/tools/gitlab_mcp_launcher.py"
        ));
        assert!(is_resident_helper(
            "node /opt/buzz/node_modules/@modelcontextprotocol/server/index.js"
        ));
        assert!(is_resident_helper("buzz-dev-mcp"));
    }

    #[test]
    fn subagents_and_background_shells_are_not_helpers() {
        assert!(!is_resident_helper("/Users/reynholm/.grok/bin/grok"));
        assert!(!is_resident_helper("zsh -c python3 operator.py"));
        assert!(!is_resident_helper(
            "/usr/bin/python3 /tmp/cell/operator.py"
        ));
    }

    #[test]
    fn parse_ps_line_keeps_the_command_tail() {
        let parsed = parse_ps_line("  42  7  /usr/bin/python3 /opt/gitlab_mcp_launcher.py --flag")
            .expect("line");
        assert_eq!(parsed.0, 42);
        assert_eq!(parsed.1, 7);
        assert_eq!(
            parsed.2,
            "/usr/bin/python3 /opt/gitlab_mcp_launcher.py --flag"
        );
    }

    #[test]
    fn blocking_pids_ignore_residents_and_helpers() {
        let children = vec![
            ChildProcess {
                pid: 1,
                command: "/usr/bin/python3 gitlab_mcp_launcher.py".into(),
            },
            ChildProcess {
                pid: 2,
                command: "/Users/reynholm/.grok/bin/grok".into(),
            },
            ChildProcess {
                pid: 3,
                command: "zsh -c sleep 100".into(),
            },
            ChildProcess {
                pid: 4,
                command: "node tool.js".into(),
            },
        ];
        let mut resident = HashSet::new();
        resident.insert(4);
        let blocking = blocking_pids(&children, &resident);
        assert_eq!(blocking.len(), 2);
        assert!(blocking.contains(&2));
        assert!(blocking.contains(&3));
    }

    #[cfg(unix)]
    #[test]
    fn children_of_filters_by_ppid() {
        let ps = "\
10 1 S /bin/buzz-acp\n\
20 10 S /usr/bin/python3 gitlab_mcp_launcher.py\n\
21 10 S /Users/reynholm/.grok/bin/grok\n\
30 21 S zsh -c echo\n";
        let tree = parse_snapshot(ps).expect("valid process snapshot");
        let kids = tree.children(10);
        let pids: HashSet<u32> = kids.iter().map(|child| child.pid).collect();
        assert_eq!(pids, HashSet::from([20, 21]));
    }
}

#[cfg(test)]
mod classification_regression_tests {
    #[test]
    fn shell_work_mentioning_mcp_is_not_a_resident_helper() {
        assert!(!super::is_resident_helper(
            "/bin/bash -c python3 /tmp/mcp-project/run-tests.py"
        ));
        assert!(!super::is_resident_helper(
            "python3 /tmp/work.py --output /tmp/mcp-report.json"
        ));
    }
}

#[cfg(all(test, unix))]
mod snapshot_tests;
