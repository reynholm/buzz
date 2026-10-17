//! Session-status enrichment through the production `messages send` and
//! `messages deliver` paths.
//!
//! The status directory is built exactly as the agent harness lays it out
//! (instance directory renamed into place with its lock already held,
//! `generations/<UUID>.lock` held while a generation is current, snapshots
//! written to a temp file and renamed). Locks are held and released with fs2
//! like the harness does, so the CLI reader sees real OS lock state. Events
//! are captured by the signature-checking local mock relay of
//! `delivery_tests`.
use super::delivery_tests::{deliver_with_status, fixture, Relay};
use super::*;
use buzz_core::session_status::{
    canonical_identity, generation_lock_path, scope_file_name, ContextOccupancy, HistoryCoverage,
    SessionState, SessionStatus, StatusEnvelope, StatusIdentity, StatusReason, StatusScope,
    GENERATIONS_DIR, INSTANCE_LOCK_FILE, MAX_SNAPSHOT_BYTES, SESSION_INFO_TAG, SESSION_TAG,
};
use fs2::FileExt;
use nostr::{Event, EventBuilder, Kind, Tag};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Harness-shaped status producer
// ---------------------------------------------------------------------------

struct Generation {
    id: Uuid,
    lock: Option<File>,
    lock_path: PathBuf,
    status: SessionStatus,
}

/// One harness instance directory with the harness's lock protocol.
struct Producer {
    _root: tempfile::TempDir,
    dir: PathBuf,
    identity: StatusIdentity,
    instance_id: Uuid,
    instance_lock: Option<File>,
    current: HashMap<StatusScope, Generation>,
}

/// Create a fresh 0600 lock file and hold an exclusive lock on it.
fn create_lock(path: &Path) -> File {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let file = options.open(path).unwrap();
    file.try_lock_exclusive().unwrap();
    file
}

impl Producer {
    fn start(relay: &str, keys: &Keys) -> Self {
        Self::with_identity(canonical_identity(relay, &keys.public_key().to_hex()).unwrap())
    }

    fn with_identity(identity: StatusIdentity) -> Self {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("status").join("v1").join("identity");
        fs::create_dir_all(&parent).unwrap();
        let instance_id = Uuid::new_v4();
        let staging = parent.join(format!(".staging-{instance_id}"));
        fs::create_dir(&staging).unwrap();
        let instance_lock = create_lock(&staging.join(INSTANCE_LOCK_FILE));
        fs::create_dir(staging.join(GENERATIONS_DIR)).unwrap();
        let dir = parent.join(instance_id.to_string());
        fs::rename(&staging, &dir).unwrap();
        Self {
            _root: root,
            dir,
            identity,
            instance_id,
            instance_lock: Some(instance_lock),
            current: HashMap::new(),
        }
    }

    fn snapshot_path(&self, scope: &StatusScope) -> PathBuf {
        self.dir.join(scope_file_name(scope).unwrap())
    }

    /// Start a new transition for `scope`, revoking its predecessor first.
    fn begin(&mut self, scope: &StatusScope, state: SessionState, reason: StatusReason) -> Uuid {
        self.revoke(scope);
        let id = Uuid::new_v4();
        let lock_path = self.dir.join(generation_lock_path(id));
        let lock = create_lock(&lock_path);
        let status = SessionStatus {
            state,
            reason,
            transition_id: Uuid::new_v4(),
            occupancy: None,
            history: None,
        };
        let transition = status.transition_id;
        self.current.insert(
            scope.clone(),
            Generation {
                id,
                lock: Some(lock),
                lock_path,
                status,
            },
        );
        self.write(scope);
        transition
    }

    /// Update the current generation's status in place (same transition).
    fn update(&mut self, scope: &StatusScope, change: impl FnOnce(&mut SessionStatus)) {
        change(&mut self.current.get_mut(scope).unwrap().status);
        self.write(scope);
    }

    fn envelope(&self, scope: &StatusScope) -> StatusEnvelope {
        let generation = &self.current[scope];
        StatusEnvelope::new(
            self.identity.clone(),
            self.instance_id,
            scope.clone(),
            generation.id,
            generation.status.clone(),
        )
    }

    fn write(&self, scope: &StatusScope) {
        assert!(
            self.current[scope].lock.is_some(),
            "write needs a held lock"
        );
        let bytes = self.envelope(scope).to_snapshot_bytes().unwrap();
        let mut file = tempfile::Builder::new()
            .prefix(".snapshot-")
            .tempfile_in(&self.dir)
            .unwrap();
        file.write_all(&bytes).unwrap();
        file.persist(self.snapshot_path(scope)).unwrap();
    }

