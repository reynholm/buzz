//! Identity-preserving process-group snapshots for explicit worker retirement.
use std::collections::{HashMap, HashSet};
use std::time::Duration;

#[derive(Clone, Debug)]
struct Process {
    parent: u32,
    group: u32,
    command: String,
    zombie: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RetirementSnapshot {
    processes: HashMap<u32, Process>,
}

impl RetirementSnapshot {
    pub(crate) fn parse(output: &str) -> Option<Self> {
        let mut processes = HashMap::new();
        for line in output.lines().filter(|line| !line.trim().is_empty()) {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse().ok()?;
            let parent = fields.next()?.parse().ok()?;
            let group = fields.next()?.parse().ok()?;
            let state = fields.next()?;
            let command = fields.collect::<Vec<_>>().join(" ");
            if command.is_empty() || processes.contains_key(&pid) {
                return None;
            }
            processes.insert(
                pid,
                Process {
                    parent,
                    group,
                    command,
                    zombie: state.starts_with('Z'),
                },
            );
        }
        (!processes.is_empty()).then_some(Self { processes })
    }

    /// A complete initial snapshot must still identify the owned live adapter.
    pub(crate) fn worker_present(&self, pid: u32) -> bool {
        self.processes
            .get(&pid)
            .is_some_and(|process| process.group == pid && !process.zombie)
    }

    fn descendants(&self, root: u32) -> HashSet<u32> {
        let mut found = HashSet::from([root]);
        let mut pending = vec![root];
        while let Some(parent) = pending.pop() {
            for (&pid, process) in &self.processes {
                if process.parent == parent && found.insert(pid) {
                    pending.push(pid);
                }
            }
        }
        found
    }

    pub(crate) fn descendant_groups(&self, root: u32, own_group: u32) -> HashSet<u32> {
        self.descendants(root)
            .iter()
            .filter_map(|pid| self.processes.get(pid))
            .map(|process| process.group)
            .filter(|group| *group > 1 && *group != own_group)
            .collect()
    }

    /// Revalidate PID and command before signalling an escaped group. Descendants
    /// of a surviving anchor are included even after that anchor was orphaned.
    pub(crate) fn verified_groups(&self, last: &Self, root: u32, own_group: u32) -> HashSet<u32> {
        let mut verified = HashSet::new();
        for pid in self.descendants(root) {
            if self
                .processes
                .get(&pid)
                .zip(last.processes.get(&pid))
                .is_some_and(|(before, after)| before.command == after.command)
            {
                verified.extend(last.descendant_groups(pid, own_group));
            }
        }
        // A sibling worker (and all its groups) is never a retirement target.
        let harness = self.processes.get(&root).map(|process| process.parent);
        if let Some(harness) = harness {
            for (&pid, process) in &last.processes {
                if process.parent == harness && pid != root {
                    for group in last.descendant_groups(pid, own_group) {
                        verified.remove(&group);
                    }
                }
            }
        }
        verified
    }
}

/// Called on a blocking thread: one attempt is bounded by two seconds and 4 MiB.
pub(crate) fn snapshot() -> Option<RetirementSnapshot> {
    let mut command = tokio::process::Command::new("/bin/ps");
    command.args(["-ww", "-axo", "pid=,ppid=,pgid=,stat=,command="]);
    RetirementSnapshot::parse(&super::bounded_output(
        command,
        Duration::from_secs(2),
        4 * 1024 * 1024,
    )?)
}
