//! Typed compaction lifecycles, fenced by provider session and local generation.
use std::collections::{HashMap, VecDeque};

use buzz_core::session_control::{ControlFailure, ControlOrigin, ControlOutcome};
use serde::Deserialize;
use uuid::Uuid;

const MAX_IDS: usize = 128;
const MAX_CREATION_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CompactionStatus {
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompactionUpdate {
    compaction_id: String,
    status: CompactionStatus,
}

#[derive(Default)]
pub(super) struct CompactionTracker {
    routes: HashMap<String, Route>,
    entries: VecDeque<Entry>,
    sequence: u64,
    creation: Option<Creation>,
}

struct Creation {
    generation: Uuid,
    advertisements: HashMap<String, bool>,
}

struct Route {
    generation: Uuid,
    advertised: bool,
    manual: Option<Manual>,
    adoptable_binding: bool,
}
struct Manual {
    after: u64,
    id: Option<String>,
    origin: ControlOrigin,
}
struct Entry {
    session: String,
    generation: Uuid,
    id: String,
    sequence: u64,
    status: CompactionStatus,
}

impl CompactionTracker {
    pub(super) fn ensure_route(&mut self, session: &str) {
        if !self.routes.contains_key(session) {
            self.route(session, Uuid::new_v4());
        }
    }
    #[cfg(all(test, unix))]
    pub(super) fn set_advertised(&mut self, session: &str) {
        if let Some(route) = self.routes.get_mut(session) {
            route.advertised = true;
        }
    }
    pub(super) fn route(&mut self, session: &str, generation: Uuid) {
        if self
            .routes
            .get(session)
            .is_some_and(|route| route.generation == generation)
        {
            return;
        }
        self.clear(session);
        self.routes.insert(
            session.into(),
            Route {
                generation,
                advertised: false,
                manual: None,
                adoptable_binding: false,
            },
        );
    }

    pub(super) fn prepare_new(&mut self) -> Uuid {
        let generation = Uuid::new_v4();
        self.creation = Some(Creation {
            generation,
            advertisements: HashMap::new(),
        });
        generation
    }

    pub(super) fn complete_new(&mut self, generation: Uuid, session: Option<&str>) {
        if self
            .creation
            .as_ref()
            .is_none_or(|pending| pending.generation != generation)
        {
            return;
        }
        let Some(pending) = self.creation.take() else {
            return;
        };
        let Some(session) = session.filter(|session| !session.is_empty()) else {
            return;
        };
        let advertised = pending
            .advertisements
            .get(session)
            .copied()
            .unwrap_or(false);
        self.route(session, generation);
        if let Some(route) = self.routes.get_mut(session) {
            route.advertised = advertised;
            route.adoptable_binding = true;
        }
    }

    pub(super) fn prepare_resume(&mut self, session: &str) -> Uuid {
        let generation = Uuid::new_v4();
        self.route(session, generation);
        generation
    }

    pub(super) fn complete_resume(&mut self, session: &str, generation: Uuid, success: bool) {
        if self
            .routes
            .get(session)
            .is_none_or(|route| route.generation != generation)
        {
            return;
        }
        if success {
            if let Some(route) = self.routes.get_mut(session) {
                route.adoptable_binding = true;
            }
        } else {
            self.clear(session);
        }
    }

    /// Attaching display metadata to this successful attachment is not a new
    /// provider generation. Keep its pre-ACK IDs and capability intact.
    pub(super) fn bind(&mut self, session: &str, generation: Uuid) {
        if let Some(route) = self
            .routes
            .get_mut(session)
            .filter(|route| route.adoptable_binding)
        {
            let prior = route.generation;
            route.generation = generation;
            route.adoptable_binding = false;
            for entry in &mut self.entries {
                if entry.session == session && entry.generation == prior {
                    entry.generation = generation;
                }
            }
        } else {
            self.route(session, generation);
        }
    }

    pub(super) fn retain_sessions(&mut self, sessions: &std::collections::HashSet<String>) {
        self.creation = None;
        self.routes.retain(|session, _| sessions.contains(session));
        self.entries
            .retain(|entry| sessions.contains(&entry.session));
    }

    pub(super) fn clear(&mut self, session: &str) {
        self.creation = None;
        self.routes.remove(session);
        self.entries.retain(|entry| entry.session != session);
    }

    pub(super) fn advertise(&mut self, session: &str, update: &serde_json::Value) {
        let advertised = update["availableCommands"]
            .as_array()
            .is_some_and(|commands| commands.iter().any(|command| command["name"] == "compact"));
        if let Some(route) = self.routes.get_mut(session) {
            route.advertised = advertised;
        } else if let Some(pending) = &mut self.creation {
            // Unknown notifications never establish a route. Only this bounded
            // in-flight creation may later adopt its successfully returned ID.
            let existing = pending.advertisements.contains_key(session);
            let bytes: usize = pending.advertisements.keys().map(String::len).sum();
            if existing
                || (pending.advertisements.len() < MAX_IDS
                    && bytes.saturating_add(session.len()) <= MAX_CREATION_BYTES)
            {
                pending.advertisements.insert(session.into(), advertised);
            }
        }
    }

    pub(super) fn supported(&self, session: &str) -> bool {
        self.routes
            .get(session)
            .is_some_and(|route| route.advertised)
    }

    pub(super) fn begin(
        &mut self,
        session: &str,
        origin: ControlOrigin,
    ) -> Result<(), ControlFailure> {
        if self.entries.len() >= MAX_IDS
            && self
                .entries
                .iter()
                .all(|entry| entry.status == CompactionStatus::InProgress)
        {
            return Err(ControlFailure::Busy);
        }
        let route = self
            .routes
            .get_mut(session)
            .ok_or(ControlFailure::StaleTarget)?;
        if route.manual.is_some() {
            return Err(ControlFailure::Busy);
        }
        route.manual = Some(Manual {
            after: self.sequence,
            id: None,
            origin,
        });
        Ok(())
    }

    /// Return a transition only on the first completed terminal for this ID.
    pub(super) fn update(
        &mut self,
        session: &str,
        value: &serde_json::Value,
    ) -> Option<Option<ControlOrigin>> {
        let update: CompactionUpdate = serde_json::from_value(value.clone()).ok()?;
        if update.compaction_id.is_empty() || update.compaction_id.len() > 128 {
            return None;
        }
        let route = self.routes.get_mut(session)?;
        let existing = self.entries.iter().position(|entry| {
            entry.session == session
                && entry.generation == route.generation
                && entry.id == update.compaction_id
        });
        let index = match existing {
            Some(index) => index,
            None => {
                if self.entries.len() >= MAX_IDS {
                    let terminal = self
                        .entries
                        .iter()
                        .position(|entry| entry.status != CompactionStatus::InProgress)?;
                    self.entries.remove(terminal);
                }
                self.sequence = self.sequence.saturating_add(1);
                self.entries.push_back(Entry {
                    session: session.into(),
                    generation: route.generation,
                    id: update.compaction_id.clone(),
                    sequence: self.sequence,
                    status: CompactionStatus::InProgress,
                });
                self.entries.len() - 1
            }
        };
        let entry = &mut self.entries[index];
        if let Some(manual) = &mut route.manual {
            if entry.sequence > manual.after && manual.id.is_none() {
                manual.id = Some(entry.id.clone());
            }
        }
        if entry.status != CompactionStatus::InProgress {
            return None;
        }
        entry.status = update.status;
        (update.status == CompactionStatus::Completed).then(|| {
            route
                .manual
                .as_ref()
                .filter(|manual| manual.id.as_deref() == Some(entry.id.as_str()))
                .map(|manual| manual.origin.clone())
        })
    }

    pub(super) fn finish(&mut self, session: &str) -> ControlOutcome {
        let route = self
            .routes
            .get_mut(session)
            .ok_or(ControlFailure::StaleTarget)?;
        let manual = route.manual.take().ok_or(ControlFailure::Unconfirmed)?;
        let entry = self.entries.iter().find(|entry| {
            entry.session == session
                && entry.generation == route.generation
                && Some(entry.id.as_str()) == manual.id.as_deref()
        });
        match entry.map(|entry| entry.status) {
            Some(CompactionStatus::Completed) => Ok(()),
            Some(CompactionStatus::Failed) => Err(ControlFailure::CompactFailed),
            Some(CompactionStatus::Cancelled) => Err(ControlFailure::Cancelled),
            _ => Err(ControlFailure::Unconfirmed),
        }
    }
}
