use super::*;
use crate::managed_agents::{
    device_home_migration::tests::{app, records, write},
    device_runtime::capture_runtime_fence,
};
use std::cell::RefCell;
use tauri::Manager;
#[tokio::test]
async fn create_import_profile_invocation_never_redirects_after_owner_relay_switch() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<AppState>();
    let (mut raw, _) = records();
    raw[0].share_across_devices = Some(true);
    write(
        &crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap(),
        &raw,
    );
    let fence = capture_runtime_fence(&state).unwrap();
    *state.keys.lock().unwrap() = nostr::Keys::generate();
    *state.relay_url_override.lock().unwrap() = Some("wss://other".into());
    let destinations = RefCell::new(vec![]);
    let result = publish_profile_with(
        app.handle(),
        &state,
        &raw[1].pubkey,
        &fence,
        "wss://stored",
        |_, _| panic!("shared proof read"),
        |relay| {
            let destinations = &destinations;
            async move {
                destinations.borrow_mut().push(relay);
                Ok(())
            }
        },
    )
    .await;
    assert!(result.is_err());
    assert!(
        destinations.borrow().is_empty(),
        "postcommit publication followed switched workspace"
    );
}
