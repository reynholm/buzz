//! Identity-preserving process-group snapshots for explicit worker retirement.
use std::collections::{HashMap, HashSet};
use std::time::Duration;

#[derive(Clone, Debug)]
struct Process {
    parent: u32,
    group: u32,
    command: String,
    zombie: bool,
    instance: Option<super::ProcessInstance>,
}

impl Process {
    fn same_instance(&self, other: &Self) -> bool {
        self.command == other.command
            && match (&self.instance, &other.instance) {
                (Some(before), Some(after)) => before == after,
                // Legacy synthetic snapshots have no birth column.
                _ => true,
            }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RetirementSnapshot {
    processes: HashMap<u32, Process>,
}

impl RetirementSnapshot {
    #[cfg(test)]
    pub(crate) fn parse(output: &str) -> Option<Self> {
        Self::parse_columns(output, false)
    }

    pub(crate) fn parse_instances(output: &str) -> Option<Self> {
        Self::parse_columns(output, true)
    }

    fn parse_columns(output: &str, birth_column: bool) -> Option<Self> {
        let mut processes = HashMap::new();
        for line in output.lines().filter(|line| !line.trim().is_empty()) {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse().ok()?;
            let parent = fields.next()?.parse().ok()?;
            let group = fields.next()?.parse().ok()?;
            let state = fields.next()?;
            let birth = if birth_column {
                Some(
                    (0..5)
                        .map(|_| fields.next())
                        .collect::<Option<Vec<_>>>()?
                        .join(" "),
                )
            } else {
                None
            };
            let command = fields.collect::<Vec<_>>().join(" ");
            if command.is_empty() || processes.contains_key(&pid) {
                return None;
            }
            processes.insert(
                pid,
                Process {
                    parent,
                    group,
                    instance: birth.map(|birth| super::ProcessInstance {
                        birth,
                        command: command.clone(),
                    }),
                    command,
                    zombie: state.starts_with('Z'),
                },
            );
        }
        (!processes.is_empty()).then_some(Self { processes })
    }

    /// Preserve old ancestry for still-live, identity-checked orphan anchors,
    /// and adopt only new descendants of the currently owned live worker.
    pub(crate) fn retain_owned(&self, previous: Option<&Self>, root: u32) -> Option<Self> {
        let mut processes = HashMap::new();
        if let Some(previous) = previous {
            let owned = previous.descendants(root);
            for pid in &owned {
                let live = previous
                    .processes
                    .get(pid)
                    .zip(self.processes.get(pid))
                    .is_some_and(|(before, after)| !after.zombie && before.same_instance(after));
                if !live {
                    continue;
                }
                let mut parent = *pid;
                while owned.contains(&parent) && !processes.contains_key(&parent) {
                    let process = previous.processes.get(&parent)?;
                    processes.insert(parent, process.clone());
                    parent = process.parent;
                }
            }
        }
        for pid in self.descendants(root) {
            processes.insert(pid, self.processes.get(&pid)?.clone());
        }
        (processes.len() <= 4096).then_some(Self { processes })
    }

    /// A complete initial snapshot must still identify the owned live adapter.
    pub(crate) fn worker_present(&self, pid: u32) -> bool {
        self.processes
            .get(&pid)
            .is_some_and(|process| process.group == pid && !process.zombie)
    }

    fn descendants(&self, root: u32) -> HashSet<u32> {
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for (&pid, process) in &self.processes {
            children.entry(process.parent).or_default().push(pid);
        }
        let mut found = HashSet::from([root]);
        let mut pending = vec![root];
        while let Some(parent) = pending.pop() {
            for pid in children.get(&parent).into_iter().flatten() {
                if found.insert(*pid) {
                    pending.push(*pid);
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

    /// Every currently live observed instance needs an owned pre-exit anchor.
    /// Missing birth/group evidence must not become a false clean retirement.
    pub(crate) fn covers_observed(
        &self,
        last: &Self,
        root: u32,
        observed: &HashMap<u32, super::ProcessInstance>,
    ) -> bool {
        let owned = self.descendants(root);
        observed.iter().all(|(pid, expected)| {
            let Some(after) = last.processes.get(pid).filter(|process| !process.zombie) else {
                return true;
            };
            let Some(current) = &after.instance else {
                return false;
            };
            if !current.same_birth(expected) {
                return true;
            } // Original PID instance exited.
            if current != expected {
                return false;
            } // Exec is uncertainty, not exit.
            owned.contains(pid)
                && self
                    .processes
                    .get(pid)
                    .is_some_and(|before| before.instance.as_ref() == Some(expected))
        })
    }

    /// Revalidate PID and command before signalling an escaped group. Descendants
    /// of a surviving anchor are included even after that anchor was orphaned.
    pub(crate) fn verified_groups(&self, last: &Self, root: u32, own_group: u32) -> HashSet<u32> {
        let mut verified = HashSet::new();
        let owned = self.descendants(root);
        for pid in &owned {
            if self
                .processes
                .get(pid)
                .zip(last.processes.get(pid))
                .is_some_and(|(before, after)| before.same_instance(after))
            {
                verified.extend(last.descendant_groups(*pid, own_group));
            }
        }
        // A sibling worker (and all its groups) is never a retirement target.
        let harness = self.processes.get(&root).map(|process| process.parent);
        if let Some(harness) = harness {
            for (&pid, process) in &last.processes {
                let owned_anchor = owned.contains(&pid)
                    && self
                        .processes
                        .get(&pid)
                        .is_some_and(|before| before.same_instance(process));
                if process.parent == harness && pid != root && !owned_anchor {
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
    command
        .args(["-ww", "-axo", "pid=,ppid=,pgid=,stat=,lstart=,command="])
        .env("LC_ALL", "C");
    RetirementSnapshot::parse_instances(&super::bounded_output(
        command,
        Duration::from_secs(2),
        4 * 1024 * 1024,
    )?)
}