    /// Ordinary revocation: remove the snapshot, release and unlink the lock.
    fn revoke(&mut self, scope: &StatusScope) {
        if let Some(mut generation) = self.current.remove(scope) {
            let _ = fs::remove_file(self.snapshot_path(scope));
            if let Some(lock) = generation.lock.take() {
                FileExt::unlock(&lock).unwrap();
            }
            let _ = fs::remove_file(&generation.lock_path);
        }
    }

    /// Revocation whose snapshot removal failed: the generation lock is
    /// released (and optionally unlinked) while the snapshot stays.
    fn revoke_leaving_snapshot(&mut self, scope: &StatusScope, unlink_lock: bool) {
        let generation = self.current.get_mut(scope).unwrap();
        let lock = generation.lock.take().unwrap();
        FileExt::unlock(&lock).unwrap();
        drop(lock);
        if unlink_lock {
            fs::remove_file(&generation.lock_path).unwrap();
        }
    }

    /// The harness process is gone: its instance lock is released while
    /// every file (and, here, even the generation lock) is left in place.
    fn release_instance(&mut self) {
        let lock = self.instance_lock.take().unwrap();
        FileExt::unlock(&lock).unwrap();
    }

    fn generation_lock_path(&self, scope: &StatusScope) -> PathBuf {
        self.current[scope].lock_path.clone()
    }

    /// Every entry under the instance directory with its bytes.
    fn tree(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut out = BTreeMap::new();
        let mut stack = vec![self.dir.clone()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                let meta = fs::symlink_metadata(&path).unwrap();
                if meta.is_dir() {
                    stack.push(path.clone());
                    out.insert(path, Vec::new());
                } else {
                    out.insert(path.clone(), fs::read(&path).unwrap_or_default());
                }
            }
        }
        out
    }
}

/// Whether some other open file description holds `path`'s lock.
fn held_elsewhere(path: &Path) -> bool {
    let file = File::open(path).unwrap();
    match file.try_lock_exclusive() {
        Ok(()) => {
            FileExt::unlock(&file).unwrap();
            false
        }
        Err(_) => true,
    }
}

fn thread_scope(channel: &str, root: &nostr::EventId) -> StatusScope {
    StatusScope {
        channel_id: Uuid::parse_str(channel).unwrap(),
        thread_root: Some(root.to_hex()),
    }
}

fn conversation_scope(channel: &str) -> StatusScope {
    StatusScope {
        channel_id: Uuid::parse_str(channel).unwrap(),
        thread_root: None,
    }
}

/// Transition A: a new session with known occupancy and exact history.
fn begin_a(producer: &mut Producer, scope: &StatusScope) -> Uuid {
    let transition = producer.begin(scope, SessionState::New, StatusReason::FirstInThread);
    producer.update(scope, |status| {
        status.occupancy = ContextOccupancy::new(64, 100);
        status.history = Some(HistoryCoverage {
            read: Some(3),
            total: Some(3),
            exact: true,
            incomplete: false,
        });
    });
    transition
}

/// Transition B: a resumed session whose occupancy is not known yet.
fn begin_b(producer: &mut Producer, scope: &StatusScope) -> Uuid {
    producer.begin(scope, SessionState::Resumed, StatusReason::IdleSleep)
}

fn expected_a(transition: Uuid, kind: &str) -> Vec<Vec<String>> {
    vec![
        vec![
            SESSION_TAG.into(),
            "new".into(),
            "first_in_thread".into(),
            "64".into(),
        ],
        vec![
            SESSION_INFO_TAG.into(),
            "1".into(),
            transition.to_string(),
            kind.into(),
            "3".into(),
            "3".into(),
            "exact".into(),
        ],
    ]
}

fn expected_b(transition: Uuid, kind: &str) -> Vec<Vec<String>> {
    vec![
        vec![
            SESSION_TAG.into(),
            "resumed".into(),
            "idle_sleep".into(),
            String::new(),
        ],
        vec![
            SESSION_INFO_TAG.into(),
            "1".into(),
            transition.to_string(),
            kind.into(),
            String::new(),
            String::new(),
            "unknown".into(),
        ],
    ]
}

// ---------------------------------------------------------------------------
// CLI helpers
// ---------------------------------------------------------------------------

fn client(r: &Relay, url: &str, status_dir: Option<&Path>) -> BuzzClient {
    BuzzClient::new(url.into(), r.keys.clone(), None, None)
        .unwrap()
        .with_session_status_dir(status_dir.map(Path::to_owned))
}

