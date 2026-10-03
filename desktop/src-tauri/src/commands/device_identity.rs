use crate::device_identity::{load_or_create_device_identity, DeviceIdentity};
use tauri::{AppHandle, Manager};

/// Return this installation's public device metadata, creating it if absent.
#[tauri::command]
pub fn get_device_identity(app: AppHandle) -> Result<DeviceIdentity, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("device data directory: {e}"))?;
    let hostname = gethostname::gethostname();
    let label = hostname
        .to_str()
        .ok_or_else(|| "device hostname is not UTF-8".to_string())?;
    load_or_create_device_identity(&directory.join("device.json"), label)
}
