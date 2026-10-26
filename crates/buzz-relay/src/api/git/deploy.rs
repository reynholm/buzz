//! Per-push ownership for authenticated callbacks after the HTTP root fence.
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};

use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
    response::{IntoResponse, Response},
};
use buzz_core::CommunityId;
use uuid::Uuid;

use crate::deploy_drain::{DrainController, DrainError, ParentContext, WorkGuard, WorkKind};

pub(crate) async fn admit_callback_request(
    state: &crate::state::AppState,
    request: Request<Body>,
) -> Result<(Request<Body>, WorkGuard), Response> {
    if request.method() != Method::POST
        || request.uri().path() != "/internal/git/policy"
        || request.uri().query().is_some()
        || !super::is_loopback(&request)
    {
        return Err((StatusCode::FORBIDDEN, "invalid callback transport").into_response());
    }
    let (parts, body) = request.into_parts();
    let bytes = to_bytes(body, 1024 * 1024)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "invalid callback body").into_response())?;
    let payload: super::policy::HookCallbackRequest = serde_json::from_slice(&bytes)
        .map_err(|_| (StatusCode::FORBIDDEN, "invalid callback body").into_response())?;
    let community = super::policy::authenticate_callback(state, &payload)?;
    let operation = payload
        .operation_id
        .ok_or_else(|| (StatusCode::FORBIDDEN, "callback parent unavailable").into_response())?;
    let identity = PushIdentity {
        repo: payload.repo_id,
        owner: payload.repo_owner,
        community,
        pusher: payload.pusher_pubkey,
    };
    let work = state
        .git_pushes
        .admit_callback(operation, &identity)
        .map_err(|_| (StatusCode::FORBIDDEN, "callback parent unavailable").into_response())?;
    Ok((Request::from_parts(parts, Body::from(bytes)), work))
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PushIdentity {
    pub repo: String,
    pub owner: String,
    pub community: CommunityId,
    pub pusher: String,
}

struct AcceptedPush {
    identity: PushIdentity,
    parent: ParentContext,
}

struct RegistryInner {
    drain: DrainController,
    pushes: Mutex<HashMap<Uuid, AcceptedPush>>,
}

#[derive(Clone)]
pub(crate) struct GitPushRegistry(Arc<RegistryInner>);

impl GitPushRegistry {
    pub(crate) fn new(drain: DrainController) -> Self {
        Self(Arc::new(RegistryInner {
            drain,
            pushes: Mutex::new(HashMap::new()),
        }))
    }

    fn entries(&self) -> Result<MutexGuard<'_, HashMap<Uuid, AcceptedPush>>, DrainError> {
        match self.0.pushes.lock() {
            Ok(entries) => Ok(entries),
            Err(_) => {
                self.0.drain.record_fault("git-push-registry-poisoned");
                Err(DrainError)
            }
        }
    }

    /// Register before spawning receive-pack. The opaque context identifies a
    /// live parent without extending ownership or holding a mutex across await.
    pub(crate) fn register(&self, identity: PushIdentity) -> Result<PushGuard, DrainError> {
        let work = self.0.drain.track_accepted_child(WorkKind::Git);
        let operation = Uuid::new_v4();
        self.entries()?.insert(
            operation,
            AcceptedPush {
                identity,
                parent: work.parent_context(),
            },
        );
        Ok(PushGuard {
            work,
            registration: PushRegistration {
                registry: self.clone(),
                operation,
            },
        })
    }

    /// Called only after loopback, HMAC, timestamp and request-shape validation.
    /// Completion/removal racing this admission is resolved by the controller's
    /// live-parent map; already admitted children may outlive the push.
    pub(crate) fn admit_callback(
        &self,
        operation: Uuid,
        identity: &PushIdentity,
    ) -> Result<WorkGuard, DrainError> {
        let entries = self.entries()?;
        let push = entries.get(&operation).ok_or(DrainError)?;
        if push.identity != *identity {
            return Err(DrainError);
        }
        self.0
            .drain
            .admit_parent_context(&push.parent, WorkKind::Callback)
    }
}

struct PushRegistration {
    registry: GitPushRegistry,
    operation: Uuid,
}

