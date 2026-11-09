//! Per-process bootstrap for the operator-selected local B24 launcher.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::Value;

pub(super) const ENV: &str = "QWEN_CODE_PRIVATE_ACP_CAPABILITY";
pub(super) const KEY: &str = "qwen-code/private-parent-capability";

pub(super) fn fresh() -> String {
    // A fresh ephemeral random scalar is used only as 32 bytes of entropy.
    // It is unrelated to any agent identity or signing key.
    let nonce = nostr::Keys::generate();
    URL_SAFE_NO_PAD.encode(nonce.secret_key().to_secret_bytes())
}

pub(super) fn redacted(value: &Value) -> Value {
    let mut safe = value.clone();
    if let Some(meta) = safe
        .pointer_mut("/params/_meta")
        .and_then(Value::as_object_mut)
    {
        if meta.contains_key(KEY) {
            meta.insert(KEY.into(), Value::String("[redacted]".into()));
        }
    }
    safe
}
