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

/// Public publication state; queued events drain through the existing scoped event-sync.
#[derive(serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceLabelPublication {
    Queued,
    Complete,
}
/// Installation metadata plus honest remote publication state.
#[derive(serde::Serialize)]
pub struct DeviceLabelResult {
    pub identity: DeviceIdentity,
    pub publication: DeviceLabelPublication,
}
/// Rename this installation and durably queue affected home definitions across known scopes.
#[tauri::command]
pub fn set_device_label(
    app: AppHandle,
    state: tauri::State<'_, crate::app_state::AppState>,
    label: String,
) -> Result<DeviceLabelResult, String> {
    let _guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    crate::managed_agents::device_home_operations::label::set_device_label_locked(
        &app, &state, &label,
    )
}
