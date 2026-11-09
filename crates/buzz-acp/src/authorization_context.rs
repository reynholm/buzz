//! Structured waiting state is evidence of a blocked action, never a grant.
use crate::provider_background::Record;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(crate) const VERSION: u64 = 1;
pub(crate) const UPDATE: &str = "_buzz/authorization/update";

/// Admission precedes queueing, including the window before the first ACP bind.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ingress {
    pub sequence: u64,
    pub pending: Vec<crate::provider_background::SavedEvent>,
    pub seen_ids: Vec<String>,
    pub incomplete: bool,
}

/// Correlation and source coverage, deliberately independent of tool verdicts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Journal {
    pub version: u64,
    pub owner_pubkey: Option<String>,
    pub revision: u64,
    pub update_id: Uuid,
    pub receipt_revision: u64,
    pub initial_coverage: bool,
    #[serde(default)]
    pub ingress_sequence: u64,
    #[serde(default)]
    pub fenced_reason: Option<String>,
    #[serde(default)]
    pub control: Option<Control>,
    #[serde(default)]
    pub admissions: std::collections::BTreeMap<Uuid, Admission>,
    payload_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Control {
    pub cause: String,
    pub source_id: Option<String>,
}

/// Old identities stay revoked even after their provider session is removed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Revocation {
    pub identity: crate::provider_background::Identity,
    pub revision: u64,
    pub update_id: Uuid,
    pub cause: String,
    pub source_id: Option<String>,
    pub source_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Admission {
    pub identity: crate::provider_background::Identity,
    pub revision: u64,
    pub check_id: Uuid,
    pub call_id: String,
    pub fingerprint: String,
}
impl Admission {
    pub fn valid(&self) -> bool {
        !self.check_id.is_nil()
            && self.revision > 0
            && !self.call_id.is_empty()
            && self.call_id.len() <= 512
            && !self.fingerprint.is_empty()
            && self.fingerprint.len() <= 512
    }
}

impl Journal {
    pub fn valid(&self, record: &Record) -> bool {
        self.version == VERSION
            && self.revision > 0
            && !self.update_id.is_nil()
            && self.receipt_revision <= self.revision
            && self.payload_hash.len() == 64
            && self
                .owner_pubkey
                .as_ref()
                .is_none_or(|key| nostr::PublicKey::from_hex(key).is_ok())
            && self
                .fenced_reason
                .as_ref()
                .is_none_or(|reason| !reason.is_empty() && reason.len() <= 512)
            && self.control.as_ref().is_none_or(|control| {
                !control.cause.is_empty()
                    && control.cause.len() <= 512
                    && control
                        .source_id
                        .as_ref()
                        .is_none_or(|id| nostr::EventId::from_hex(id).is_ok())
            })
            && self.admissions.len() <= 128
            && self.admissions.iter().all(|(id, admission)| {
                *id == admission.check_id
                    && admission.valid()
                    && admission.identity == record.identity
                    && admission.revision <= self.revision
            })
    }
    pub fn matches_payload(&self, record: &Record) -> bool {
        serde_json::to_vec(&self.payload(record))
            .is_ok_and(|bytes| hex::encode(Sha256::digest(bytes)) == self.payload_hash)
    }
    pub fn new(owner_pubkey: Option<String>, initial_coverage: bool) -> Self {
        Self {
            version: VERSION,
            owner_pubkey,
            revision: 0,
            update_id: Uuid::nil(),
            receipt_revision: 0,
            initial_coverage,
            ingress_sequence: 0,
            fenced_reason: None,
            control: None,
            admissions: Default::default(),
            payload_hash: String::new(),
        }
    }
    pub fn refresh(&mut self, record: &Record) -> Result<(), &'static str> {
        if self.version != VERSION {
            return Err("authorization_version_unknown");
        }
        let bytes =
            serde_json::to_vec(&self.payload(record)).map_err(|_| "authorization_encoding")?;
        let digest = hex::encode(Sha256::digest(bytes));
        if digest != self.payload_hash {
            self.revision = self
                .revision
                .checked_add(1)
                .ok_or("authorization_revision_exhausted")?;
            self.update_id = Uuid::new_v4();
            self.payload_hash = digest;
        }
        Ok(())
    }
    pub fn snapshot(&self, record: &Record) -> Value {
        let mut value = self.payload(record);
        value["version"] = serde_json::json!(VERSION);
        value["identity"] = serde_json::json!(record.identity);
        value["update_id"] = serde_json::json!(self.update_id);
        value["revision"] = serde_json::json!(self.revision);
        value
    }
    fn payload(&self, record: &Record) -> Value {
        let channel = record.scope.channel_id().to_string();
        let sources: Vec<_> = record.events.iter().enumerate().map(|(index, saved)| {
            let event = &saved.event;
            let tags: Vec<_> = event.tags.iter().map(|tag| tag.as_slice()).collect();
            let actor = if tags.iter().any(|tag| tag.first().map(String::as_str)==Some("auth")) {
                "agent"
            } else if self.owner_pubkey.as_deref() == Some(event.pubkey.to_hex().as_str()) {
                "owner_authority"
            } else { "unknown" };
            let channel_matches = tags.iter().filter(|tag| tag.first().map(String::as_str)==Some("h"))
                .all(|tag| tag.get(1)==Some(&channel)) && tags.iter().any(|tag| tag.get(1)==Some(&channel) && tag.first().map(String::as_str)==Some("h"));
            let complete = event.verify().is_ok() && channel_matches
                && !tags.iter().any(|tag| tag.first().map(String::as_str)==Some("imeta"))
                // Structured/edit payloads keep raw evidence but cannot inherit
                // authority until their attachment coverage is established.
                && event.kind.as_u16()==9
                && source_matches_route(record, saved)
                && (event.kind.as_u16()!=40003 || saved.edit.is_some());
            let mentions: Vec<_> = tags.iter().filter(|tag| tag.first().map(String::as_str)==Some("p"))
                .filter_map(|tag| tag.get(1)).filter_map(|key| nostr::PublicKey::from_hex(key).ok())
                .map(|key| key.to_hex()).collect();
            serde_json::json!({"sequence":index+1,"source_id":event.id.to_hex(),"actor_pubkey":event.pubkey.to_hex(),
                "actor_kind":actor,"kind":event.kind.as_u16(),"raw_text":event.content,"signed_event":event,
                "original_id":saved.edit.as_ref().map(|edit| &edit.target_event_id),"mention_pubkeys":mentions,"complete":complete})
        }).collect();
        let complete = self.initial_coverage
            && self.owner_pubkey.is_some()
            && self.fenced_reason.is_none()
            && record.events.len() < 50
            && !sources.is_empty()
            && sources.iter().all(|source| source["complete"] == true);
        let state = if record.needs_user.is_some() {
            "needs_user"
        } else if self.control.is_some()
            || record.cancellation.is_some()
            || record.terminal_reason.is_some()
        {
            "revoked"
        } else if complete {
            "active"
        } else {
            "unknown"
        };
        serde_json::json!({"state":state,"owner_pubkey":self.owner_pubkey,
            "route":{"channel_id":channel,"reply_to":record.thread_tags.parent_event_id,"thread_root":record.thread_tags.root_event_id},
            "coverage":{"complete":complete,"through_sequence":sources.len(),"reason":self.fenced_reason},"sources":sources,
            "control":{"reason":self.control.as_ref().map(|control| &control.cause).or(record.terminal_reason.as_ref()),
                "source_id":self.control.as_ref().and_then(|control|control.source_id.as_ref()),"cancellation":record.cancellation}})
    }
}

