//! Optional NIP-FI verified profiles. Errors here never change admission.

use crate::{nip_fi_config::CommunitySource, state::AppState};
use buzz_auth::VerifiedAssertion;
use buzz_core::{StoredEvent, TenantContext};
use buzz_db::verified_profiles::{ProfileAdmission, ProfilePolicy};
use std::sync::Arc;

/// Policies that may publish: none without a stable relay key or NIP-FI
/// enforcement, since a stale name is only harmless when every post needs a
/// fresh issuer assertion.
fn enabled_policies(state: &AppState) -> impl Iterator<Item = &ProfilePolicy> {
    let enabled = state.config.relay_private_key.is_some() && state.config.nip_fi.mode.enforces();
    state
        .config
        .nip_fi
        .profile_policies
        .iter()
        .filter(move |policy| enabled && policy.label.is_some())
}

/// Run only after a successful WebSocket admission; HTTP never calls this.
/// Agent tokens and missing or invalid claims publish nothing.
pub(crate) fn admitted(
    state: &Arc<AppState>,
    tenant: &TenantContext,
    assertion: Option<&VerifiedAssertion>,
) {
    let Some(assertion) = assertion else {
        return;
    };
    let (Some(evidence), Some(key)) = (assertion.profile_assertion(), assertion.asserted_key())
    else {
        return;
    };
    if evidence.profile.is_none() {
        return;
    }
    let issuer = assertion.identity().issuer();
    let Some(policy) = enabled_policies(state).find(|p| p.issuer == issuer) else {
        return;
    };
    let state = Arc::clone(state);
    let tenant = tenant.clone();
    let evidence = evidence.clone();
    let policy = policy.clone();
    let pubkey = key.to_hex();
    let admitted_at = chrono::Utc::now();
    let work = state.track_accepted_child(crate::deploy_drain::WorkKind::Background);
    tokio::spawn(crate::deploy_drain::run_accepted(work, async move {
        #[cfg(test)]
        crate::nip_fi_test_hooks::before_deploy_post_commit(tenant.community()).await;
        let (Some(profile), Some(label)) = (evidence.profile.as_ref(), policy.label.as_deref())
        else {
            return;
        };
        let result = state
            .db
            .publish_verified_profile(
                ProfileAdmission {
                    community: tenant.community(),
                    issuer: &policy.issuer,
                    pubkey: &pubkey,
                    issued_at: evidence.issued_at,
                    admitted_at,
                    display_name: &profile.display_name,
                    username: &profile.username,
                    full_name: profile.full_name.as_deref(),
                    label,
                    refresh_seconds: policy.refresh_seconds,
                    membership_required: state.config.require_relay_membership,
                },
                &state.relay_keypair,
            )
            .await;
        // A failed publish is not retried; the next admission republishes.
        let event = match result {
            Ok(Some(event)) => event,
            Ok(None) => {
                // Unchanged, ineligible, or superseded by a newer verification.
                metrics::counter!("buzz_verified_profile_total", "outcome" => "not_written")
                    .increment(1);
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "verified profile publication failed");
                metrics::counter!("buzz_verified_profile_total", "outcome" => "publication_failed")
                    .increment(1);
                return;
            }
        };
        deliver(&state, &tenant, &event).await;
    }));
}

/// Deliver once to local subscribers and other pods. Clients that miss it
/// read the stored event on their next fetch.
async fn deliver(state: &AppState, tenant: &TenantContext, event: &nostr::Event) {
    let community = tenant.community();
    state.mark_local_event(community, &event.id);
    let outcome = if state
        .pubsub
        .publish_event(tenant, buzz_pubsub::EventTopic::Global, event)
        .await
        .is_ok()
    {
        "published"
    } else {
        state
            .local_event_ids
            .invalidate(&(community, event.id.to_bytes()));
        "broadcast_failed"
    };
    let stored = StoredEvent::with_received_at(event.clone(), chrono::Utc::now(), None, true);
    crate::handlers::event::fan_out_event_to_local_subscribers(state, community, &stored).await;
    metrics::counter!("buzz_verified_profile_total", "outcome" => outcome).increment(1);
}

