//! Postcommit snapshot memory publication uses the importing operation's original scope.
use crate::{
    app_state::AppState,
    managed_agents::{
        device_authority::{original_publication_with, InstanceAuthorityAction},
        device_runtime::RuntimeFence,
        persona_device_view::DevicePolicyContext,
    },
};
use tauri::AppHandle;
/// Guard signing and dispatch for a snapshot entry; failed continuations remain partial imports.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn publish_snapshot_memory_entry_with<
    R: tauri::Runtime,
    Fut: std::future::Future<Output = Result<(), String>>,
>(
    app: &AppHandle<R>,
    state: &AppState,
    pubkey: &str,
    fence: &RuntimeFence,
    keys: &nostr::Keys,
    body: &buzz_core_pkg::engram::Body,
    created_at: u64,
    context: impl FnOnce(&AppHandle<R>, &AppState) -> Result<DevicePolicyContext, String>,
    submit: impl FnOnce(String, Vec<u8>) -> Fut,
) -> Result<(), String> {
    original_publication_with(
        app,
        state,
        pubkey,
        fence,
        InstanceAuthorityAction::PublishHead,
        context,
        |scope| async move {
            let owner = nostr::PublicKey::from_hex(&scope.owner_pubkey)
                .map_err(|e| format!("failed to parse original snapshot owner: {e}"))?;
            // Engram encryption hides plaintext from the final HTTP guard.
            crate::egress_guard::assert_no_key_backup_bytes(
                &body.to_json_bytes(),
                "snapshot memory publication",
            )?;
            let event = buzz_core_pkg::engram::build_event(keys, &owner, body, created_at)
                .map_err(|e| format!("memory build failed: {e}"))?;
            let url = format!(
                "{}/events",
                crate::relay::relay_http_base_url(&scope.relay_url)
            );
            let bytes = nostr::JsonUtil::as_json(&event).into_bytes();
            submit(url, bytes).await
        },
    )
    .await
}
/// Native snapshot adapter; authorization precedes memory signing and the HTTP primitive.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn publish_snapshot_memory_entry(
    app: &AppHandle,
    state: &AppState,
    pubkey: &str,
    fence: &RuntimeFence,
    keys: &nostr::Keys,
    body: &buzz_core_pkg::engram::Body,
    created_at: u64,
    auth_tag: Option<&str>,
) -> Result<(), String> {
    publish_snapshot_memory_entry_with(
        app,
        state,
        pubkey,
        fence,
        keys,
        body,
        created_at,
        crate::managed_agents::persona_device_view::load_device_policy_context,
        |url, bytes| async move {
            crate::commands::submit_engram_event(state, keys, &bytes, &url, auth_tag).await
        },
    )
    .await
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_agents::{
        device_home_migration::tests::{app, records, write},
        device_runtime::capture_runtime_fence,
    };
    use std::cell::RefCell;
    use tauri::Manager;
    #[tokio::test]
    async fn original_snapshot_memory_invocation_never_signs_or_sends_into_switched_scope() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let (mut raw, keys) = records();
        raw[0].share_across_devices = Some(true);
        write(
            &crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap(),
            &raw,
        );
        let fence = capture_runtime_fence(&state).unwrap();
        let body = buzz_core_pkg::engram::Body::Memory {
            slug: "test".into(),
            value: Some("private memory".into()),
        };
        *state.keys.lock().unwrap() = nostr::Keys::generate();
        *state.relay_url_override.lock().unwrap() = Some("wss://other".into());
        let sent = RefCell::new(vec![]);
        let result = publish_snapshot_memory_entry_with(
            app.handle(),
            &state,
            &raw[1].pubkey,
            &fence,
            &keys,
            &body,
            100,
            |_, _| panic!("shared proof read"),
            |url, bytes| {
                let sent = &sent;
                async move {
                    sent.borrow_mut().push((url, bytes));
                    Ok(())
                }
            },
        )
        .await;
        assert!(result.is_err());
        assert!(sent.borrow().is_empty());
    }
}

#[cfg(test)]
mod egress_tests {
    use super::*;
    use crate::managed_agents::{
        device_home_migration::tests::{app, records, write},
        device_runtime::capture_runtime_fence,
    };
    use tauri::Manager;
    #[tokio::test]
    async fn authorized_snapshot_memory_adapter_blocks_key_backup_before_submit() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let (mut raw, keys) = records();
        raw[0].share_across_devices = Some(true);
        write(
            &crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap(),
            &raw,
        );
        let fence = capture_runtime_fence(&state).unwrap();
        let body = buzz_core_pkg::engram::Body::Memory {
            slug: "test".into(),
            value: Some("ncryptsec1forbidden".into()),
        };
        let error = publish_snapshot_memory_entry_with(
            app.handle(),
            &state,
            &raw[1].pubkey,
            &fence,
            &keys,
            &body,
            100,
            |_, _| panic!("shared proof read"),
            |_, _| async { panic!("key backup reached submit") },
        )
        .await
        .unwrap_err();
        assert!(error.contains("ncryptsec"));
    }
}
