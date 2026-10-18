//! Live per-scope session status snapshots.
//!
//! The harness writes one small snapshot per conversation scope so the CLI can
//! tag the agent's outgoing messages with the session's state, the reason for
//! its last transition and the reported context occupancy. Status is display
//! metadata only: it never decides whether a turn runs or a session resumes.
//!
//! # Layout
//!
//! ```text
//! <session root>/status/v1/<identity hash>/<instance UUID>/   (0700)
//!     instance.lock                       held while this harness runs
//!     generations/<generation UUID>.lock  held while that generation is current
//!     <scope file>                        snapshot (0600), see `scope_file_name`
//! ```
//!
//! `BUZZ_SESSION_STATUS_DIR` names the instance directory. The instance
//! directory only appears under its final name once its lock is held, and a
//! snapshot is only published while its generation lock is held. A reader
//! trusts a snapshot only while both locks are held by someone else.
//!
//! Each scope has at most one current generation. A successor generation, a
//! retirement or a failed write revokes the shared guard of the old one: its
//! lock is released (even when deleting its snapshot fails) and no clone of
//! its binding can write again. A scope whose write failed stops producing
//! status for the rest of this runtime.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use buzz_core::session_status::{
    generation_lock_path, scope_file_name, ContextOccupancy, HistoryCoverage, SessionState,
    SessionStatus, StatusEnvelope, StatusIdentity, StatusReason, StatusScope, GENERATIONS_DIR,
    INSTANCE_LOCK_FILE,
};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Environment variable naming the current harness instance's status directory.
pub(crate) const SESSION_STATUS_DIR_ENV: &str = "BUZZ_SESSION_STATUS_DIR";

const STATUS_DIR: &str = "status";
const FORMAT_DIR: &str = "v1";
const STAGING_PREFIX: &str = ".staging-";
/// Sibling instance directories examined per startup sweep.
const MAX_SWEPT_INSTANCES: usize = 64;

/// The status writer of one harness instance. Cheap to clone.
#[derive(Clone)]
pub(crate) struct SessionStatusRuntime {
    shared: Arc<Shared>,
}

