//! Negotiated provider task telemetry, including frames received between prompts.
use super::*;
use crate::provider_background::{Record, SNAPSHOT, UPDATE, VERSION};
use crate::session_store::SessionStore;
use futures_util::FutureExt;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct ProviderBackground {
    pub authorization_enabled: bool,
    authorization_owner: Option<String>,
    authorization_coverage: bool,
    authorization_synthetic: bool,
    authorization_checks: std::collections::VecDeque<serde_json::Value>,
    permission_binding: Option<PermissionBinding>,
    pub enabled: bool,
    binding: Option<Binding>,
    error: Option<&'static str>,
}
struct PermissionBinding {
    identity: crate::provider_background::Identity,
    scope: crate::scope::SessionScope,
    route: crate::queue::ThreadTags,
    store: Arc<SessionStore>,
    lease: uuid::Uuid,
}
struct Binding {
    record: Record,
    store: Arc<SessionStore>,
    lease: uuid::Uuid,
}

impl AcpClient {
    async fn flush_authorization_checks(&mut self) -> Result<(), AcpError> {
        while let Some(request) = self.provider_background.authorization_checks.pop_front() {
            self.handle_authorization_check(&request).await?;
        }
        Ok(())
    }
    pub(crate) fn authorization_supported(&self) -> bool {
        self.provider_background.authorization_enabled
    }
    pub(crate) fn install_authorization_wake(&mut self, wake: Arc<tokio::sync::Notify>) {
        self.authorization_wake = Some(wake);
    }
    pub(super) fn authorization_synthetic(&self) -> bool {
        self.provider_background.authorization_synthetic
    }
    pub(super) fn authorization_revision(&self, session: &str) -> Option<u64> {
        self.authorization_prompt_metadata(session)
            .and_then(|value| value["revision"].as_u64())
    }
    pub(super) fn fence_stale_parent(
        &mut self,
        session: &str,
        revision: Option<u64>,
    ) -> Result<bool, AcpError> {
        let Some(revision) = revision else {
            return Ok(false);
        };
        self.reload_authorization()?;
        if self.authorization_revision(session) == Some(revision) {
            return Ok(false);
        }
        if let Some(binding) = &mut self.provider_background.binding {
            binding.record.parent_pending = true;
        }
        self.persist_provider_background()?;
        Ok(true)
    }
    pub(super) async fn handle_authorization_check(
        &mut self,
        msg: &serde_json::Value,
    ) -> Result<(), AcpError> {
        let id = &msg["id"];
        let request = serde_json::from_value::<crate::authorization_context::Admission>(
            msg["params"].clone(),
        );
        let response = match request {
            Ok(request)
                if self.provider_background.authorization_enabled
                    && request.valid()
                    && msg["params"]["identity"] == serde_json::to_value(&request.identity)? =>
            {
                let result = self.provider_background.binding.as_ref().and_then(|binding| {
                    binding.store.check_authorization_admission(&binding.record.scope, binding.lease, &request).ok()
                }).unwrap_or_else(|| serde_json::json!({"identity":request.identity,"revision":request.revision,
                    "check_id":request.check_id,"call_id":request.call_id,"fingerprint":request.fingerprint,
                    "status":"stale","durable":false}));
                self.reload_authorization()?;
                serde_json::json!({"jsonrpc":"2.0","id":id,"result":result})
            }
            _ => {
                serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"authorization admission unavailable or invalid"}})
            }
        };
        self.write_ndjson(&response).await
    }
    pub(crate) fn configure_authorization_policy(
        &mut self,
        owner: Option<String>,
        complete: bool,
        synthetic: bool,
    ) {
        self.provider_background.authorization_owner = owner;
        self.provider_background.authorization_coverage = complete;
        self.provider_background.authorization_synthetic = synthetic;
    }

    pub(super) fn authorization_prompt_metadata(&self, session: &str) -> Option<serde_json::Value> {
        let record = self
            .provider_background_record()
            .filter(|record| record.identity.session_id == session)?;
        let journal = record.authorization.as_ref()?;
        self.provider_background
            .authorization_enabled
            .then(|| journal.snapshot(record))
    }

    pub(crate) async fn reconcile_authorization(&mut self) -> Result<(), AcpError> {
        if !self.provider_background.authorization_enabled {
            return Ok(());
        }
        self.reload_authorization()?;
        let Some(record) = self.provider_background_record() else {
            return Ok(());
        };
        let Some(journal) = record.authorization.as_ref() else {
            return Ok(());
        };
        let snapshot = journal.snapshot(record);
        let budget = record.remaining().min(std::time::Duration::from_secs(2));
        let result = tokio::time::timeout(
            budget,
            self.send_request(crate::authorization_context::UPDATE, snapshot.clone()),
        )
        .await
        .map_err(|_| AcpError::Timeout(budget))??;
        if result["version"] != crate::authorization_context::VERSION
            || result["identity"] != snapshot["identity"]
            || result["update_id"] != snapshot["update_id"]
            || result["revision"] != snapshot["revision"]
            || result["coverage"]["through_sequence"] != snapshot["coverage"]["through_sequence"]
            || result["state"] != snapshot["state"]
            || result["durable"] != true
        {
            return Err(AcpError::Protocol(
                "authorization_receipt_unconfirmed".into(),
            ));
        }
        if let Some(binding) = &mut self.provider_background.binding {
            if let Some(journal) = &mut binding.record.authorization {
                journal.receipt_revision = journal.revision;
            }
        }
        self.persist_provider_background()
    }
    pub(crate) fn bind_permission_context(
        &mut self,
        identity: crate::provider_background::Identity,
        scope: crate::scope::SessionScope,
        route: crate::queue::ThreadTags,
        store: Arc<SessionStore>,
        lease: uuid::Uuid,
    ) {
        self.provider_background.permission_binding = Some(PermissionBinding {
            identity,
            scope,
            route,
            store,
            lease,
        });
    }
    pub(crate) fn record_provider_needs_user(
        &mut self,
        session: &str,
        required: crate::authorization_context::NeedsUser,
    ) -> Result<(), AcpError> {
        let receipt = self
            .provider_background
            .permission_binding
            .as_ref()
            .filter(|binding| binding.identity.session_id == session)
            .map(|binding| {
                (
                    binding.store.clone(),
                    binding.lease,
                    crate::authorization_context::NeedsUserReceipt {
                        identity: binding.identity.clone(),
                        scope: binding.scope.clone(),
                        route: binding.route.clone(),
                        request: required.clone(),
                    },
                )
            })
            .or_else(|| {
                self.provider_background
                    .binding
                    .as_ref()
                    .filter(|binding| binding.record.identity.session_id == session)
                    .map(|binding| {
                        (
                            binding.store.clone(),
                            binding.lease,
                            crate::authorization_context::NeedsUserReceipt {
                                identity: binding.record.identity.clone(),
                                scope: binding.record.scope.clone(),
                                route: binding.record.thread_tags.clone(),
                                request: required.clone(),
                            },
                        )
                    })
            });
        if let Some((store, lease, receipt)) = receipt {
            store
                .commit_needs_user(receipt, lease)
                .map_err(|_| AcpError::Protocol("needs_user_not_persisted".into()))?;
        }
        if let Some(binding) = self
            .provider_background
            .binding
            .as_mut()
            .filter(|binding| binding.record.identity.session_id == session)
        {
            binding.record.needs_user = Some(required);
            binding.record.terminal_reason = Some("needs_user".into());
        }
        self.persist_provider_background()
    }
    pub(super) fn consume_authorization_needs_user(&mut self, message: &serde_json::Value) {
        if !self.provider_background.authorization_enabled
            || message["method"] != "_buzz/authorization/needs_user"
        {
            return;
        }
        let params = &message["params"];
        let Some(record) = self.provider_background_record() else {
            return;
        };
        let session = record.identity.session_id.clone();
        let valid = params["identity"]
            == serde_json::to_value(&record.identity).unwrap_or_default()
            && params["execution_state"] == "not_started"
            && params
                .get("sessionId")
                .is_none_or(|id| id.as_str() == Some(session.as_str()))
            && ["call_id", "fingerprint", "reason"].iter().all(|key| {
                params[*key]
                    .as_str()
                    .is_some_and(|text| !text.is_empty() && text.len() <= 512)
            });
        if !valid {
            self.provider_background.error = Some("authorization_needs_user_invalid");
            return;
        }
        let required = crate::authorization_context::NeedsUser {
            call_id: params["call_id"].as_str().unwrap_or_default().into(),
            fingerprint: params["fingerprint"].as_str().map(str::to_owned),
            reason: params["reason"].as_str().unwrap_or_default().into(),
            execution_state: "not_started".into(),
        };
        self.needs_user = Some(required.clone());
        if self
            .record_provider_needs_user(&session, required.clone())
            .is_err()
        {
            self.provider_background.error = Some("needs_user_not_persisted");
        }
        self.observe("authorization_needs_user", serde_json::json!({"sessionId":session,"identity":params["identity"],"request":required,"state":"needs_user","approved":false}));
    }
    pub(crate) fn append_provider_inputs(
        &mut self,
        inputs: &[crate::queue::BatchEvent],
    ) -> Result<(), AcpError> {
        if let Some(binding) = &mut self.provider_background.binding {
            let mut next = binding.record.clone();
            for input in inputs {
                if !next
                    .events
                    .iter()
                    .any(|saved| saved.event.id == input.event.id)
                {
                    next.events
                        .push(crate::provider_background::SavedEvent::from(input));
                }
            }
            next.validate()
                .map_err(|reason| AcpError::Protocol(reason.into()))?;
            binding.record = next;
        }
        self.persist_provider_background()
    }
    pub(crate) fn suspend_provider_background(&mut self, reason: &str) -> Result<(), AcpError> {
        if let Some(binding) = &mut self.provider_background.binding {
            binding.record.terminal_reason = Some(reason.into());
        }
        self.persist_provider_background()
    }
    pub(crate) fn abandon_provider_background(&mut self) {
        self.provider_background.binding = None;
        self.provider_background.error = None;
    }
    pub(crate) async fn cancel_provider_background(&mut self, cause: &str) -> Result<(), AcpError> {
        let Some(binding) = &mut self.provider_background.binding else {
            return Ok(());
        };
        let cancel = binding
            .record
            .cancellation
            .get_or_insert_with(|| crate::provider_background::Cancellation {
                identity: binding.record.identity.clone(),
                cancel_id: uuid::Uuid::new_v4(),
                cause: cause.into(),
                acknowledged: false,
            })
            .clone();
        if !cancel.valid() {
            return Err(AcpError::Protocol("background_cancel_cause_invalid".into()));
        }
        self.persist_provider_background()?;
        if cancel.acknowledged {
            return Ok(());
        }
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.send_request("_buzz/background_tasks/cancel", serde_json::json!({"identity":cancel.identity,"cancel_id":cancel.cancel_id,"cause":cancel.cause})),
        ).await.map_err(|_| AcpError::Timeout(std::time::Duration::from_secs(2)))??;
        if result["identity"] != serde_json::to_value(&cancel.identity)?
            || result["cancel_id"] != serde_json::json!(cancel.cancel_id)
            || result["cause"] != cancel.cause
            || result["state"] != "cancelled"
            || result["durable"] != true
        {
            return Err(AcpError::Protocol(
                "background_cancellation_unconfirmed".into(),
            ));
        }
        if let Some(binding) = &mut self.provider_background.binding {
            if let Some(receipt) = &mut binding.record.cancellation {
                receipt.acknowledged = true;
            }
        }
        self.persist_provider_background()
    }
    pub(crate) fn provider_background_supported(&self) -> bool {
        self.provider_background.enabled
    }
    pub(crate) fn provider_background_record(&self) -> Option<&Record> {
        self.provider_background
            .binding
            .as_ref()
            .map(|binding| &binding.record)
    }
    pub(crate) fn provider_background_active(&self) -> bool {
        self.provider_background_record()
            .is_some_and(Record::recoverable)
    }
    pub(crate) fn finish_provider_parent(&mut self, reason: &str) -> Result<(), AcpError> {
        if let Some(binding) = &mut self.provider_background.binding {
            binding.record.parent_pending = false;
            binding.record.terminal_reason = Some(reason.into());
        }
        self.persist_provider_background()
    }
    pub(crate) fn provider_background_outstanding(&self) -> bool {
        self.provider_background_record()
            .is_some_and(Record::outstanding)
    }
    pub(crate) fn provider_background_pending(&self) -> bool {
        self.provider_background.error.is_some()
            || !self.provider_background.authorization_checks.is_empty()
            || self.provider_background_record().is_some_and(|record| {
                record.terminal_reason.is_none()
                    && record.cancellation.is_none()
                    && (!record.pending().is_empty()
                        || record
                            .authorization
                            .as_ref()
                            .is_some_and(|journal| journal.receipt_revision != journal.revision))
            })
    }
    pub(crate) fn bind_provider_background(
        &mut self,
        mut record: Record,
        store: Arc<SessionStore>,
        lease: uuid::Uuid,
    ) -> Result<(), AcpError> {
        if self.provider_background.authorization_enabled && record.authorization.is_none() {
            record.authorization = Some(crate::authorization_context::Journal::new(
                self.provider_background.authorization_owner.clone(),
                self.provider_background.authorization_coverage,
            ));
        }
        if let Some(journal) = &mut record.authorization {
            if journal.owner_pubkey != self.provider_background.authorization_owner {
                journal.owner_pubkey = self.provider_background.authorization_owner.clone();
                journal.initial_coverage = false;
            }
        }
        record
            .refresh_authorization()
            .map_err(|reason| AcpError::Protocol(reason.into()))?;
        record
            .validate()
            .map_err(|reason| AcpError::Protocol(reason.into()))?;
        if self
            .provider_background
            .binding
            .as_ref()
            .is_some_and(|binding| binding.record.identity == record.identity)
        {
            return Ok(());
        }
        self.provider_background.binding = Some(Binding {
            record,
            store,
            lease,
        });
        self.provider_background.error = None;
        self.persist_provider_background()
    }
    fn persist_provider_background(&mut self) -> Result<(), AcpError> {
        let Some(binding) = &mut self.provider_background.binding else {
            return Ok(());
        };
        binding
            .record
            .refresh_authorization()
            .map_err(|reason| AcpError::Protocol(reason.into()))?;
        let record = binding.record.clone();
        binding
            .store
            .commit_background(&record.scope, binding.lease, record.clone())
            .map_err(|_| AcpError::Protocol("background_registry_not_persisted".into()))?;
        self.reload_authorization()
    }

    fn reload_authorization(&mut self) -> Result<(), AcpError> {
        if !self.provider_background.authorization_enabled {
            return Ok(());
        }
        let Some(binding) = &mut self.provider_background.binding else {
            return Ok(());
        };
        let crate::session_store::LoadOutcome::Found(saved) =
            binding.store.load(&binding.record.scope)
        else {
            return Err(AcpError::Protocol(
                "authorization_registry_unavailable".into(),
            ));
        };
        let live = saved
            .live
            .filter(|live| live.lease == binding.lease)
            .ok_or_else(|| AcpError::Protocol("authorization_lease_changed".into()))?;
        let current = live
            .provider_background
            .filter(|current| current.identity == binding.record.identity)
            .ok_or_else(|| AcpError::Protocol("authorization_identity_changed".into()))?;
        binding.record.authorization = current.authorization;
        binding.record.events = current.events;
        Ok(())
    }
    /// Called on every ACP read path, before matching an RPC response.
    pub(super) fn consume_provider_background(&mut self, msg: &serde_json::Value) {
        if !self.provider_background.enabled
            || msg.get("method").and_then(serde_json::Value::as_str) != Some(UPDATE)
        {
            return;
        }
        let Some(binding) = &mut self.provider_background.binding else {
            return;
        };
        match binding.record.apply(&msg["params"]) {
            Ok(false) => {}
            Ok(true) => {
                if self.persist_provider_background().is_err() {
                    self.provider_background.error = Some("background_registry_not_persisted");
                }
            }
            Err(reason) => self.provider_background.error = Some(reason),
        }
    }
    pub(super) fn check_provider_background(&self) -> Result<(), AcpError> {
        self.provider_background
            .error
            .map_or(Ok(()), |reason| Err(AcpError::Protocol(reason.into())))
    }
    pub(crate) async fn reconcile_provider_background(&mut self) -> Result<(), AcpError> {
        self.flush_authorization_checks().await?;
        self.reconcile_authorization().await?;
        if !self.provider_background.enabled {
            return Err(AcpError::Protocol("background_tasks_not_negotiated".into()));
        }
        let Some(binding) = &self.provider_background.binding else {
            return Ok(());
        };
        let params = serde_json::to_value(&binding.record.identity)?;
        let snapshot = self.send_request(SNAPSHOT, params).await?;
        if let Some(binding) = &mut self.provider_background.binding {
            binding
                .record
                .reconcile(&snapshot)
                .map_err(|reason| AcpError::Protocol(reason.into()))?;
        }
        self.persist_provider_background()
    }
    pub(crate) fn start_provider_collection(&mut self) -> Result<(), AcpError> {
        if let Some(binding) = &mut self.provider_background.binding {
            binding.record.start_collection();
        }
        self.persist_provider_background()
    }
    pub(super) fn provider_prompt_metadata(&self, session: &str) -> Option<serde_json::Value> {
        let binding = self
            .provider_background
            .binding
            .as_ref()
            .filter(|binding| binding.record.identity.session_id == session)?;
        Some(
            serde_json::json!({"version":VERSION,"identity":binding.record.identity,
            "collect":binding.record.pending(),"continuation_started":binding.record.started,
            "deadline_unix":binding.record.deadline_unix}),
        )
    }
    /// Bounded nonblocking drain; FramedRead retains partial frames on cancellation.
    pub(crate) fn poll_provider_background(&mut self) {
        if !self.provider_background.enabled || self.provider_background.error.is_some() {
            return;
        }
        if self.reload_authorization().is_err() {
            self.provider_background.error = Some("authorization_registry_unavailable");
            return;
        }
        for _ in 0..128 {
            let Some(line) = self.reader.next().now_or_never() else {
                break;
            };
            let Some(Ok(line)) = line else {
                self.provider_background.error = Some("background_idle_transport_failed");
                break;
            };
            let Ok(msg) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
                self.provider_background.error = Some("background_idle_frame_invalid");
                break;
            };
            self.observe_incoming(&msg, &line);
            if self.provider_background.error.is_some() {
                break;
            }
            if msg.get("method").and_then(serde_json::Value::as_str) == Some("session/update") {
                self.handle_session_update(&msg);
            }
            if msg.get("method").is_some() && msg.get("id").is_some() {
                if self.provider_background.authorization_enabled
                    && msg["method"] == "_buzz/authorization/check"
                    && self.provider_background.authorization_checks.len() < 128
                {
                    self.provider_background.authorization_checks.push_back(msg);
                    continue;
                }
                self.provider_background.error = Some("background_idle_request_requires_prompt");
                break;
            }
        }
    }
}