/// Community-authorized, stable-key capability. Failure omits discovery.
pub(crate) async fn discovery(
    state: &AppState,
    tenant: &TenantContext,
) -> Option<serde_json::Value> {
    let mut count = 0;
    for policy in enabled_policies(state) {
        let authorized = match state.config.nip_fi.source {
            CommunitySource::Env => state
                .config
                .nip_fi
                .communities
                .resolve(tenant.host())
                .is_some_and(|binding| binding.authorizes(&policy.issuer)),
            CommunitySource::Database => state
                .db
                .lookup_nip_fi_binding_by_host(tenant.host())
                .await
                .ok()
                .flatten()
                .is_some_and(|b| b.active && b.issuers.contains(&policy.issuer)),
        };
        if authorized {
            count += 1;
        }
    }
    // `searchable`: `POST /profiles/search` also matches and returns these.
    (count > 0).then(
        || serde_json::json!({"kind": 30382, "multiple_issuers": count > 1, "searchable": true}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn real_profile_assertion() -> buzz_auth::VerifiedAssertion {
        use crate::nip_fi_core::test_support as fi;
        use buzz_auth::{CommunityBinding, TokenClass};
        use jsonwebtoken::{Algorithm, Header};
        let key = nostr::Keys::generate().public_key();
        let now = chrono::Utc::now().timestamp();
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(fi::TEST_KID.into());
        header.typ = Some("nip-fi+jwt".into());
        let token = jsonwebtoken::encode(
            &header,
            &serde_json::json!({
                "iss": fi::TEST_ISSUER, "aud": fi::TEST_COMMUNITY_URI,
                "sub": "profile-lifecycle-test", "iat": now, "exp": now + 600,
                "nostr_pubkey": key.to_hex(),
                "nostr_public_profile": {"display_name":"Alice", "username":"alice"}
            }),
            &fi::assertion_signing_key().0,
        )
        .unwrap();
        let community =
            CommunityBinding::new(fi::TEST_COMMUNITY_URI.into(), vec![fi::TEST_ISSUER.into()])
                .unwrap();
        let assertion = fi::assertion_verifier(TokenClass::DedicatedNipFi)
            .verify(&token, &community)
            .unwrap();
        assert_eq!(
            assertion
                .profile_assertion()
                .unwrap()
                .profile
                .as_ref()
                .unwrap()
                .username,
            "alice"
        );
        assertion
    }
    #[tokio::test]
    async fn admitted_profile_owns_work_before_spawn_and_after_socket_parent_finishes() {
        use crate::{deploy_drain::WorkKind, nip_fi_core::test_support as fi};
        let base = crate::state::tests::test_state_with_unreachable_database_url(
            "postgres://test:test@127.0.0.1:1/test",
        )
        .await;
        let mut state = (*base).clone();
        let config = Arc::make_mut(&mut state.config);
        config.deploy_control_token =
            Some(crate::config::DeployControlToken::from_secret("a".repeat(64)).unwrap());
        config.relay_private_key = Some(state.relay_keypair.secret_key().to_secret_hex());
        config.nip_fi.mode = buzz_auth::NipFiMode::Enforce;
        config.nip_fi.profile_policies = vec![ProfilePolicy {
            issuer: fi::TEST_ISSUER.into(),
            label: Some("Test issuer".into()),
            refresh_seconds: 3600,
        }];
        state.deploy_drain = crate::deploy_drain::DrainController::new();
        let state = Arc::new(state);
        let tenant = TenantContext::resolved(
            buzz_core::CommunityId::from_uuid(uuid::Uuid::new_v4()),
            fi::TEST_HOST,
        );
        let assertion = real_profile_assertion();
        let (reached, release) =
            crate::nip_fi_test_hooks::deploy_post_commit_hook::arm(tenant.community());
        let drain = state.deploy_drain.clone();
        let parent = drain.try_admit_root(WorkKind::WebSocket).unwrap();
        parent
            .scope(async {
                admitted(&state, &tenant, Some(&assertion));
            })
            .await;
        assert_eq!(
            drain.snapshot().background_inflight,
            1,
            "registered before the task can run"
        );
        tokio::time::timeout(std::time::Duration::from_secs(2), reached)
            .await
            .unwrap()
            .unwrap();
        let operation = uuid::Uuid::new_v4();
        let boot = drain.begin(operation).unwrap().boot_id;
        drain
            .fence_http(operation, boot, "test-edge-retired")
            .unwrap();
        parent.complete();
        assert!(!drain.snapshot().safe_to_stop);
        release.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while drain.snapshot().background_inflight != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(drain.snapshot().safe_to_stop);
    }
}
