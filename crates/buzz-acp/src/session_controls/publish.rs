//! Signed bounded result delivery, reusable by manual compaction.
use super::Publication;
use buzz_core::session_control::SessionResult;

pub(crate) fn build_result(
    keys: &nostr::Keys,
    publication: &Publication,
) -> anyhow::Result<nostr::Event> {
    let result = SessionResult {
        command_id: publication.command_id.clone(),
        result: publication.result,
    };
    let tag = result
        .event_tag()
        .ok_or_else(|| anyhow::anyhow!("invalid command identity"))?;
    let target = nostr::EventId::from_hex(&publication.command_id)?;
    Ok(buzz_sdk::build_reaction(target, result.result.reaction())?
        .tag(nostr::Tag::parse(tag)?)
        .sign_with_keys(keys)?)
}

/// Publish the same signed event at most three times; publication cannot rerun
/// the operation. The caller retains the durable outcome on delivery failure.
pub(crate) async fn publish_result(
    rest: &crate::relay::RestClient,
    publication: &Publication,
) -> anyhow::Result<()> {
    let event = build_result(&rest.keys, publication)?;
    let mut last = None;
    for delay in [0, 1, 2] {
        if delay != 0 {
            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
        }
        match tokio::time::timeout(
            std::time::Duration::from_secs(3),
            rest.submit_event_once(&event),
        )
        .await
        {
            Ok(Ok(_)) => {
                if matches!(
                    publication.result,
                    buzz_core::session_control::ControlResult::Terminal(_)
                ) {
                    crate::pool::reaction_remove(rest, &publication.command_id, "⏳").await;
                }
                return Ok(());
            }
            Ok(Err(error)) => last = Some(error.to_string()),
            Err(_) => last = Some("result publication timed out".into()),
        }
    }
    Err(anyhow::anyhow!("{}", last.unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use buzz_core::session_control::{ControlFailure, ControlResult};
    #[test]
    fn control_results_are_signed_and_content_matches_canonical_result_tag() {
        let keys = nostr::Keys::generate();
        for result in [
            ControlResult::Pending,
            ControlResult::Terminal(Ok(())),
            ControlResult::Terminal(Err(ControlFailure::Unconfirmed)),
        ] {
            let publication = Publication {
                command_id: "a".repeat(64),
                result,
            };
            let event = build_result(&keys, &publication).unwrap();
            event.verify().unwrap();
            assert_eq!(event.kind.as_u16(), 7);
            let tags = event
                .tags
                .iter()
                .map(|tag| tag.as_slice().to_vec())
                .collect::<Vec<_>>();
            assert_eq!(
                SessionResult::from_tags(&event.content, &tags)
                    .unwrap()
                    .result,
                result
            );
            assert!(tags
                .iter()
                .any(|tag| tag.first().map(String::as_str) == Some("e")
                    && tag.get(1) == Some(&publication.command_id)));
        }
    }
}

#[cfg(test)]
mod queue_tests {
    use buzz_core::session_control::ControlResult;
    #[test]
    fn publication_queue_is_bounded_and_terminal_replaces_pending() {
        let runtime = crate::session_controls::SessionControlRuntime::default();
        for id in 0..1000 {
            runtime.queue_result(format!("{id:064x}"), ControlResult::Pending);
        }
        runtime.queue_result(format!("{:064x}", 0), ControlResult::Terminal(Ok(())));
        let publications = runtime.take_publications();
        assert!(publications.len() <= 128);
        assert!(publications.iter().any(|publication| publication.command_id
            == format!("{:064x}", 0)
            && publication.result == ControlResult::Terminal(Ok(()))));
        assert!(
            !publications.iter().any(
                |publication| publication.command_id == format!("{:064x}", 0)
                    && publication.result == ControlResult::Pending
            )
        );
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[tokio::test]
    async fn control_result_publication_has_three_total_signed_attempts() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let received = attempts.clone();
        let server = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut request = vec![0; 8192];
                let count = socket.read(&mut request).await.unwrap();
                received.lock().unwrap().push(request[..count].to_vec());
                socket.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
            }
        });
        let rest = crate::relay::RestClient {
            http: reqwest::Client::new(),
            base_url: url,
            keys: nostr::Keys::generate(),
            auth_tag_json: None,
        };
        let publication = Publication {
            command_id: "a".repeat(64),
            result: buzz_core::session_control::ControlResult::Terminal(Ok(())),
        };
        assert!(publish_result(&rest, &publication).await.is_err());
        let requests = attempts.lock().unwrap();
        assert_eq!(
            requests.len(),
            3,
            "result helper owns the entire retry budget"
        );
        let mut ids = Vec::new();
        for request in requests.iter() {
            let text = String::from_utf8_lossy(request);
            assert!(text.to_ascii_lowercase().contains("authorization: nostr "));
            let (_, body) = text.split_once("\r\n\r\n").unwrap();
            let event: nostr::Event = serde_json::from_str(body).unwrap();
            event.verify().unwrap();
            ids.push(event.id);
        }
        assert!(ids.windows(2).all(|pair| pair[0] == pair[1]));
        server.abort();
    }
}
