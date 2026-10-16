//! Author-signed managed report recovery over the Nostr event bridge.
use nostr::Event;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Prefix of a signed canonical report in a standard Nostr OK message.
pub const RECEIPT_PREFIX: &str = "managed-delivery:";
/// Maximum serialized recovery payload size.
pub const MAX_RECOVERY_BYTES: usize = 512 * 1024;

/// Saved original evidence and a newly timestamped copy, both author-signed.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryRequest {
    /// The first journaled report, whose body and route are immutable.
    pub original: Event,
    /// A current signed report that may be inserted only if no report won yet.
    pub candidate: ReportStamp,
}

/// Signed fields that change during recovery; body and route remain in the original.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportStamp {
    /// Signed creation timestamp.
    pub created_at: u64,
    /// Canonical event ID.
    pub id: String,
    /// Author Schnorr signature.
    pub sig: String,
}

impl From<&Event> for ReportStamp {
    fn from(event: &Event) -> Self {
        Self {
            created_at: event.created_at.as_secs(),
            id: event.id.to_hex(),
            sig: hex::encode(event.sig.serialize()),
        }
    }
}

impl ReportStamp {
    /// Reconstruct and verify the signed event against its immutable original fields.
    pub fn restore(&self, original: &Event) -> Result<Event, String> {
        let mut value = serde_json::to_value(original).map_err(|e| e.to_string())?;
        value["created_at"] = self.created_at.into();
        value["id"] = self.id.clone().into();
        value["sig"] = self.sig.clone().into();
        let event: Event = serde_json::from_value(value).map_err(|e| e.to_string())?;
        event.verify().map_err(|e| e.to_string())?;
        Ok(event)
    }
}

/// Extract a single canonical delivery UUID from a kind-9 report.
pub fn delivery_key(event: &Event) -> Option<Uuid> {
    if event.kind.as_u16() != 9 {
        return None;
    }
    let mut tags = event
        .tags
        .iter()
        .filter(|t| t.as_slice()[0] == "buzz-delivery");
    let tag = tags.next()?.as_slice();
    if tags.next().is_some() || tag.len() != 2 {
        return None;
    }
    let key = Uuid::parse_str(&tag[1]).ok()?;
    (key.to_string() == tag[1]).then_some(key)
}

/// Compare immutable signed report fields; only time, ID and signature may differ.
pub fn same_report(first: &Event, other: &Event) -> bool {
    first.kind.as_u16() == 9
        && first.kind == other.kind
        && first.pubkey == other.pubkey
        && first.tags == other.tags
        && first.content == other.content
        && delivery_key(first).is_some()
}

impl RecoveryRequest {
    /// Verify the wrapper and both reports before passing the candidate to normal ingest.
    pub fn decode(wrapper: &Event, now: u64) -> Result<Self, String> {
        if wrapper.kind.as_u16() != crate::kind::KIND_MANAGED_DELIVERY_RECOVERY as u16
            || wrapper.content.len() > MAX_RECOVERY_BYTES
            || wrapper.created_at.as_secs().abs_diff(now) > 900
        {
            return Err("invalid recovery envelope or timestamp".into());
        }
        wrapper.verify().map_err(|e| e.to_string())?;
        let request: Self = serde_json::from_str(&wrapper.content).map_err(|e| e.to_string())?;
        request.original.verify().map_err(|e| e.to_string())?;
        let candidate = request.candidate.restore(&request.original)?;
        if !same_report(&request.original, &candidate)
            || wrapper.pubkey != request.original.pubkey
            || request.candidate.created_at.abs_diff(now) > 900
            || request.original.created_at.as_secs().saturating_sub(now) > 900
        {
            return Err(
                "recovery must preserve the verified original report and current candidate".into(),
            );
        }
        let channels = |event: &Event| {
            event
                .tags
                .iter()
                .filter(|t| t.as_slice()[0] == "h")
                .map(|t| t.as_slice().to_vec())
                .collect::<Vec<_>>()
        };
        let original_channels = channels(&request.original);
        if original_channels.len() != 1
            || original_channels[0].len() != 2
            || Uuid::parse_str(&original_channels[0][1]).is_err()
            || channels(wrapper) != original_channels
        {
            return Err("recovery wrapper must name the original channel".into());
        }
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag, Timestamp};

    #[test]
    fn recovery_checks_signed_evidence_and_immutable_payload() {
        let keys = Keys::generate();
        let now = Timestamp::now().as_secs();
        let channel = Uuid::new_v4().to_string();
        let tags = vec![
            Tag::parse(["h", &channel]).unwrap(),
            Tag::parse(["buzz-delivery", &Uuid::new_v4().to_string()]).unwrap(),
        ];
        let make = |content: &str, timestamp: u64| {
            EventBuilder::new(Kind::Custom(9), content)
                .tags(tags.clone())
                .custom_created_at(Timestamp::from(timestamp))
                .sign_with_keys(&keys)
                .unwrap()
        };
        let original = make("first", now - 3600);
        let wrapper = |original: Event, candidate: Event, timestamp: u64| {
            EventBuilder::new(
                Kind::Custom(crate::kind::KIND_MANAGED_DELIVERY_RECOVERY as u16),
                serde_json::to_string(&RecoveryRequest {
                    original,
                    candidate: ReportStamp::from(&candidate),
                })
                .unwrap(),
            )
            .tag(Tag::parse(["h", &channel]).unwrap())
            .custom_created_at(Timestamp::from(timestamp))
            .sign_with_keys(&keys)
            .unwrap()
        };
        assert!(
            RecoveryRequest::decode(&wrapper(original.clone(), make("first", now), now), now)
                .is_ok()
        );
        assert!(RecoveryRequest::decode(
            &wrapper(original.clone(), make("changed", now), now),
            now
        )
        .is_err());
        assert!(RecoveryRequest::decode(
            &wrapper(original.clone(), make("first", now - 901), now),
            now
        )
        .is_err());
        assert!(RecoveryRequest::decode(
            &wrapper(original.clone(), make("first", now + 901), now),
            now
        )
        .is_err());
        assert!(RecoveryRequest::decode(
            &wrapper(original.clone(), make("first", now), now + 901),
            now
        )
        .is_err());
        let mut forged = original.clone();
        forged.content.push('!');
        assert!(RecoveryRequest::decode(&wrapper(forged, make("first", now), now), now).is_err());
        let other = Keys::generate();
        let other_event = EventBuilder::new(original.kind, &original.content)
            .tags(original.tags.clone())
            .sign_with_keys(&other)
            .unwrap();
        assert!(RecoveryRequest::decode(&wrapper(original, other_event, now), now).is_err());
    }
}