struct Shared {
    dir: PathBuf,
    identity: StatusIdentity,
    instance_id: Uuid,
    /// Held for the life of the runtime; see the module docs.
    instance_lock: Option<File>,
    scopes: Mutex<Scopes>,
    #[cfg(test)]
    before_write: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

#[derive(Default)]
struct Scopes {
    current: HashMap<StatusScope, Arc<Generation>>,
    disabled: HashSet<StatusScope>,
}

struct Generation {
    runtime: Weak<Shared>,
    id: Uuid,
    scope: StatusScope,
    session_id: String,
    origin: Option<buzz_core::session_control::ControlOrigin>,
    snapshot: PathBuf,
    lock_path: PathBuf,
    state: Mutex<GenerationState>,
}

struct GenerationState {
    /// The held generation lock; `None` once revoked.
    guard: Option<File>,
    status: SessionStatus,
}

/// A handle on one scope generation. Clones share one revocable guard; none
/// of them keeps the generation alive after it was revoked.
#[derive(Clone)]
pub(crate) struct StatusBinding {
    generation: Arc<Generation>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl SessionStatusRuntime {
    /// Create this instance's directory under `root` and hold its lock before
    /// the directory can be found under its final name.
    pub(crate) fn open(root: &Path, identity: StatusIdentity) -> io::Result<Self> {
        identity
            .validate()
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        let parent = root
            .join(STATUS_DIR)
            .join(FORMAT_DIR)
            .join(identity_dir(&identity)?);
        create_private_dir_all(&parent)?;
        sweep_dead_instances(&parent);

        let instance_id = Uuid::new_v4();
        let staging = parent.join(format!("{STAGING_PREFIX}{instance_id}"));
        let dir = parent.join(instance_id.to_string());
        let instance_lock = (|| {
            create_private_dir(&staging)?;
            let lock = create_lock(&staging.join(INSTANCE_LOCK_FILE))?;
            create_private_dir(&staging.join(GENERATIONS_DIR))?;
            fs::rename(&staging, &dir)?;
            Ok::<_, io::Error>(lock)
        })()
        .inspect_err(|_| {
            let _ = fs::remove_dir_all(&staging);
        })?;
        Ok(Self {
            shared: Arc::new(Shared {
                dir,
                identity,
                instance_id,
                instance_lock: Some(instance_lock),
                scopes: Mutex::new(Scopes::default()),
                #[cfg(test)]
                before_write: Mutex::new(None),
            }),
        })
    }

    /// The instance directory exported as [`SESSION_STATUS_DIR_ENV`].
    pub(crate) fn dir(&self) -> &Path {
        &self.shared.dir
    }

    /// Start a new generation for `scope`: revoke the current one, then
    /// publish the new state with unknown occupancy under a held lock.
    ///
    /// Returns `None` when the scope or reason is invalid, the scope is
    /// disabled, or publication failed (which disables the scope).
    pub(crate) fn begin(
        &self,
        scope: StatusScope,
        session_id: &str,
        state: SessionState,
        reason: StatusReason,
    ) -> Option<StatusBinding> {
        self.begin_with_origin(scope, session_id, state, reason, None)
    }

    /// Start a generation with an authorized manual transition's provenance.
    pub(crate) fn begin_with_origin(
        &self,
        scope: StatusScope,
        session_id: &str,
        state: SessionState,
        reason: StatusReason,
        origin: Option<buzz_core::session_control::ControlOrigin>,
    ) -> Option<StatusBinding> {
        if origin
            .as_ref()
            .is_some_and(|origin| origin.validate().is_err())
        {
            return None;
        }
        let file_name = scope_file_name(&scope).ok()?;
        reason.validate().ok()?;
        let mut scopes = lock(&self.shared.scopes);
        if scopes.disabled.contains(&scope) {
            return None;
        }
        if let Some(previous) = scopes.current.remove(&scope) {
            previous.revoke();
        }
        let id = Uuid::new_v4();
        let lock_path = self.shared.dir.join(generation_lock_path(id));
        let guard = match create_lock(&lock_path) {
            Ok(guard) => guard,
            Err(err) => {
                disable(&mut scopes, &scope, &err);
                return None;
            }
        };
        let generation = Arc::new(Generation {
            runtime: Arc::downgrade(&self.shared),
            id,
            scope: scope.clone(),
            session_id: session_id.to_owned(),
            origin,
            snapshot: self.shared.dir.join(file_name),
            lock_path,
            state: Mutex::new(GenerationState {
                guard: Some(guard),
                status: SessionStatus {
                    state,
                    reason,
                    transition_id: Uuid::new_v4(),
                    occupancy: None,
                    history: None,
                },
            }),
        });
        {
            let mut generation_state = lock(&generation.state);
            let status = generation_state.status.clone();
            if let Err(err) = generation.write(&self.shared, &status) {
                generation.revoke_locked(&mut generation_state);
                disable(&mut scopes, &scope, &err);
                return None;
            }
        }
        scopes.current.insert(scope, generation.clone());
        Some(StatusBinding { generation })
    }

    /// The current generation of `scope`, if it belongs to `session_id`.
    pub(crate) fn bind_existing(
        &self,
        scope: &StatusScope,
        session_id: &str,
    ) -> Option<StatusBinding> {
        let scopes = lock(&self.shared.scopes);
        let generation = scopes.current.get(scope)?;
        (generation.session_id == session_id && generation.is_live()).then(|| StatusBinding {
            generation: generation.clone(),
        })
    }

    /// Retire the current generation of `scope`, whoever owns it.
    pub(crate) fn retire(&self, scope: &StatusScope) {
        let mut scopes = lock(&self.shared.scopes);
        if let Some(generation) = scopes.current.remove(scope) {
            generation.revoke();
        }
    }

    /// Retire every scope of `channel_id`.
    pub(crate) fn retire_channel(&self, channel_id: Uuid) {
        let mut scopes = lock(&self.shared.scopes);
        let retired: Vec<StatusScope> = scopes
            .current
            .keys()
            .filter(|scope| scope.channel_id == channel_id)
            .cloned()
            .collect();
        for scope in retired {
            if let Some(generation) = scopes.current.remove(&scope) {
                generation.revoke();
            }
        }
    }

    /// Run `hook` inside the next snapshot write, before it is renamed into
    /// place, so a test can hold a writer mid-flight.
    #[cfg(test)]
    pub(crate) fn delay_next_write(&self, hook: Box<dyn FnOnce() + Send>) {
        *lock(&self.shared.before_write) = Some(hook);
    }
}

impl StatusBinding {
    /// Record the cold-start history coverage of this generation.
    pub(crate) fn set_history(&self, history: HistoryCoverage) {
        if history.validate().is_err() {
            return;
        }
        self.generation.update(|status| {
            let changed = status.history != Some(history);
            status.history = Some(history);
            changed
        });
    }

