//! Direct children of an ACP worker.
//!
//! Pool sleep kills the worker's process group. Children that are still
//! doing the turn's work (a subagent, a background shell) must keep the
//! worker alive. Long-lived helpers such as MCP servers must not, or a
//! lazy pool would never sleep.

use std::collections::HashSet;

/// One direct child, as reported by `ps`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChildProcess {
    pub pid: u32,
    /// Full command line (`ps` command column), with whitespace collapsed.
    pub command: String,
}

/// A child that belongs to the worker for the life of the process.
///
/// MCP servers are the usual case. Their command lines carry `mcp`
/// (`gitlab_mcp_launcher.py`, `@modelcontextprotocol/...`, `buzz-dev-mcp`).
/// Matching on that substring keeps the check independent of the agent CLI.
pub fn is_resident_helper(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    // `mcp` covers gitlab_mcp_launcher and buzz-dev-mcp. The official SDK
    // package is `@modelcontextprotocol`, which does not contain those letters
    // in a row.
    command.contains("mcp") || command.contains("modelcontextprotocol")
}

/// Parse one `ps -axo pid=,ppid=,command=` line.
///
/// Returns `(pid, ppid, command)`.
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

/// Direct children of `parent` from a full `ps` listing.
pub fn children_of(parent: u32, ps_output: &str) -> Vec<ChildProcess> {
    let mut children = Vec::new();
    for line in ps_output.lines() {
        let Some((pid, ppid, command)) = parse_ps_line(line) else {
            continue;
        };
        if ppid == parent {
            children.push(ChildProcess { pid, command });
        }
    }
    children
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

/// Direct children of `parent`, or `None` when the listing could not be read.
///
/// Callers that are about to kill the worker treat `None` as "still busy".
#[cfg(unix)]
pub fn direct_children(parent: u32) -> Option<Vec<ChildProcess>> {
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid=,command="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Some(children_of(parent, &text))
}

/// Non-Unix builds have no process-group kill of the worker's children.
#[cfg(not(unix))]
pub fn direct_children(_parent: u32) -> Option<Vec<ChildProcess>> {
    Some(Vec::new())
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

    #[test]
    fn children_of_filters_by_ppid() {
        let ps = "\
10 1 /bin/buzz-acp\n\
20 10 /usr/bin/python3 gitlab_mcp_launcher.py\n\
21 10 /Users/reynholm/.grok/bin/grok\n\
30 21 zsh -c echo\n";
        let kids = children_of(10, ps);
        let pids: HashSet<u32> = kids.iter().map(|child| child.pid).collect();
        assert_eq!(pids, HashSet::from([20, 21]));
    }
}