/// Run `buzz messages <args>` through the production dispatcher and return
/// the event the relay received (signature verified).
async fn run_messages(client: &BuzzClient, r: &Relay, args: &[&str]) -> Result<Event, CliError> {
    let before = r.bodies.lock().unwrap().len();
    let mut argv = vec!["buzz", "messages"];
    argv.extend_from_slice(args);
    let cli = parse_args(argv).expect("valid arguments");
    let Cmd::Messages(command) = cli.command else {
        panic!("expected messages command");
    };
    commands::messages::dispatch(command, client, &OutputFormat::Json).await?;
    let bodies = r.bodies.lock().unwrap();
    assert_eq!(bodies.len(), before + 1, "exactly one publication");
    let event: Event = serde_json::from_slice(bodies.last().unwrap()).unwrap();
    event.verify().expect("final event signature verifies");
    Ok(event)
}

async fn send(
    r: &Relay,
    url: &str,
    status_dir: Option<&Path>,
    extra: &[&str],
) -> Result<Event, CliError> {
    let client = client(r, url, status_dir);
    let mut args = vec!["send", "--channel", &r.channel];
    args.extend_from_slice(extra);
    run_messages(&client, r, &args).await
}

fn tags(event: &Event) -> Vec<Vec<String>> {
    event.tags.iter().map(|t| t.as_slice().to_vec()).collect()
}

fn status_tags_of(event: &Event) -> Vec<Vec<String>> {
    tags(event)
        .into_iter()
        .filter(|t| {
            matches!(
                t.first().map(String::as_str),
                Some(SESSION_TAG | SESSION_INFO_TAG)
            )
        })
        .collect()
}

/// Store a kind-9 event in `channel` (optionally a reply) on the mock relay.
fn seed_event(
    r: &Relay,
    keys: &Keys,
    channel: &str,
    content: &str,
    reply: Option<(&Event, &Event)>,
) -> Event {
    let mut builder =
        EventBuilder::new(Kind::Custom(9), content).tag(Tag::parse(["h", channel]).unwrap());
    if let Some((root, parent)) = reply {
        builder = builder
            .tag(Tag::parse(["e", &root.id.to_hex(), "", "root"]).unwrap())
            .tag(Tag::parse(["e", &parent.id.to_hex(), "", "reply"]).unwrap());
    }
    let event = builder.sign_with_keys(keys).unwrap();
    r.events.lock().unwrap().push(event.clone());
    event
}

/// `(root, parent)` of a reply under the shared NIP-10 rules.
fn thread_of(event: &Event) -> Option<(String, String)> {
    buzz_core::nip10::parse_thread_markers(&event.tags).resolve()
}

fn thread_root(event: &Event) -> Option<String> {
    thread_of(event).map(|(root, _)| root)
}

// ---------------------------------------------------------------------------
// messages send
// ---------------------------------------------------------------------------