    /// Record a reported context occupancy; a zero size is unavailable.
    pub(crate) fn record_occupancy(&self, used: u64, size: u64) {
        let Some(occupancy) = ContextOccupancy::new(used, size) else {
            return;
        };
        self.generation.update(|status| {
            let changed = status.occupancy != Some(occupancy);
            status.occupancy = Some(occupancy);
            changed
        });
    }

    /// Retire this generation. A generation that is no longer current was
    /// already revoked, so this never touches a successor.
    pub(crate) fn retire(&self) {
        let generation = &self.generation;
        match generation.runtime.upgrade() {
            Some(shared) => {
                let mut scopes = lock(&shared.scopes);
                if scopes
                    .current
                    .get(&generation.scope)
                    .is_some_and(|current| Arc::ptr_eq(current, generation))
                {
                    scopes.current.remove(&generation.scope);
                }
                generation.revoke();
            }
            None => generation.revoke(),
        }
    }

    /// Whether this generation can still publish.
    pub(crate) fn is_live(&self) -> bool {
        self.generation.is_live()
    }

    /// Provider session this generation describes.
    #[cfg(test)]
    pub(crate) fn session_id(&self) -> &str {
        &self.generation.session_id
    }
}

impl Generation {
    fn is_live(&self) -> bool {
        lock(&self.state).guard.is_some()
    }

    /// Apply `change` and republish; a revoked generation ignores it.
    fn update(&self, change: impl FnOnce(&mut SessionStatus) -> bool) {
        let Some(shared) = self.runtime.upgrade() else {
            return;
        };
        let mut state = lock(&self.state);
        if state.guard.is_none() {
            return;
        }
        let mut next = state.status.clone();
        if !change(&mut next) {
            return;
        }
        match self.write(&shared, &next) {
            Ok(()) => state.status = next,
            Err(err) => {
                self.revoke_locked(&mut state);
                drop(state);
                let mut scopes = lock(&shared.scopes);
                if scopes
                    .current
                    .get(&self.scope)
                    .is_some_and(|current| current.id == self.id)
                {
                    scopes.current.remove(&self.scope);
                }
                disable(&mut scopes, &self.scope, &err);
            }
        }
    }

    /// Atomically replace the snapshot: a bounded temp file in the same
    /// directory, renamed into place.
    fn write(&self, shared: &Shared, status: &SessionStatus) -> io::Result<()> {
        let mut envelope = StatusEnvelope::new(
            shared.identity.clone(),
            shared.instance_id,
            self.scope.clone(),
            self.id,
            status.clone(),
        );
        envelope.origin = self.origin.clone();
        let bytes = envelope
            .to_snapshot_bytes()
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        let mut file = tempfile::Builder::new()
            .prefix(".snapshot-")
            .tempfile_in(&shared.dir)?;
        file.write_all(&bytes)?;
        #[cfg(test)]
        if let Some(hook) = lock(&shared.before_write).take() {
            hook();
        }
        file.persist(&self.snapshot).map_err(|err| err.error)?;
        Ok(())
    }

    fn revoke(&self) {
        let mut state = lock(&self.state);
        self.revoke_locked(&mut state);
    }

