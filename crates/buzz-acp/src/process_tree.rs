//! Bounded process snapshots for an ACP worker and its resident providers.
//!
//! Pool sleep kills the worker's process group. Children that are still
//! doing the turn's work (a subagent, a background shell) must keep the
//! worker alive. Long-lived helpers such as MCP servers must not, or a
//! lazy pool would never sleep. Agent CLIs often start MCP servers through
//! launcher chains (`npm exec`, `sh -c`, a node launcher script); a chain
//! whose every leaf is a helper is a helper too.

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
            return helper_name(script) || in_helper_package(script);
        }
    }
    false
}

fn helper_name(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    name == "mcp"
        || name.starts_with("mcp-")
        || name.starts_with("mcp_")
        || name.contains("-mcp")
        || name.contains("_mcp")
        || path.split('/').any(|part| part == "@modelcontextprotocol")
}

/// Whether a script path lies inside an installed package with a helper
/// name: the component right after `node_modules/` (or after its `@scope/`).
/// Ordinary directories are not packages, so a work directory that merely
/// contains the letters never qualifies.
fn in_helper_package(script: &str) -> bool {
    let parts: Vec<&str> = script.split('/').collect();
    parts.iter().enumerate().any(|(index, part)| {
        if *part != "node_modules" {
            return false;
        }
        let package = match parts.get(index + 1) {
            Some(scope) if scope.starts_with('@') => parts.get(index + 2),
            other => other,
        };
        package.is_some_and(|package| helper_name(package))
    })
}

/// Launcher chains deeper than this are treated as work.
const MAX_LAUNCHER_DEPTH: usize = 8;

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

/// A single process snapshot, shared across a complete classification pass.
#[derive(Default)]
pub struct ProcessTree {
    #[cfg(unix)]
    retirement: Option<retirement::RetirementSnapshot>,
    children: HashMap<u32, Vec<ChildProcess>>,
    resident_helpers: HashSet<u32>,
    snapshot_helpers: HashSet<u32>,
    snapshot_candidates: HashSet<u32>,
    uncertain_snapshots: HashSet<u32>,
    instances: HashMap<u32, ProcessInstance>,
    registered_services: HashSet<u32>,
}

/// PID birth and current command, both required for a service registration.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ProcessInstance {
    birth: String,
    command: String,
}

impl ProcessInstance {
    /// Exec changes a command without creating a new PID birth.
    pub(crate) fn same_birth(&self, other: &Self) -> bool {
        self.birth == other.birth
    }
}

impl ProcessTree {
    /// Process-group proof from the same native enumeration as classification.
    #[cfg(unix)]
    pub(crate) fn retirement_snapshot(&self) -> Option<&retirement::RetirementSnapshot> {
        self.retirement.as_ref()
    }
    /// Executable basename only: arguments can contain credentials or task data.
    pub(crate) fn safe_command(&self, pid: u32) -> String {
        self.children
            .values()
            .flatten()
            .find(|child| child.pid == pid)
            .and_then(|child| child.command.split_whitespace().next())
            .and_then(|program| program.rsplit('/').next())
            .unwrap_or("unknown")
            .chars()
            .take(64)
            .collect()
    }
    #[cfg(unix)]
    pub(crate) fn service_instances(&self, root: u32) -> HashMap<u32, ProcessInstance> {
        self.descendant_pids(root)
            .into_iter()
            .chain([root])
            .filter_map(|pid| {
                self.instances
                    .get(&pid)
                    .cloned()
                    .map(|instance| (pid, instance))
            })
            .collect()
    }

    #[cfg(unix)]
    pub(crate) fn register_service_instances(
        &mut self,
        root: u32,
        instances: &HashMap<u32, ProcessInstance>,
    ) {
        if instances.get(&root).is_none() || instances.get(&root) != self.instances.get(&root) {
            return;
        }
        let live = self.descendant_pids(root);
        for (&pid, instance) in instances {
            if (pid == root || live.contains(&pid)) && self.instances.get(&pid) == Some(instance) {
                self.registered_services.insert(pid);
            }
        }
    }

    pub(crate) fn is_registered_service(&self, pid: u32) -> bool {
        self.registered_services.contains(&pid)
    }

    pub(crate) fn process_instance(&self, pid: u32) -> Option<&ProcessInstance> {
        self.instances.get(&pid)
    }
    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    pub(crate) fn from_snapshot_with_identities(output: &str, identities: Option<&str>) -> Self {
        let mut tree = parse_snapshot(output).unwrap();
        let snapshots = codex_snapshots::candidates(&tree);
        codex_helpers::apply_snapshot_identities(&mut tree, &snapshots, identities);
        tree
    }

    /// Snapshot shells are bookkeeping only while their current identity matches.
    pub(crate) fn is_snapshot_helper(&self, pid: u32) -> bool {
        self.snapshot_helpers.contains(&pid)
    }

    /// A structural snapshot branch must never authorize permanent residency.
    pub(crate) fn is_snapshot_candidate(&self, pid: u32) -> bool {
        self.snapshot_candidates.contains(&pid)
    }

    /// Missing executable proof is uncertainty rather than observed task work.
    pub(crate) fn is_uncertain_snapshot(&self, pid: u32) -> bool {
        self.uncertain_snapshots.contains(&pid)
    }

    /// Startup residents exclude snapshot branches, whose children may be real work.
    pub(crate) fn startup_resident_pids(&self, parent: u32) -> HashSet<u32> {
        let mut residents = HashSet::new();
        let mut pending = vec![parent];
        while let Some(parent) = pending.pop() {
            for child in self.children(parent) {
                if !self.is_snapshot_candidate(child.pid)
                    && !self.is_registered_service(child.pid)
                    && residents.insert(child.pid)
                {
                    pending.push(child.pid);
                }
            }
        }
        residents
    }