#[tokio::test]
async fn status_reply_carries_thread_status_and_keeps_existing_tags() {
    let (r, url, files) = fixture().await;
    let mut producer = Producer::start(&url, &r.keys);
    let scope = thread_scope(&r.channel, &r.parent.id);
    let transition = begin_a(&mut producer, &scope);

    // Everything the existing send path adds must survive enrichment.
    let mention = Keys::generate().public_key().to_hex();
    r.extra_members.lock().unwrap().push(mention.clone());
    let palette = EventBuilder::new(Kind::Custom(30030), "")
        .tags([
            Tag::parse(["d", "buzz:custom-emoji"]).unwrap(),
            Tag::parse(["emoji", "wave", "https://cdn.example/wave.png"]).unwrap(),
        ])
        .sign_with_keys(&r.keys)
        .unwrap();
    r.palette.lock().unwrap().push(palette);
    let image = files.path().join("pixel.png");
    fs::write(&image, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01").unwrap();
    let owner = Keys::generate();
    let auth_json = buzz_sdk::nip_oa::compute_auth_tag(&owner, &r.keys.public_key(), "").unwrap();
    let auth = buzz_sdk::nip_oa::parse_auth_tag(&auth_json).unwrap();
    let client = BuzzClient::new(
        url.clone(),
        r.keys.clone(),
        Some(auth.clone()),
        Some(auth_json),
    )
    .unwrap()
    .with_session_status_dir(Some(producer.dir.clone()));
    let parent = r.parent.id.to_hex();
    let image_arg = image.to_str().unwrap().to_owned();
    let event = run_messages(
        &client,
        &r,
        &[
            "send",
            "--channel",
            &r.channel,
            "--reply-to",
            &parent,
            "--content",
            "done :wave:",
            "--mention",
            &mention,
            "--broadcast",
            "--file",
            &image_arg,
        ],
    )
    .await
    .unwrap();

    assert_eq!(event.kind.as_u16(), 9);
    assert_eq!(status_tags_of(&event), expected_a(transition, "thread"));
    let all = tags(&event);
    let has = |name: &str| {
        all.iter()
            .any(|t| t.first().map(String::as_str) == Some(name))
    };
    assert_eq!(thread_root(&event).as_deref(), Some(parent.as_str()));
    assert!(all.contains(&vec!["h".into(), r.channel.clone()]));
    assert!(all.contains(&vec!["p".into(), mention]));
    assert!(all.contains(&vec!["broadcast".into(), "1".into()]));
    assert!(has("imeta"), "media tag kept: {all:?}");
    assert!(all.contains(&vec![
        "emoji".into(),
        "wave".into(),
        "https://cdn.example/wave.png".into()
    ]));
    assert!(all.contains(&auth.as_slice().to_vec()), "auth tag kept");
    assert_eq!(
        all.iter()
            .filter(|t| t.first().map(String::as_str) == Some(SESSION_TAG))
            .count(),
        1
    );
}

#[tokio::test]
async fn status_nested_reply_uses_the_canonical_thread_root() {
    let (r, url, _files) = fixture().await;
    let mut producer = Producer::start(&url, &r.keys);
    let scope = thread_scope(&r.channel, &r.parent.id);
    let transition = begin_a(&mut producer, &scope);
    let human = Keys::generate();
    let nested = seed_event(
        &r,
        &human,
        &r.channel,
        "follow-up",
        Some((&r.parent, &r.parent)),
    );

    let event = send(
        &r,
        &url,
        Some(&producer.dir),
        &["--reply-to", &nested.id.to_hex(), "--content", "answer"],
    )
    .await
    .unwrap();

    assert_eq!(
        thread_of(&event),
        Some((r.parent.id.to_hex(), nested.id.to_hex()))
    );
    assert_eq!(status_tags_of(&event), expected_a(transition, "thread"));
}

#[tokio::test]
async fn status_publication_into_another_thread_never_inherits_status() {
    let (r, url, _files) = fixture().await;
    let mut producer = Producer::start(&url, &r.keys);
    let human = Keys::generate();
    let other_root = seed_event(&r, &human, &r.channel, "another topic", None);
    // The worker's last active status belongs to thread A.
    let a = thread_scope(&r.channel, &r.parent.id);
    let transition_a = begin_a(&mut producer, &a);

    let into_b = send(
        &r,
        &url,
        Some(&producer.dir),
        &["--reply-to", &other_root.id.to_hex(), "--content", "x"],
    )
    .await
    .unwrap();
    assert!(status_tags_of(&into_b).is_empty(), "{:?}", tags(&into_b));
    let top_level = send(&r, &url, Some(&producer.dir), &["--content", "top"])
        .await
        .unwrap();
    assert!(status_tags_of(&top_level).is_empty());

    // Two concurrent thread sessions: each destination gets its own status.
    let b = thread_scope(&r.channel, &other_root.id);
    let transition_b = begin_b(&mut producer, &b);
    let into_b = send(
        &r,
        &url,
        Some(&producer.dir),
        &["--reply-to", &other_root.id.to_hex(), "--content", "x"],
    )
    .await
    .unwrap();
    assert_eq!(status_tags_of(&into_b), expected_b(transition_b, "thread"));
    let into_a = send(
        &r,
        &url,
        Some(&producer.dir),
        &["--reply-to", &r.parent.id.to_hex(), "--content", "y"],
    )
    .await
    .unwrap();
    assert_eq!(status_tags_of(&into_a), expected_a(transition_a, "thread"));
}

#[tokio::test]
async fn status_channel_policy_conversation_covers_its_channel_only() {
    let (r, url, _files) = fixture().await;
    let mut producer = Producer::start(&url, &r.keys);
    let transition = begin_a(&mut producer, &conversation_scope(&r.channel));

    let reply = send(
        &r,
        &url,
        Some(&producer.dir),
        &[
            "--reply-to",
            &r.parent.id.to_hex(),
            "--content",
            "in a thread",
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        status_tags_of(&reply),
        expected_a(transition, "conversation")
    );
    let top_level = send(&r, &url, Some(&producer.dir), &["--content", "top"])
        .await
        .unwrap();
    assert_eq!(
        status_tags_of(&top_level),
        expected_a(transition, "conversation")
    );

    // Another channel's conversation status never applies here.
    let other_channel = Uuid::new_v4().to_string();
    let mut elsewhere = Producer::start(&url, &r.keys);
    begin_a(&mut elsewhere, &conversation_scope(&other_channel));
    let plain = send(&r, &url, Some(&elsewhere.dir), &["--content", "top"])
        .await
        .unwrap();
    assert!(status_tags_of(&plain).is_empty());
}

#[tokio::test]
async fn status_dm_reply_uses_the_conversation_scope() {
    let (r, url, _files) = fixture().await;
    let dm = Uuid::new_v4().to_string();
    let human = Keys::generate();
    let trigger = seed_event(&r, &human, &dm, "please check", None);
    let mut producer = Producer::start(&url, &r.keys);
    let transition = begin_b(&mut producer, &conversation_scope(&dm));

    let client = client(&r, &url, Some(&producer.dir));
    let trigger_id = trigger.id.to_hex();
    let event = run_messages(
        &client,
        &r,
        &[
            "send",
            "--channel",
            &dm,
            "--reply-to",
            &trigger_id,
            "--content",
            "checked",
        ],
    )
    .await
    .unwrap();

    assert_eq!(thread_root(&event).as_deref(), Some(trigger_id.as_str()));
    assert_eq!(
        status_tags_of(&event),
        expected_b(transition, "conversation")
    );
}

#[tokio::test]
async fn status_absent_or_invalid_snapshot_sends_without_status_tags() {
    let (r, url, files) = fixture().await;
    let parent = r.parent.id.to_hex();
    let scope = thread_scope(&r.channel, &r.parent.id);
    let reply = ["--reply-to", parent.as_str(), "--content", "report"];

    // Control: a valid snapshot is used, so every case below differs only in
    // the condition it names.
    let mut valid = Producer::start(&url, &r.keys);
    let transition = begin_a(&mut valid, &scope);
    let event = send(&r, &url, Some(&valid.dir), &reply).await.unwrap();
    assert_eq!(status_tags_of(&event), expected_a(transition, "thread"));

    let mut cases: Vec<(&str, Producer, Option<PathBuf>)> = Vec::new();

    let none = Producer::start(&url, &r.keys);
    cases.push(("no status directory", none, None));

    let missing = Producer::start(&url, &r.keys);
    let dir = missing.dir.clone();
    cases.push(("missing snapshot", missing, Some(dir)));

    let corrupt = Producer::start(&url, &r.keys);
    fs::write(corrupt.snapshot_path(&scope), b"{\"version\":1,").unwrap();
    let dir = corrupt.dir.clone();
    cases.push(("corrupt snapshot", corrupt, Some(dir)));

    let mut oversized = Producer::start(&url, &r.keys);
    begin_a(&mut oversized, &scope);
    let mut bytes = oversized.envelope(&scope).to_snapshot_bytes().unwrap();
    bytes.resize(MAX_SNAPSHOT_BYTES + 1, b' ');
    fs::write(oversized.snapshot_path(&scope), bytes).unwrap();
    let dir = oversized.dir.clone();
    cases.push(("oversized snapshot", oversized, Some(dir)));

    let mut dead = Producer::start(&url, &r.keys);
    begin_a(&mut dead, &scope);
    dead.release_instance();
    let dir = dead.dir.clone();
    cases.push(("dead instance", dead, Some(dir)));

    let mut wrong_key = Producer::start(&url, &Keys::generate());
    begin_a(&mut wrong_key, &scope);
    let dir = wrong_key.dir.clone();
    cases.push(("another agent key", wrong_key, Some(dir)));

    let mut wrong_relay = Producer::start("wss://other-community.example", &r.keys);
    begin_a(&mut wrong_relay, &scope);
    let dir = wrong_relay.dir.clone();
    cases.push(("another community relay", wrong_relay, Some(dir)));

    let other_root = seed_event(&r, &r.keys, &r.channel, "other root", None);
    let mut wrong_root = Producer::start(&url, &r.keys);
    begin_a(&mut wrong_root, &thread_scope(&r.channel, &other_root.id));
    let dir = wrong_root.dir.clone();
    cases.push(("another thread root", wrong_root, Some(dir)));

    let other_channel = Uuid::new_v4().to_string();
    let mut wrong_channel = Producer::start(&url, &r.keys);
    begin_a(
        &mut wrong_channel,
        &thread_scope(&other_channel, &r.parent.id),
    );
    let dir = wrong_channel.dir.clone();
    cases.push(("another channel", wrong_channel, Some(dir)));

    // A live snapshot of another scope placed under this scope's name.
    let mut misplaced = Producer::start(&url, &r.keys);
    let foreign = thread_scope(&other_channel, &r.parent.id);
    begin_a(&mut misplaced, &foreign);
    fs::copy(
        misplaced.snapshot_path(&foreign),
        misplaced.snapshot_path(&scope),
    )
    .unwrap();
    let dir = misplaced.dir.clone();
    cases.push(("snapshot names another scope", misplaced, Some(dir)));

    // A live snapshot written by another instance placed in this one.
    let copied = Producer::start(&url, &r.keys);
    let mut source = Producer::start(&url, &r.keys);
    begin_a(&mut source, &scope);
    fs::copy(source.snapshot_path(&scope), copied.snapshot_path(&scope)).unwrap();
    let source_lock = source.generation_lock_path(&scope);
    let copied_lock = copied
        .dir
        .join(source_lock.strip_prefix(&source.dir).unwrap());
    fs::copy(&source_lock, &copied_lock).unwrap();
    let held_copy = File::open(&copied_lock).unwrap();
    held_copy.try_lock_exclusive().unwrap();
    let dir = copied.dir.clone();
    cases.push(("snapshot from another instance", copied, Some(dir)));

    let mut ambiguous = Producer::start(&url, &r.keys);
    begin_a(&mut ambiguous, &scope);
    begin_b(&mut ambiguous, &conversation_scope(&r.channel));
    let dir = ambiguous.dir.clone();
    cases.push(("thread and conversation both live", ambiguous, Some(dir)));

    // The link target is this instance's own valid snapshot, so only the
    // link itself is wrong.
    #[cfg(unix)]
    {
        let mut linked = Producer::start(&url, &r.keys);
        begin_a(&mut linked, &scope);
        let outside = files.path().join("outside-snapshot.json");
        fs::rename(linked.snapshot_path(&scope), &outside).unwrap();
        std::os::unix::fs::symlink(&outside, linked.snapshot_path(&scope)).unwrap();
        let dir = linked.dir.clone();
        cases.push(("snapshot is a symlink", linked, Some(dir)));
    }

    // A relative spelling that does resolve to a valid live instance.
    #[cfg(unix)]
    {
        let mut relative = Producer::start(&url, &r.keys);
        begin_a(&mut relative, &scope);
        let cwd = std::env::current_dir().unwrap();
        let mut relative_dir = PathBuf::new();
        for _ in 1..cwd.components().count() {
            relative_dir.push("..");
        }
        relative_dir.push(relative.dir.strip_prefix("/").unwrap());
        assert!(relative_dir.is_relative() && relative_dir.join(INSTANCE_LOCK_FILE).exists());
        cases.push(("relative directory", relative, Some(relative_dir)));
    }

    let mut renamed = Producer::start(&url, &r.keys);
    begin_a(&mut renamed, &scope);
    let not_uuid = files.path().join("not-an-instance");
    fs::rename(&renamed.dir, &not_uuid).unwrap();
    renamed.dir = not_uuid.clone();
    cases.push(("directory is not an instance", renamed, Some(not_uuid)));

    for (name, _producer, dir) in &cases {
        let event = send(&r, &url, dir.as_deref(), &reply)
            .await
            .unwrap_or_else(|e| panic!("{name}: send must succeed: {e}"));
        assert!(
            status_tags_of(&event).is_empty(),
            "{name}: unexpected status tags {:?}",
            tags(&event)
        );
        assert_eq!(
            thread_root(&event).as_deref(),
            Some(parent.as_str()),
            "{name}: thread kept"
        );
    }
    drop(held_copy);
}

/// Ruling R3: after the producer releases a generation, a snapshot that its
/// failed removal left behind is never used.
#[tokio::test]
async fn status_released_generation_lock_rejects_remaining_snapshot() {
    let (r, url, _files) = fixture().await;
    let reply_args = |id: String| vec!["--reply-to".to_owned(), id, "--content".into(), "r".into()];
    for unlink_lock in [true, false] {
        let mut producer = Producer::start(&url, &r.keys);
        let scope = thread_scope(&r.channel, &r.parent.id);
        let transition = begin_a(&mut producer, &scope);
        let args = reply_args(r.parent.id.to_hex());
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let live = send(&r, &url, Some(&producer.dir), &args).await.unwrap();
        assert_eq!(status_tags_of(&live), expected_a(transition, "thread"));

        let lock_path = producer.generation_lock_path(&scope);
        producer.revoke_leaving_snapshot(&scope, unlink_lock);
        let snapshot = fs::read(producer.snapshot_path(&scope)).unwrap();
        let before = producer.tree();

        let stale = send(&r, &url, Some(&producer.dir), &args).await.unwrap();
        assert!(
            status_tags_of(&stale).is_empty(),
            "unlink_lock={unlink_lock}: {:?}",
            tags(&stale)
        );
        assert_eq!(fs::read(producer.snapshot_path(&scope)).unwrap(), snapshot);
        assert_eq!(producer.tree(), before, "reader changed the directory");
        if !unlink_lock {
            assert!(!held_elsewhere(&lock_path), "reader released its probe");
        }
    }
}

/// Ruling R3: a released instance lock means the harness is gone, even when
/// its snapshot and generation lock still look current.
#[tokio::test]
async fn status_released_instance_lock_rejects_live_looking_snapshot() {
    let (r, url, _files) = fixture().await;
    let mut producer = Producer::start(&url, &r.keys);
    let scope = conversation_scope(&r.channel);
    let transition = begin_a(&mut producer, &scope);
    let live = send(&r, &url, Some(&producer.dir), &["--content", "a"])
        .await
        .unwrap();
    assert_eq!(
        status_tags_of(&live),
        expected_a(transition, "conversation")
    );

    producer.release_instance();
    let before = producer.tree();
    let dead = send(&r, &url, Some(&producer.dir), &["--content", "b"])
        .await
        .unwrap();
    assert!(status_tags_of(&dead).is_empty(), "{:?}", tags(&dead));
    assert_eq!(producer.tree(), before);
    assert!(!held_elsewhere(&producer.dir.join(INSTANCE_LOCK_FILE)));
    assert!(held_elsewhere(&producer.generation_lock_path(&scope)));
}

#[tokio::test]
async fn status_reader_never_mutates_or_takes_live_locks() {
    let (r, url, _files) = fixture().await;
    let mut producer = Producer::start(&url, &r.keys);
    let thread = thread_scope(&r.channel, &r.parent.id);
    let transition = begin_a(&mut producer, &thread);
    let before = producer.tree();
    for _ in 0..3 {
        let event = send(
            &r,
            &url,
            Some(&producer.dir),
            &["--reply-to", &r.parent.id.to_hex(), "--content", "z"],
        )
        .await
        .unwrap();
        assert_eq!(status_tags_of(&event), expected_a(transition, "thread"));
    }
    send(&r, &url, Some(&producer.dir), &["--content", "top"])
        .await
        .unwrap();
    assert_eq!(producer.tree(), before);
    assert!(held_elsewhere(&producer.dir.join(INSTANCE_LOCK_FILE)));
    assert!(held_elsewhere(&producer.generation_lock_path(&thread)));
}

#[tokio::test]
async fn status_failed_send_keeps_status_and_transition() {
    let (r, url, _files) = fixture().await;
    let mut producer = Producer::start(&url, &r.keys);
    let scope = thread_scope(&r.channel, &r.parent.id);
    let transition = begin_a(&mut producer, &scope);
    let before = producer.tree();
    let parent = r.parent.id.to_hex();

    *r.ack.lock().unwrap() = "http-error";
    let failed = send(
        &r,
        &url,
        Some(&producer.dir),
        &["--reply-to", &parent, "--content", "lost"],
    )
    .await;
    assert!(failed.is_err());
    let attempted: Event =
        serde_json::from_slice(r.bodies.lock().unwrap().last().unwrap()).unwrap();
    assert_eq!(status_tags_of(&attempted), expected_a(transition, "thread"));
    assert_eq!(producer.tree(), before);

    *r.ack.lock().unwrap() = "ok";
    let retried = send(
        &r,
        &url,
        Some(&producer.dir),
        &["--reply-to", &parent, "--content", "again"],
    )
    .await
    .unwrap();
    assert_eq!(status_tags_of(&retried), expected_a(transition, "thread"));
}

#[tokio::test]
async fn status_only_kind_nine_messages_are_enriched() {
    let (r, url, _files) = fixture().await;
    let mut producer = Producer::start(&url, &r.keys);
    let thread = thread_scope(&r.channel, &r.parent.id);
    let transition = begin_a(&mut producer, &thread);
    let parent = r.parent.id.to_hex();

    let explicit = send(
        &r,
        &url,
        Some(&producer.dir),
        &["--kind", "9", "--reply-to", &parent, "--content", "k9"],
    )
    .await
    .unwrap();
    assert_eq!(status_tags_of(&explicit), expected_a(transition, "thread"));

    let comment = send(
        &r,
        &url,
        Some(&producer.dir),
        &[
            "--kind",
            "45003",
            "--reply-to",
            &parent,
            "--content",
            "comment",
        ],
    )
    .await
    .unwrap();
    assert_eq!(comment.kind.as_u16(), 45003);
    assert!(status_tags_of(&comment).is_empty());

    begin_a(&mut producer, &conversation_scope(&r.channel));
    let post = send(
        &r,
        &url,
        Some(&producer.dir),
        &["--kind", "45001", "--content", "post"],
    )
    .await
    .unwrap();
    assert_eq!(post.kind.as_u16(), 45001);
    assert!(status_tags_of(&post).is_empty());

    let client = client(&r, &url, Some(&producer.dir));
    let diff = run_messages(
        &client,
        &r,
        &[
            "send-diff",
            "--channel",
            &r.channel,
            "--diff=--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n",
            "--repo",
            "https://example.com/repo",
            "--commit",
            "abcdef1234567890abcdef1234567890abcdef12",
            "--reply-to",
            &parent,
        ],
    )
    .await
    .unwrap();
    assert_ne!(diff.kind.as_u16(), 9);
    assert!(status_tags_of(&diff).is_empty());
}

// ---------------------------------------------------------------------------
// messages deliver (managed journal)
// ---------------------------------------------------------------------------

fn journal_record(dir: &Path) -> serde_json::Value {
    let journal: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("journal.json")).unwrap()).unwrap();
    journal["entries"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone()
}

#[tokio::test]
async fn managed_status_retry_keeps_the_originally_prepared_tags() {
    let (r, url, journal) = fixture().await;
    let mut producer = Producer::start(&url, &r.keys);
    let scope = thread_scope(&r.channel, &r.parent.id);
    let transition_a = begin_a(&mut producer, &scope);
    let key = Uuid::new_v4().to_string();

    // The submit response is lost: the prepared event is journaled.
    *r.ack.lock().unwrap() = "malformed";
    assert_eq!(
        deliver_with_status(&r, &url, journal.path(), &key, "final", Some(&producer.dir)).await,
        2
    );
    let saved: Event =
        serde_json::from_value(journal_record(journal.path())["event"].clone()).unwrap();
    assert_eq!(status_tags_of(&saved), expected_a(transition_a, "thread"));

    // Live status moves on to transition B before the retry.
    let transition_b = begin_b(&mut producer, &scope);
    *r.ack.lock().unwrap() = "ok";
    assert_eq!(
        deliver_with_status(
            &r,
            &url,
            journal.path(),
            &key,
            "changed",
            Some(&producer.dir)
        )
        .await,
        0
    );
    {
        let bodies = r.bodies.lock().unwrap();
        assert_eq!(bodies.len(), 2);
        assert_eq!(
            bodies[0], bodies[1],
            "retry resubmits the same signed event"
        );
        let events = r.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, saved.id);
        assert_eq!(
            status_tags_of(&events[0]),
            expected_a(transition_a, "thread")
        );
    }

    // A new delivery key prepares a new event with the current status.
    assert_eq!(
        deliver_with_status(
            &r,
            &url,
            journal.path(),
            &Uuid::new_v4().to_string(),
            "next",
            Some(&producer.dir)
        )
        .await,
        0
    );
    let events = r.events.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(
        status_tags_of(&events[1]),
        expected_b(transition_b, "thread")
    );
}

