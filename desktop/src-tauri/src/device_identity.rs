//! Public device identity and separate local keychain proof.
use std::io::Write;
use std::path::Path;

use crate::secret_store::SecretStore;
use serde::{Deserialize, Serialize};

/// Public, durable identity of this Desktop installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceIdentity {
    /// UUID v4 identifying this installation's public metadata.
    pub device_id: String,
    /// Nonempty, trimmed display name of the device.
    pub label: String,
    /// RFC 3339 timestamp of identity creation.
    pub created_at: String,
}

/// Private host marker, available only inside Desktop; never serialized to IPC.
pub(crate) struct HostProof(String);
impl HostProof {
    /// Compare a local instance binding against the verified host marker.
    pub(crate) fn matches(&self, binding: &str) -> bool {
        self.binding() == binding
    }
    /// Marker to persist only in local instance binding records.
    pub(crate) fn binding(&self) -> &str {
        &self.0
    }
}

/// Load the installation identity or atomically create it once across processes.
/// Corrupt existing metadata is reported without replacing it.
pub fn load_or_create_device_identity(
    path: &Path,
    default_label: &str,
) -> Result<DeviceIdentity, String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| format!("device identity directory: {e}"))?;
    let lock_path = path.with_extension("json.lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .map_err(|e| format!("device identity lock open: {e}"))?;
    lock.lock()
        .map_err(|e| format!("device identity lock: {e}"))?;
    // The open file holds the interprocess lock until this function returns.
    match std::fs::read(path) {
        Ok(bytes) => {
            let identity: DeviceIdentity =
                serde_json::from_slice(&bytes).map_err(|e| format!("device identity JSON: {e}"))?;
            validate_identity(&identity)?;
            return Ok(identity);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("device identity read: {e}")),
    }
    let identity = DeviceIdentity {
        device_id: uuid::Uuid::new_v4().to_string(),
        label: default_label.trim().to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    validate_identity(&identity)?;
    let bytes =
        serde_json::to_vec(&identity).map_err(|e| format!("device identity serialize: {e}"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("device identity temporary file: {e}"))?;
    temporary
        .write_all(&bytes)
        .map_err(|e| format!("device identity write: {e}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|e| format!("device identity sync: {e}"))?;
    temporary
        .persist(path)
        .map_err(|e| format!("device identity rename: {e}"))?;
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|dir| dir.sync_all())
        .map_err(|e| format!("device identity directory sync: {e}"))?;
    Ok(identity)
}

fn validate_identity(identity: &DeviceIdentity) -> Result<(), String> {
    let id = uuid::Uuid::parse_str(&identity.device_id)
        .map_err(|e| format!("device identity UUID: {e}"))?;
    if id.get_version_num() != 4 {
        return Err("device identity must use UUID v4".into());
    }
    if identity.label.is_empty() || identity.label.trim() != identity.label {
        return Err("device identity label must be nonempty and trimmed".into());
    }
    chrono::DateTime::parse_from_rfc3339(&identity.created_at)
        .map_err(|e| format!("device identity timestamp: {e}"))?;
    Ok(())
}

/// Load a fresh durable marker or create one in the supplied keychain store.
/// Keychain failures propagate; the marker has no file fallback.
pub(crate) fn load_or_create_host_proof(store: &SecretStore) -> Result<HostProof, String> {
    let marker = store.get_or_create_verified("host", || uuid::Uuid::new_v4().to_string())?;
    let id = uuid::Uuid::parse_str(&marker).map_err(|e| format!("host proof UUID: {e}"))?;
    if id.get_version_num() != 4 {
        return Err("host proof must use UUID v4".into());
    }
    Ok(HostProof(marker))
}

#[cfg(test)]
mod tests;

/// Read existing metadata without creating or repairing installation state.
pub(crate) fn load_existing_device_identity(path: &Path) -> Result<DeviceIdentity, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("device identity read: {e}"))?;
    let identity =
        serde_json::from_slice(&bytes).map_err(|e| format!("device identity JSON: {e}"))?;
    validate_identity(&identity)?;
    Ok(identity)
}
/// Read existing host authority freshly without generating a missing marker.
pub(crate) fn load_existing_host_proof(store: &SecretStore) -> Result<HostProof, String> {
    let marker = store.read_existing_verified("host")?;
    let id = uuid::Uuid::parse_str(&marker).map_err(|e| format!("host proof UUID: {e}"))?;
    if id.get_version_num() != 4 {
        return Err("host proof must use UUID v4".into());
    }
    Ok(HostProof(marker))
}
/// Initialize installation authority in startup, outside read-only policy queries.
pub(crate) fn initialize_device_authority<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<(), String> {
    use tauri::Manager;
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("device data directory: {e}"))?
        .join("device.json");
    let hostname = gethostname::gethostname();
    let label = hostname
        .to_str()
        .ok_or_else(|| "device hostname is not UTF-8".to_string())?;
    load_or_create_device_identity(&path, label)?;
    load_or_create_host_proof(&SecretStore::keyring(
        crate::build_identity::device_host_service(),
    ))?;
    Ok(())
}