fn source_matches_route(record: &Record, source: &crate::provider_background::SavedEvent) -> bool {
    let tags = crate::queue::routing_thread_tags(&source.event, source.edit.as_ref());
    let actual = tags.root_event_id.or(tags.parent_event_id);
    let expected = record
        .thread_tags
        .root_event_id
        .as_ref()
        .or(record.thread_tags.parent_event_id.as_ref());
    match (actual.as_ref(), expected) {
        (Some(actual), Some(expected)) => actual == expected,
        (None, Some(expected)) => {
            source.event.id.to_hex() == *expected
                || source
                    .edit
                    .as_ref()
                    .is_some_and(|edit| edit.target_event_id == *expected)
        }
        (None, None) => true,
        _ => false,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NeedsUser {
    pub call_id: String,
    pub fingerprint: Option<String>,
    pub reason: String,
    pub execution_state: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NeedsUserReceipt {
    pub identity: crate::provider_background::Identity,
    pub scope: crate::scope::SessionScope,
    pub route: crate::queue::ThreadTags,
    pub request: NeedsUser,
}

impl NeedsUser {
    pub fn from_permission(message: &Value) -> Option<Self> {
        let params = &message["params"];
        let marker = params.pointer("/_meta/buzz/authorization/requiresHuman");
        let headless = params.get("approvalRequired");
        // A malformed explicit human marker cannot fall through to allow_once.
        if ![marker, headless]
            .into_iter()
            .flatten()
            .any(|value| value != &Value::Bool(false))
        {
            return None;
        }
        let call_id = params["toolCall"]["toolCallId"]
            .as_str()
            .filter(|value| !value.is_empty() && value.len() <= 512)
            .map(str::to_owned)
            .unwrap_or_else(|| "unavailable".into());
        let fingerprint = params
            .pointer("/_meta/buzz/authorization/fingerprint")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty() && value.len() <= 512)
            .map(str::to_owned);
        Some(Self {
            call_id,
            fingerprint,
            reason: "human_approval_required".into(),
            execution_state: "not_started".into(),
        })
    }
}