    /// Definite work children, excluding residents, helpers and uncertain snapshots.
    pub fn blocking_pids(&self, parent: u32, resident: &HashSet<u32>) -> HashSet<u32> {
        self.children(parent)
            .iter()
            .filter(|child| {
                !resident.contains(&child.pid)
                    && !self.is_resident_helper(child)
                    && !self.is_uncertain_snapshot(child.pid)
            })
            .map(|child| child.pid)
            .collect()
    }

    /// Helpers confirmed by executable identity, the MCP classifier, or a
    /// launcher whose children are all helpers. A childless non-helper is work.
    pub fn is_resident_helper(&self, child: &ChildProcess) -> bool {
        self.is_snapshot_helper(child.pid) || self.is_helper_within(child, MAX_LAUNCHER_DEPTH)
    }

    fn is_helper_within(&self, child: &ChildProcess, depth: usize) -> bool {
        if self.registered_services.contains(&child.pid)
            || self.resident_helpers.contains(&child.pid)
            || is_resident_helper(&child.command)
        {
            return true;
        }
        let children = self.children(child.pid);
        depth > 0
            && !children.is_empty()
            && children
                .iter()
                .all(|grandchild| self.is_helper_within(grandchild, depth - 1))
    }

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
    command
        .args(["-ww", "-axo", "pid=,ppid=,pgid=,stat=,lstart=,command="])
        .env("LC_ALL", "C");
    let output = bounded_output(
        command,
        std::time::Duration::from_millis(250),
        4 * 1024 * 1024,
    )?;
    let tree = parse_native_snapshot(&output)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let tree = codex_helpers::classify(tree);
    Some(tree)
}

#[cfg(unix)]
fn parse_native_snapshot(output: &str) -> Option<ProcessTree> {
    let retirement = retirement::RetirementSnapshot::parse_instances(output)?;
    let mut normalized = String::new();
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.split_whitespace();
        let pid = fields.next()?;
        let parent = fields.next()?;
        fields.next()?.parse::<u32>().ok()?;
        normalized.push_str(&format!(
            "{pid} {parent} {}\n",
            fields.collect::<Vec<_>>().join(" ")
        ));
    }
    let mut tree = parse_instance_snapshot(&normalized)?;
    tree.retirement = Some(retirement);
    Some(tree)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod codex_helpers;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod codex_snapshots;

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
pub(crate) fn parse_instance_snapshot(output: &str) -> Option<ProcessTree> {
    let mut normalized = String::new();
    let mut instances = HashMap::new();
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.split_whitespace();
        let pid: u32 = fields.next()?.parse().ok()?;
        let parent = fields.next()?;
        let state = fields.next()?;
        let birth = (0..5)
            .map(|_| fields.next())
            .collect::<Option<Vec<_>>>()?
            .join(" ");
        let command = fields.collect::<Vec<_>>().join(" ");
        if command.is_empty() || instances.contains_key(&pid) {
            return None;
        }
        normalized.push_str(&format!("{pid} {parent} {state} {command}\n"));
        if !state.starts_with('Z') {
            instances.insert(pid, ProcessInstance { birth, command });
        }
    }
    let mut tree = parse_snapshot(&normalized)?;
    tree.instances = instances;
    Some(tree)
}

#[cfg(unix)]
pub(crate) fn bounded_output(
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
        let tree = ProcessTree {
            children: HashMap::from([(99, children)]),
            ..Default::default()
        };
        let blocking = tree.blocking_pids(99, &resident);
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

#[cfg(all(test, unix))]
mod launcher_chain_tests {
    use super::*;

    #[test]
    fn npm_exec_mcp_chain_is_resident() {
        let tree = parse_snapshot(concat!(
            "11 10 S npm exec @zereight/mcp-gitlab\n",
            "12 11 S sh -c 'mcp-gitlab'\n",
            "13 12 S node /home/u/.npm/_npx/d6b1/node_modules/.bin/mcp-gitlab\n",
        ))
        .expect("valid process snapshot");
        assert!(tree.is_resident_helper(&tree.children(10)[0]));
        assert!(tree.blocking_pids(10, &HashSet::new()).is_empty());
    }

    #[test]
    fn launcher_into_scoped_mcp_package_is_resident() {
        let tree = parse_snapshot(concat!(
            "21 10 S /usr/bin/node /home/u/.cache/b24-mcp/launcher.cjs\n",
            "22 21 S /usr/bin/node /home/u/.cache/b24-mcp/versions/1.1.2/node_modules/@bitrix/b24-mcp/dist/cli.js\n",
        ))
        .expect("valid process snapshot");
        assert!(tree.blocking_pids(10, &HashSet::new()).is_empty());
    }

    #[test]
    fn shell_with_non_helper_leaf_still_blocks() {
        let tree = parse_snapshot(concat!(
            "31 10 S /bin/bash -c sleep 600\n",
            "32 31 S sleep 600\n",
            "41 10 S npm exec @upstash/context7-mcp\n",
        ))
        .expect("valid process snapshot");
        // 41 has no children yet: not proven resident on this poll.
        assert_eq!(
            tree.blocking_pids(10, &HashSet::new()),
            HashSet::from([31, 41])
        );
    }

    #[test]
    fn work_dir_named_like_mcp_is_not_a_package() {
        assert!(!is_resident_helper("python3 /home/u/my-mcp-project/run.py"));
    }
}

#[cfg(all(test, unix))]
mod retirement_tests;

#[cfg(unix)]
pub(crate) mod retirement;