#[tokio::test]
async fn managed_status_timestamp_recovery_preserves_original_tags() {
    let (r, url, journal) = fixture().await;
    let mut producer = Producer::start(&url, &r.keys);
    let scope = thread_scope(&r.channel, &r.parent.id);
    let transition_a = begin_a(&mut producer, &scope);
    let key = Uuid::new_v4().to_string();
    *r.ack.lock().unwrap() = "malformed";
    assert_eq!(
        deliver_with_status(&r, &url, journal.path(), &key, "final", Some(&producer.dir)).await,
        2
    );

    // Age the journaled event past the recovery threshold, keeping its tags.
    let path = journal.path().join("journal.json");
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let record = value["entries"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();
    let first: Event = serde_json::from_value(record["event"].clone()).unwrap();
    let old = EventBuilder::new(first.kind, &first.content)
        .tags(first.tags.clone())
        .allow_self_tagging()
        .custom_created_at(nostr::Timestamp::from(
            nostr::Timestamp::now().as_secs() - 3600,
        ))
        .sign_with_keys(&r.keys)
        .unwrap();
    record["event"] = serde_json::to_value(&old).unwrap();
    fs::write(&path, value.to_string()).unwrap();

    let transition_b = begin_b(&mut producer, &scope);
    *r.ack.lock().unwrap() = "ok";
    // The mock relay has no atomic recovery, so the delivery stays pending;
    // what matters is the signed candidate it journaled and submitted.
    assert_eq!(
        deliver_with_status(
            &r,
            &url,
            journal.path(),
            &key,
            "changed",
            Some(&producer.dir)
        )
        .await,
        2
    );
    let record = journal_record(journal.path());
    let original: Event = serde_json::from_value(record["event"].clone()).unwrap();
    let stamp: buzz_core::managed_delivery::ReportStamp =
        serde_json::from_value(record["recovery_candidate"].clone()).unwrap();
    let candidate = stamp.restore(&original).unwrap();
    assert_eq!(
        status_tags_of(&original),
        expected_a(transition_a, "thread")
    );
    assert_eq!(
        status_tags_of(&candidate),
        expected_a(transition_a, "thread")
    );
    let wrapper: Event = serde_json::from_slice(r.bodies.lock().unwrap().last().unwrap()).unwrap();
    let request: buzz_core::managed_delivery::RecoveryRequest =
        serde_json::from_str(&wrapper.content).unwrap();
    assert_eq!(request.original, original);
    assert!(status_tags_of(&wrapper).is_empty());

    // A fresh key still captures the current transition.
    assert_eq!(
        deliver_with_status(
            &r,
            &url,
            journal.path(),
            &Uuid::new_v4().to_string(),
            "next",
            Some(&producer.dir)
        )
        .await,
        0
    );
    let latest: Event = serde_json::from_slice(r.bodies.lock().unwrap().last().unwrap()).unwrap();
    assert_eq!(status_tags_of(&latest), expected_b(transition_b, "thread"));
}
