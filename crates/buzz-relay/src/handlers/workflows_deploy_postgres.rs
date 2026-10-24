//! Real command, webhook and approval descendants across the deployment fence.
use super::*;
use crate::deploy_drain::WorkKind;
use buzz_core::channel::{ChannelType, ChannelVisibility};
use nostr::{EventBuilder, Keys, Kind, Tag};
use sha2::{Digest, Sha256};
use std::time::Duration;

#[tokio::test]
#[ignore = "requires Postgres"]
async fn deployment_manual_webhook_and_approval_runs_finish_after_legacy_timeout() {
    let pool = sqlx::PgPool::connect(&crate::test_support::database_url())
        .await
        .unwrap();
    let base = crate::state::tests::test_state_with_database_pool(pool.clone()).await;
    let mut configured = (*base).clone();
    Arc::make_mut(&mut configured.config).deploy_control_token =
        Some(crate::config::DeployControlToken::from_secret("a".repeat(64)).unwrap());
    assert!(configured
        .workflow_engine
        .set_run_admission(Arc::new(configured.deploy_drain.clone())));
    let state = Arc::new(configured);
    let host = format!("workflow-drain-{}.test", Uuid::new_v4().simple());
    let community = state
        .db
        .ensure_configured_community(&host)
        .await
        .unwrap()
        .id;
    let tenant = TenantContext::resolved(community, host);
    let owner = Keys::generate();
    state
        .db
        .ensure_user(community, &owner.public_key().to_bytes())
        .await
        .unwrap();
    let channel = state
        .db
        .create_channel(
            community,
            "drain",
            ChannelType::Stream,
            ChannelVisibility::Open,
            None,
            &owner.public_key().to_bytes(),
            None,
        )
        .await
        .unwrap();
    let definition = serde_json::json!({
        "name":"Drain runtime", "trigger":{"on":"webhook"},
        "steps":[{"id":"approved", "action":"delay", "duration":"0s"},
                 {"id":"long", "action":"delay", "duration":"38s"}],
        "enabled":true, "_webhook_secret":"synthetic-drain-secret"
    });
    let workflow = state
        .db
        .create_workflow(
            community,
            Some(channel.id),
            &owner.public_key().to_bytes(),
            "Drain runtime",
            &definition.to_string(),
            &[57; 32],
        )
        .await
        .unwrap();
    let approval_run = state
        .db
        .create_workflow_run(community, workflow, None, None)
        .await
        .unwrap();
    state
        .db
        .update_workflow_run(
            community,
            approval_run,
            RunStatus::WaitingApproval,
            0,
            &serde_json::json!([]),
            None,
        )
        .await
        .unwrap();
    let token = Uuid::new_v4().to_string();
    state
        .db
        .create_approval(buzz_db::workflow::CreateApprovalParams {
            community_id: community,
            token: &token,
            workflow_id: workflow,
            run_id: approval_run,
            step_id: "approved",
            step_index: 0,
            approver_spec: "any",
            expires_at: Utc::now() + chrono::Duration::minutes(5),
        })
        .await
        .unwrap();
    let auth = IngestAuth::Nip42 {
        pubkey: owner.public_key(),
        scopes: vec![],
        channel_ids: None,
        conn_id: Uuid::new_v4(),
    };
    let trigger = EventBuilder::new(Kind::Custom(KIND_WORKFLOW_TRIGGER as u16), "")
        .tag(Tag::parse(["d", &workflow.to_string()]).unwrap())
        .sign_with_keys(&owner)
        .unwrap();
    let approval = EventBuilder::new(Kind::Custom(KIND_APPROVAL_GRANT as u16), "")
        .tag(Tag::parse(["d", &hex::encode(Sha256::digest(token.as_bytes()))]).unwrap())
        .sign_with_keys(&owner)
        .unwrap();
    let parent = state.deploy_drain.try_admit_root(WorkKind::Http).unwrap();
    let operation = Uuid::new_v4();
    let boot = state.deploy_drain.begin(operation).unwrap().boot_id;
    state
        .deploy_drain
        .fence_http(operation, boot, "direct-probes-stopped")
        .unwrap();
    parent
        .scope(async {
            handle_workflow_trigger(&tenant, &state, &trigger, &auth)
                .await
                .unwrap();
            handle_approval_grant(&tenant, &state, &approval, &auth)
                .await
                .unwrap();
            let mut headers = axum::http::HeaderMap::new();
            headers.insert("host", tenant.host().parse().unwrap());
            headers.insert(
                "x-webhook-secret",
                "synthetic-drain-secret".parse().unwrap(),
            );
            let (status, _) = crate::api::bridge::workflow_webhook(
                axum::extract::State(state.clone()),
                axum::extract::Path(workflow.to_string()),
                axum::extract::Query(crate::api::bridge::WebhookQuery { secret: None }),
                headers,
                axum::body::Bytes::from_static(b"{}"),
            )
            .await
            .unwrap();
            assert_eq!(status, axum::http::StatusCode::ACCEPTED);
        })
        .await;
    parent.complete();
    assert_eq!(state.deploy_drain.snapshot().background_inflight, 3);
    tokio::time::sleep(Duration::from_secs(36)).await;
    assert_eq!(state.deploy_drain.snapshot().background_inflight, 3);
    assert!(!state.deploy_drain.snapshot().safe_to_stop);
    tokio::time::timeout(Duration::from_secs(10), async {
        while state.deploy_drain.snapshot().background_inflight != 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let complete: i64 = sqlx::query_scalar("SELECT count(*) FROM workflow_runs WHERE community_id=$1 AND workflow_id=$2 AND status='completed'")
        .bind(community.as_uuid()).bind(workflow).fetch_one(&pool).await.unwrap();
    assert_eq!(complete, 3);
    assert!(!state
        .deploy_drain
        .snapshot()
        .blocking_workers
        .iter()
        .any(|name| name.contains("unresolved")));
    // Only rows belonging to this random synthetic community are removed.
    for table in [
        "workflow_approvals",
        "workflow_runs",
        "workflows",
        "events",
        "channel_members",
        "channels",
        "users",
        "communities",
    ] {
        let key = if table == "communities" {
            "id"
        } else {
            "community_id"
        };
        let sql = format!("DELETE FROM {table} WHERE {key}=$1");
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(community.as_uuid())
            .execute(&pool)
            .await
            .unwrap();
    }
}