impl Drop for PushRegistration {
    fn drop(&mut self) {
        if let Ok(mut entries) = self.registry.entries() {
            entries.remove(&self.operation);
        }
    }
}

pub(crate) struct PushGuard {
    work: WorkGuard,
    registration: PushRegistration,
}

impl PushGuard {
    pub(crate) fn operation_id(&self) -> Uuid {
        self.registration.operation
    }

    pub(crate) async fn scope<F: Future>(&self, future: F) -> F::Output {
        self.work.scope(future).await
    }

    pub(crate) fn complete(self) {
        self.work.complete();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy_drain::{DrainController, WorkKind};
    use buzz_core::CommunityId;
    use uuid::Uuid;

    fn identity() -> PushIdentity {
        PushIdentity {
            repo: "example".to_owned(),
            owner: "a".repeat(64),
            community: CommunityId::from_uuid(Uuid::new_v4()),
            pusher: "b".repeat(64),
        }
    }

    #[tokio::test]
    async fn actual_router_allows_only_authenticated_live_callback_after_http_fence() {
        use crate::api::git::policy::{compute_hmac, HookCallbackRequest, HookRefUpdate};
        use axum::{
            body::Body,
            extract::ConnectInfo,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;

        fn request(payload: &HookCallbackRequest, peer: &str) -> Request<Body> {
            Request::builder()
                .method("POST")
                .uri("/internal/git/policy")
                .header("content-type", "application/json")
                .extension(ConnectInfo(peer.parse::<std::net::SocketAddr>().unwrap()))
                .body(Body::from(serde_json::to_vec(payload).unwrap()))
                .unwrap()
        }

        let mut state = crate::state::tests::test_state_with_database_url(
            "postgres://synthetic:synthetic@127.0.0.1:1/unused",
        )
        .await;
        let config = Arc::get_mut(&mut Arc::get_mut(&mut state).unwrap().config).unwrap();
        config.deploy_control_token =
            Some(crate::config::DeployControlToken::from_secret("a".repeat(64)).unwrap());
        config.git_hook_hmac_secret = "synthetic-hook-secret".to_owned();
        let parent = state.deploy_drain.try_admit_root(WorkKind::Http).unwrap();
        let identity = identity();
        let push = parent
            .scope(async { state.git_pushes.register(identity.clone()).unwrap() })
            .await;
        let mut payload = HookCallbackRequest {
            repo_id: identity.repo.clone(),
            repo_owner: identity.owner.clone(),
            community_id: identity.community.as_uuid().to_string(),
            pusher_pubkey: identity.pusher.clone(),
            operation_id: Some(push.operation_id()),
            ref_updates: vec![HookRefUpdate {
                old_oid: "1".repeat(40),
                new_oid: "2".repeat(40),
                ref_name: "refs/heads/main".to_owned(),
                is_ancestor: true,
            }],
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            signature: String::new(),
        };
        payload.signature = hex::encode(compute_hmac(b"synthetic-hook-secret", &payload));
        let operation = Uuid::new_v4();
        let boot = state.deploy_drain.begin(operation).unwrap().boot_id;
        state
            .deploy_drain
            .fence_http(operation, boot, "qualified-edge")
            .unwrap();
        parent.complete();
        let router = crate::router::build_router(state.clone());
        let response = router
            .clone()
            .oneshot(request(&payload, "127.0.0.1:12345"))
            .await
            .unwrap();
        let status = response.status();
        let held = state.deploy_drain.snapshot();
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let after_body = state.deploy_drain.snapshot();
        let mut rejected = Vec::new();
        for variant in 0..4 {
            let mut invalid = payload.clone();
            let mut peer = "127.0.0.1:12345";
            match variant {
                0 => invalid.signature = "0".repeat(64),
                1 => {
                    invalid.repo_id.push('x');
                    invalid.signature =
                        hex::encode(compute_hmac(b"synthetic-hook-secret", &invalid));
                }
                2 => peer = "198.51.100.1:12345",
                _ => {
                    invalid.operation_id = None;
                    invalid.signature =
                        hex::encode(compute_hmac(b"synthetic-hook-secret", &invalid));
                }
            }
            let response = router
                .clone()
                .oneshot(request(&invalid, peer))
                .await
                .unwrap();
            rejected.push(response.status());
            drop(response);
        }
        push.complete();
        let stale = router
            .oneshot(request(&payload, "127.0.0.1:12345"))
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "authenticated callback must reach the real policy handler"
        );
        assert_eq!(
            body.as_ref(),
            b"internal error",
            "unreachable fixture DB proves handler admission, without slow DB waits"
        );
        assert_eq!(held.http_inflight, 0);
        assert_eq!(held.git_inflight, 1);
        assert_eq!(held.callback_inflight, 1);
        assert_eq!(after_body.callback_inflight, 0);
        assert!(rejected
            .iter()
            .all(|status| *status == StatusCode::FORBIDDEN));
        assert_eq!(stale.status(), StatusCode::FORBIDDEN);
        assert_eq!(state.deploy_drain.snapshot().callback_inflight, 0);
    }

    #[tokio::test]
    async fn callback_after_http_fence_descends_from_live_matching_push() {
        let drain = DrainController::new();
        let registry = GitPushRegistry::new(drain.clone());
        let parent = drain.try_admit_root(WorkKind::Http).unwrap();
        let identity = identity();
        let push = parent
            .scope(async { registry.register(identity.clone()).unwrap() })
            .await;
        let operation = push.operation_id();
        let deployment = Uuid::new_v4();
        let boot = drain.begin(deployment).unwrap().boot_id;
        drain
            .fence_http(deployment, boot, "qualified-edge")
            .unwrap();
        parent.complete();
        let callback = registry.admit_callback(operation, &identity).unwrap();
        assert_eq!(drain.snapshot().git_inflight, 1);
        assert_eq!(drain.snapshot().callback_inflight, 1);
        let child = push
            .scope(async {
                tokio::task::yield_now().await;
                drain.admit_current_child(WorkKind::Background).unwrap()
            })
            .await;
        push.complete();
        assert!(registry.admit_callback(operation, &identity).is_err());
        assert_eq!(drain.snapshot().git_inflight, 0);
        assert!(!drain.snapshot().safe_to_stop);
        callback.complete();
        child.complete();
        assert!(drain.snapshot().safe_to_stop);
    }

    #[tokio::test]
    async fn callback_cannot_change_push_identity_or_reuse_another_boot() {
        let drain = DrainController::new();
        let registry = GitPushRegistry::new(drain.clone());
        let other_boot = GitPushRegistry::new(DrainController::new());
        let parent = drain.try_admit_root(WorkKind::Http).unwrap();
        let identity = identity();
        let push = parent
            .scope(async { registry.register(identity.clone()).unwrap() })
            .await;
        for field in 0..4 {
            let mut wrong = identity.clone();
            match field {
                0 => wrong.repo.push('x'),
                1 => wrong.owner = "c".repeat(64),
                2 => wrong.community = CommunityId::from_uuid(Uuid::new_v4()),
                _ => wrong.pusher = "d".repeat(64),
            }
            assert!(registry
                .admit_callback(push.operation_id(), &wrong)
                .is_err());
        }
        assert!(other_boot
            .admit_callback(push.operation_id(), &identity)
            .is_err());
        assert!(registry.admit_callback(Uuid::new_v4(), &identity).is_err());
        assert_eq!(drain.snapshot().callback_inflight, 0);
        push.complete();
        parent.complete();
    }

    #[tokio::test]
    async fn lost_push_removes_callback_capability_and_keeps_unresolved_fault() {
        let drain = DrainController::new();
        let registry = GitPushRegistry::new(drain.clone());
        let parent = drain.try_admit_root(WorkKind::Http).unwrap();
        let identity = identity();
        let push = parent
            .scope(async { registry.register(identity.clone()).unwrap() })
            .await;
        let operation = push.operation_id();
        drop(push);
        parent.complete();
        assert!(registry.admit_callback(operation, &identity).is_err());
        let deployment = Uuid::new_v4();
        let boot = drain.begin(deployment).unwrap().boot_id;
        drain
            .fence_http(deployment, boot, "qualified-edge")
            .unwrap();
        let snapshot = drain.snapshot();
        assert_eq!(snapshot.git_inflight, 0);
        assert!(!snapshot.safe_to_stop);
        assert!(snapshot
            .blocking_workers
            .iter()
            .any(|fault| fault == "unresolved-git"));
    }
}
