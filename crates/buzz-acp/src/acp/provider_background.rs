//! Negotiated provider task telemetry, including frames received between prompts.
use super::*;
use crate::provider_background::{Record, SNAPSHOT, UPDATE, VERSION};
use crate::session_store::SessionStore;
use futures_util::FutureExt;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct ProviderBackground {
    pub enabled: bool,
    binding: Option<Binding>,
    error: Option<&'static str>,
}
struct Binding {
    record: Record,
    store: Arc<SessionStore>,
    lease: uuid::Uuid,
}

impl AcpClient {
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
            || self.provider_background_record().is_some_and(|record| {
                record.terminal_reason.is_none() && !record.pending().is_empty()
            })
    }
    pub(crate) fn bind_provider_background(
        &mut self,
        record: Record,
        store: Arc<SessionStore>,
        lease: uuid::Uuid,
    ) -> Result<(), AcpError> {
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
        let Some(binding) = &self.provider_background.binding else {
            return Ok(());
        };
        let record = binding.record.clone();
        binding
            .store
            .commit_background(&record.scope, binding.lease, record.clone())
            .map_err(|_| AcpError::Protocol("background_registry_not_persisted".into()))
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
                self.provider_background.error = Some("background_idle_request_requires_prompt");
                break;
            }
        }
    }
}
