use super::*;
use crate::secret_store::TestBlobBackend;
use std::sync::{Arc, Mutex};

struct MemoryBlob {
    bytes: Mutex<Option<Vec<u8>>>,
    failure: &'static str,
}
impl TestBlobBackend for MemoryBlob {
    fn read(&self) -> Result<Option<Vec<u8>>, String> {
        if self.failure == "load" {
            return Err("injected load failure".into());
        }
        let bytes = self.bytes.lock().unwrap().clone();
        if self.failure == "verify" && bytes.is_some() {
            return Ok(Some(b"{}".to_vec()));
        }
        if self.failure == "verify-read" && bytes.is_some() {
            return Err("injected verify failure".into());
        }
        Ok(bytes)
    }
    fn write(&self, bytes: &[u8]) -> Result<(), String> {
        if self.failure == "store" {
            return Err("injected store failure".into());
        }
        *self.bytes.lock().unwrap() = Some(bytes.to_vec());
        Ok(())
    }
}
fn store(failure: &'static str) -> SecretStore {
    let mut store = SecretStore::keyring(format!("buzz-device-test-{}", uuid::Uuid::new_v4()));
    store.test_backend = Some(Arc::new(MemoryBlob {
        bytes: Mutex::new(None),
        failure,
    }));
    store
}

#[test]
fn identity_child_process() {
    let Some(path) = std::env::var_os("BUZZ_DEVICE_TEST_IDENTITY_PATH") else {
        return;
    };
    let ready = std::env::var_os("BUZZ_DEVICE_TEST_READY").unwrap();
    std::fs::write(ready, b"ready").unwrap();
    let gate = std::env::var_os("BUZZ_DEVICE_TEST_GATE").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !Path::new(&gate).exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "parent did not release child"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let identity = load_or_create_device_identity(Path::new(&path), " Child host ").unwrap();
    let output = std::env::var_os("BUZZ_DEVICE_TEST_OUTPUT").unwrap();
    std::fs::write(output, identity.device_id).unwrap();
}

#[test]
fn concurrent_identity_initialization_has_one_uuid() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device.json");
    let gate = dir.path().join("gate");
    let mut children = Vec::new();
    for index in 0..8 {
        let output = dir.path().join(format!("result-{index}"));
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "device_identity::tests::identity_child_process",
                "--nocapture",
            ])
            .env("BUZZ_DEVICE_TEST_IDENTITY_PATH", &path)
            .env("BUZZ_DEVICE_TEST_OUTPUT", &output)
            .env(
                "BUZZ_DEVICE_TEST_READY",
                dir.path().join(format!("ready-{index}")),
            )
            .env("BUZZ_DEVICE_TEST_GATE", &gate)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        children.push((child, output));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !(0..8).all(|index| dir.path().join(format!("ready-{index}")).exists()) {
        assert!(
            std::time::Instant::now() < deadline,
            "children did not become ready"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    std::fs::write(&gate, b"go").unwrap();
    for (child, _) in &mut children {
        assert!(child.wait().unwrap().success());
    }
    let identity = load_or_create_device_identity(&path, "other host").unwrap();
    for (_, output) in children {
        assert_eq!(identity.device_id, std::fs::read_to_string(output).unwrap());
    }
    assert_eq!(
        uuid::Uuid::parse_str(&identity.device_id)
            .unwrap()
            .get_version_num(),
        4
    );
    assert_eq!(identity.label, "Child host");
    chrono::DateTime::parse_from_rfc3339(&identity.created_at).unwrap();
}

#[test]
fn deleted_device_json_keeps_host_proof() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device.json");
    let store = store("");
    let first = load_or_create_device_identity(&path, "first").unwrap();
    let proof = load_or_create_host_proof(&store).unwrap();
    std::fs::remove_file(&path).unwrap();
    let recreated = load_or_create_device_identity(&path, "second").unwrap();
    let next = load_or_create_host_proof(&store).unwrap();
    assert_ne!(first.device_id, recreated.device_id);
    assert!(next.matches(proof.binding()));
    assert!(!next.matches("different machine"));
    let json = std::fs::read_to_string(path).unwrap();
    assert!(!json.contains(proof.binding()));
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 3);
}

#[test]
fn unavailable_keychain_does_not_rebind() {
    for failure in ["load", "store", "verify", "verify-read"] {
        let unavailable_store = store(failure);
        assert!(
            load_or_create_host_proof(&unavailable_store).is_err(),
            "{failure}"
        );
    }
}

#[test]
fn demo_marker_does_not_touch_production() {
    let production = crate::build_identity::device_host_service_for(None);
    let demo = crate::build_identity::device_host_service_for(Some("board-123"));
    assert_eq!(production, "buzz-desktop-device-host");
    assert_eq!(demo, "buzz-desktop-device-host-demo.board-123");
    assert_ne!(production, demo);
    assert_ne!(
        demo,
        crate::build_identity::device_host_service_for(Some("other"))
    );
}

#[test]
fn malformed_identity_is_preserved_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device.json");
    for bytes in [
        "broken JSON",
        r#"{"device_id":"bad","label":"host","created_at":"now"}"#,
        r#"{"device_id":"fd2f3ed6-a882-4db7-808c-00d6b3ca11a1","label":" ","created_at":"2026-10-03T00:00:00Z"}"#,
    ] {
        std::fs::write(&path, bytes).unwrap();
        assert!(load_or_create_device_identity(&path, "host").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), bytes);
    }
}

#[test]
fn empty_default_label_cannot_create_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device.json");
    assert!(load_or_create_device_identity(&path, "  ").is_err());
    assert!(!path.exists());
}

#[test]
fn verified_value_uses_fresh_storage_and_generates_once() {
    let store = store("");
    let first = store
        .get_or_create_verified("host", || "first".into())
        .unwrap();
    let second = store
        .get_or_create_verified("host", || panic!("must not regenerate"))
        .unwrap();
    assert_eq!(first, "first");
    assert_eq!(second, "first");
    store
        .test_backend
        .as_ref()
        .unwrap()
        .write(br#"{"host":"external","other":"preserved"}"#)
        .unwrap();
    assert_eq!(
        store
            .get_or_create_verified("host", || panic!("fresh read must win"))
            .unwrap(),
        "external"
    );
}

#[test]
fn concurrent_host_proof_initialization_generates_once() {
    struct SlowBlob(MemoryBlob);
    impl TestBlobBackend for SlowBlob {
        fn read(&self) -> Result<Option<Vec<u8>>, String> {
            let result = self.0.read();
            std::thread::sleep(std::time::Duration::from_millis(15));
            result
        }
        fn write(&self, bytes: &[u8]) -> Result<(), String> {
            self.0.write(bytes)
        }
    }
    let backend = Arc::new(SlowBlob(MemoryBlob {
        bytes: Mutex::new(None),
        failure: "",
    }));
    let service = format!("buzz-device-test-{}", uuid::Uuid::new_v4());
    let start = Arc::new(std::sync::Barrier::new(8));
    let generated = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let backend = backend.clone();
            let service = service.clone();
            let start = start.clone();
            let generated = generated.clone();
            std::thread::spawn(move || {
                let mut store = SecretStore::keyring(service);
                store.test_backend = Some(backend);
                start.wait();
                store
                    .get_or_create_verified("host", || {
                        generated.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        uuid::Uuid::new_v4().to_string()
                    })
                    .unwrap()
            })
        })
        .collect();
    let values: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(generated.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(values.iter().all(|value| value == &values[0]));
}
