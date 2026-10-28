use super::*;
use crate::{
    config::PermissionMode,
    observer::{ObserverEvent, ObserverHandle},
    queue::{EventQueue, QueuedEvent, RecoveryPolicy},
    scope::SessionScope,
};
use nostr::{EventBuilder, Keys, Kind, Tag};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};

fn work() -> (EventQueue, SessionScope, crate::queue::LogicalWork) {
    let scope = SessionScope::Conversation {
        channel_id: uuid::Uuid::new_v4(),
    };
    let policy = RecoveryPolicy {
        total_budget: Duration::from_secs(5),
        approval_progress_timeout: Duration::from_secs(1),
        ..Default::default()
    };
    let mut queue =
        EventQueue::new(crate::config::DedupMode::Queue).with_kimi_recovery(policy, "/tmp");
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope: scope.clone(),
        event: EventBuilder::new(Kind::Custom(9), "safe fixture")
            .sign_with_keys(&Keys::generate())
            .unwrap(),
        received_at: Instant::now(),
        prompt_tag: "test".into(),
        edit: None,
    });
    let work = queue.flush_next().unwrap().work.unwrap();
    (queue, scope, work)
}

async fn waiting(rx: &mut broadcast::Receiver<ObserverEvent>) -> uuid::Uuid {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let event = rx.recv().await.unwrap();
            if event.kind == "permission_lifecycle" && event.payload["state"] == "waiting_approval"
            {
                return uuid::Uuid::parse_str(event.payload["requestToken"].as_str().unwrap())
                    .unwrap();
            }
        }
    })
    .await
    .unwrap()
}

async fn manual(
    work: crate::queue::LogicalWork,
    duration: Duration,
) -> (
    tokio::task::JoinHandle<(Result<StopReason, AcpError>, Vec<ObserverEvent>)>,
    broadcast::Receiver<ObserverEvent>,
    mpsc::Sender<kimi_permission::PermissionDecision>,
) {
    let observer = ObserverHandle::in_process();
    let observed = observer.subscribe();
    let (tx, rx) = mpsc::channel(16);
    let mut client = AcpClient::spawn(
        "python3",
        &[
            "-u".into(),
            "-c".into(),
            kimi_permission_tests::ADAPTER.into(),
            kimi_permission_tests::REQUEST.into(),
            "success".into(),
        ],
        &[],
        false,
    )
    .await
    .unwrap();
    client.set_observer(Some(observer.clone()), 0);
    client.initialize().await.unwrap();
    client
        .session_new("/tmp", vec![], None, None)
        .await
        .unwrap();
    client.configure_kimi_policy(PermissionMode::Default);
    client.install_permission_decisions(rx);
    client.configure_kimi_work(Some(&work), None);
    let task = tokio::spawn(async move {
        let result = client
            .session_prompt_with_idle_timeout(
                "fixture-session",
                "manual fixture",
                Duration::from_secs(2),
                duration,
            )
            .await;
        client.shutdown().await;
        (result, observer.snapshot())
    });
    (task, observed, tx)
}

fn grants(events: &[ObserverEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            event.kind == "acp_write"
                && event
                    .payload
                    .pointer("/result/outcome/optionId")
                    .and_then(serde_json::Value::as_str)
                    == Some("approve_once")
        })
        .count()
}

#[tokio::test]
async fn kimi_manual_permission_requires_the_current_token_then_grants_once() {
    let (_queue, _scope, work) = work();
    let (task, mut observed, decisions) = manual(work, Duration::from_secs(3)).await;
    let token = waiting(&mut observed).await;
    decisions
        .send(kimi_permission::PermissionDecision {
            token: uuid::Uuid::new_v4(),
            allow: true,
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let event = observed.recv().await.unwrap();
            if event.payload["state"] == "stale_decision_ignored" {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(
        !task.is_finished(),
        "a stale vote cannot settle the request"
    );
    decisions
        .send(kimi_permission::PermissionDecision { token, allow: true })
        .await
        .unwrap();
    let (result, events) = task.await.unwrap();
    assert_eq!(result.unwrap(), StopReason::EndTurn);
    assert_eq!(grants(&events), 1);
}

#[tokio::test]
async fn kimi_manual_permission_late_allow_after_cancel_never_grants() {
    let (mut queue, scope, work) = work();
    let (task, mut observed, decisions) = manual(work, Duration::from_secs(3)).await;
    let token = waiting(&mut observed).await;
    assert!(queue.cancel_logical_work(&scope));
    decisions
        .send(kimi_permission::PermissionDecision { token, allow: true })
        .await
        .unwrap();
    let (result, events) = task.await.unwrap();
    assert!(result.is_err());
    assert_eq!(grants(&events), 0);
}

#[tokio::test]
async fn kimi_manual_permission_timeout_has_no_implicit_approval() {
    let (_queue, _scope, work) = work();
    let (task, mut observed, _decisions) = manual(work, Duration::from_millis(200)).await;
    let _ = waiting(&mut observed).await;
    let (result, events) = task.await.unwrap();
    assert!(
        matches!(result, Err(AcpError::PermissionDecisionTimeout)),
        "{result:?}"
    );
    assert_eq!(grants(&events), 0);
}

#[test]
fn kimi_manual_control_authenticates_owner_agent_and_command_intent() {
    let owner = Keys::generate();
    let other = Keys::generate();
    let agent = Keys::generate();
    let token = uuid::Uuid::new_v4();
    let mut forged = EventBuilder::new(Kind::Custom(9), format!("!approve {token}"))
        .tag(Tag::parse(["p", &agent.public_key().to_hex()]).unwrap())
        .sign_with_keys(&other)
        .unwrap();
    forged.pubkey = owner.public_key();
    assert!(kimi_permission::owner_decision(
        &forged,
        &agent.public_key().to_hex(),
        Some(&owner.public_key().to_hex())
    )
    .is_none());
    for (actor, text, mention, expected) in [
        (&owner, format!("!approve {token}"), true, Some(true)),
        (
            &owner,
            format!("!deny {token} @Example Agent"),
            true,
            Some(false),
        ),
        (&other, format!("!approve {token}"), true, None),
        (&owner, format!("!approve {token}"), false, None),
        (&owner, format!("never send !approve {token}"), true, None),
        (&owner, "!approve invalid-token".into(), true, None),
    ] {
        let mut builder = EventBuilder::new(Kind::Custom(9), text);
        if mention {
            builder = builder.tag(Tag::parse(["p", &agent.public_key().to_hex()]).unwrap());
        }
        let event = builder.sign_with_keys(actor).unwrap();
        assert_eq!(
            kimi_permission::owner_decision(
                &event,
                &agent.public_key().to_hex(),
                Some(&owner.public_key().to_hex())
            )
            .map(|decision| decision.allow),
            expected
        );
    }
}
