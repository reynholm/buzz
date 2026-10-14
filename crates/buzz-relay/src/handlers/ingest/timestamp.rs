//! Timestamp admission for ordinary events and signed managed-report recovery.
use super::{IngestAuth, IngestError};
use buzz_core::kind::KIND_STREAM_MESSAGE;
use nostr::Event;

fn managed_report(event: &Event, auth: &IngestAuth) -> bool {
    if event.kind.as_u16() != KIND_STREAM_MESSAGE as u16 || event.pubkey != *auth.pubkey() {
        return false;
    }
    let mut markers = event
        .tags
        .iter()
        .map(|tag| tag.as_slice())
        .filter(|tag| tag.first().is_some_and(|name| name == "buzz-delivery"));
    let Some(marker) = markers.next() else {
        return false;
    };
    marker.len() == 2
        && markers.next().is_none()
        && uuid::Uuid::parse_str(&marker[1]).is_ok_and(|key| key.to_string() == marker[1])
}

pub(super) fn validate(event: &Event, auth: &IngestAuth, now: u64) -> Result<(), IngestError> {
    const MAX_DRIFT: u64 = 900;
    let timestamp = event.created_at.as_secs();
    // Old marked reports can acknowledge prior acceptance. New old rows still
    // fail the commit-time DB floor; full recovery elects a current candidate
    // through the managed recovery command. Current authorization remains required.
    if timestamp.saturating_sub(now) > MAX_DRIFT
        || (now.saturating_sub(timestamp) > MAX_DRIFT && !managed_report(event, auth))
    {
        return Err(IngestError::Rejected(
            "invalid: event timestamp too far from server time".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use buzz_auth::Scope;
    use nostr::{EventBuilder, Keys, Kind, Tag, Timestamp};

    #[test]
    fn managed_timestamp_exception_preserves_future_and_marker_authority_boundaries() {
        let signer = Keys::generate();
        let other = Keys::generate();
        let key = "9937de50-bed6-41c7-8d7b-ee7b9db9e6ba";
        let canonical = vec![vec!["buzz-delivery".to_string(), key.to_string()]];
        let malformed = [
            vec![],
            vec![vec!["buzz-delivery".into(), "invalid".into()]],
            vec![vec!["buzz-delivery".into(), key.to_uppercase()]],
            vec![vec!["buzz-delivery".into(), key.into(), "extra".into()]],
            vec![
                vec!["buzz-delivery".into(), key.into()],
                vec!["buzz-delivery".into(), uuid::Uuid::new_v4().to_string()],
            ],
        ];
        let event = |timestamp, kind, tags: Vec<Vec<String>>| {
            EventBuilder::new(Kind::Custom(kind), "report")
                .custom_created_at(Timestamp::from(timestamp))
                .tags(tags.into_iter().map(|tag| Tag::parse(tag).unwrap()))
                .sign_with_keys(&signer)
                .unwrap()
        };
        let auth = |keys: &Keys| IngestAuth::Http {
            pubkey: keys.public_key(),
            scopes: vec![Scope::MessagesWrite],
            auth_method: super::super::HttpAuthMethod::Nip98,
        };
        let now = 2_000_000;
        for (timestamp, tags, allowed) in [
            (now - 900, vec![], true),
            (now - 901, vec![], false),
            (now - 3600, canonical.clone(), true),
            (0, canonical.clone(), true),
            (now + 900, canonical.clone(), true),
            (now + 901, canonical.clone(), false),
            (u64::MAX, canonical.clone(), false),
        ] {
            assert_eq!(
                validate(&event(timestamp, 9, tags), &auth(&signer), now).is_ok(),
                allowed
            );
        }
        for tags in malformed {
            assert!(validate(&event(now - 3600, 9, tags), &auth(&signer), now).is_err());
        }
        assert!(validate(
            &event(now - 3600, 1, canonical.clone()),
            &auth(&signer),
            now
        )
        .is_err());
        assert!(validate(&event(now - 3600, 9, canonical), &auth(&other), now).is_err());
    }
}