    /// Release the generation lock even when the snapshot cannot be removed:
    /// a reader then rejects whatever file remains.
    fn revoke_locked(&self, state: &mut GenerationState) {
        let Some(guard) = state.guard.take() else {
            return;
        };
        let _ = fs::remove_file(&self.snapshot);
        let _ = FileExt::unlock(&guard);
        drop(guard);
        let _ = fs::remove_file(&self.lock_path);
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        let scopes = std::mem::take(&mut *lock(&self.scopes));
        for generation in scopes.current.values() {
            generation.revoke();
        }
        let _ = fs::remove_dir_all(&self.dir);
        drop(self.instance_lock.take());
    }
}

fn disable(scopes: &mut Scopes, scope: &StatusScope, err: &io::Error) {
    if scopes.disabled.insert(scope.clone()) {
        tracing::warn!(
            target: "session_status",
            scope = scope.kind(),
            error = %err.kind(),
            "session status disabled for this scope"
        );
    }
}

/// Route a standard `usage_update` to the binding of its own
/// `params.sessionId`. Missing or malformed values are ignored.
pub(crate) fn record_usage_update(
    bindings: &HashMap<String, StatusBinding>,
    msg: &serde_json::Value,
) {
    let Some(binding) = msg
        .pointer("/params/sessionId")
        .and_then(serde_json::Value::as_str)
        .and_then(|session_id| bindings.get(session_id))
    else {
        return;
    };
    let update = &msg["params"]["update"];
    if let Some(occupancy) = ContextOccupancy::from_json(update.get("used"), update.get("size")) {
        binding.record_occupancy(occupancy.used, occupancy.size);
    }
}

/// The harness owns [`SESSION_STATUS_DIR_ENV`]: set it from `extra_env` or
/// remove an inherited value naming another harness instance.
pub(crate) fn apply_status_dir_env(
    cmd: &mut tokio::process::Command,
    extra_env: &[(String, String)],
) {
    cmd.env_remove(SESSION_STATUS_DIR_ENV);
    if let Some((name, dir)) = extra_env
        .iter()
        .rev()
        .find(|(name, _)| name == SESSION_STATUS_DIR_ENV)
    {
        cmd.env(name, dir);
    }
}

/// Open the status runtime of `config`'s session root, or `None` (with a
/// diagnostic) when status cannot be published.
pub(crate) fn open_for_config(config: &crate::config::Config) -> Option<SessionStatusRuntime> {
    let root = match &config.session_store_dir {
        Some(dir) => dir.clone(),
        None => std::env::current_dir().ok()?.join(".buzz-sessions"),
    };
    let identity = buzz_core::session_status::canonical_identity(
        &config.relay_url,
        &config.keys.public_key().to_hex(),
    )
    .ok()?;
    match SessionStatusRuntime::open(&root, identity) {
        Ok(runtime) => Some(runtime),
        Err(err) => {
            tracing::warn!(
                target: "session_status",
                error = %err.kind(),
                "session status unavailable"
            );
            None
        }
    }
}

fn identity_dir(identity: &StatusIdentity) -> io::Result<String> {
    let encoded = serde_json::to_vec(&("buzz-session-status-v1", &identity.relay, &identity.agent))
        .map_err(io::Error::other)?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn create_private_dir_all(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)
}

fn create_private_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    let mut builder = fs::DirBuilder::new();
    #[cfg(not(unix))]
    let builder = fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)
}

/// Create a fresh lock file (0600) and hold an exclusive lock on it.
fn create_lock(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let file = options.open(path)?;
    file.try_lock_exclusive()?;
    Ok(file)
}

/// Remove sibling instance directories whose instance lock nobody holds.
/// Bounded; directories still being created (staging names) are skipped.
fn sweep_dead_instances(parent: &Path) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten().take(MAX_SWEPT_INSTANCES) {
        let path = entry.path();
        let is_instance = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| Uuid::parse_str(name).is_ok());
        if !is_instance {
            continue;
        }
        let Ok(file) = OpenOptions::new()
            .read(true)
            .open(path.join(INSTANCE_LOCK_FILE))
        else {
            continue;
        };
        if file.try_lock_exclusive().is_ok() {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

#[cfg(all(test, unix))]
#[path = "session_status_tests.rs"]
mod tests;
