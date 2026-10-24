//! Real scheduler admission/claim race, isolated from other tests' workflows.
use std::{sync::Arc, time::Duration};

use buzz_core::channel::{ChannelType, ChannelVisibility};
use buzz_db::Db;
use buzz_relay::deploy_drain::DrainController;
use buzz_workflow::{WorkflowConfig, WorkflowEngine};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires Postgres"]
async fn deployment_scheduler_claim_racing_begin_retains_run_and_fences_later_claims() {
    let database_url = std::env::var("BUZZ_TEST_DATABASE_URL").expect("test database URL");
    let admin = sqlx::PgPool::connect(&database_url).await.unwrap();
    let name = format!("deploy_drain_{}", Uuid::new_v4().simple());
    // Audited identifier: fixed ASCII prefix plus generated UUID hex only.
    let create_sql = format!("CREATE DATABASE \"{name}\"");
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&admin)
        .await
        .unwrap();
    let mut url = url::Url::parse(&database_url).unwrap();
    url.set_path(&format!("/{name}"));
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(url.as_str())
        .await
        .unwrap();
    let db = Db::from_pool(pool.clone());
    db.migrate().await.unwrap();
    let community = db
        .ensure_configured_community("scheduler-drain.test")
        .await
        .unwrap()
        .id;
    let owner = nostr::Keys::generate().public_key().to_bytes();
    db.ensure_user(community, &owner).await.unwrap();
    let channel = db
        .create_channel(
            community,
            "scheduler-drain",
            ChannelType::Stream,
            ChannelVisibility::Open,
            None,
            &owner,
            None,
        )
        .await
        .unwrap();
    let definition = serde_json::json!({
        "name": "Drain race", "trigger": {"on": "schedule", "cron": "* * * * * * *"},
        "steps": [{"id": "wait", "action": "delay", "duration": "0s"}], "enabled": true,
    });
    let workflow = db
        .create_workflow(
            community,
            Some(channel.id),
            &owner,
            "Drain race",
            &definition.to_string(),
            &[31; 32],
        )
        .await
        .unwrap();
    let engine = Arc::new(WorkflowEngine::new(db.clone(), WorkflowConfig::default()));
    let drain = DrainController::new();
    assert!(engine.set_run_admission(Arc::new(drain.clone())));

    // Hold the real claim table, so admitted work cannot reach its durable
    // claim or spawn before begin closes autonomous admission.
    let mut holder = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE scheduled_workflow_fires IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *holder)
        .await
        .unwrap();
    let tick = tokio::spawn({
        let engine = engine.clone();
        async move { engine.run_scheduled_tick().await }
    });
    let admitted = tokio::time::timeout(Duration::from_secs(5), async {
        while drain.snapshot().background_inflight != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_ok();
    let operation = Uuid::new_v4();
    let boot = drain.begin(operation).unwrap().boot_id;
    drain.fence_http(operation, boot, "qualified-edge").unwrap();
    let parked = drain.snapshot();
    holder.commit().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), tick)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while drain.snapshot().background_inflight != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let completed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workflow_runs WHERE community_id=$1 AND workflow_id=$2 AND status='completed'",
    ).bind(community.as_uuid()).bind(workflow).fetch_one(&pool).await.unwrap();

    // Clear this database's one fire to make a missing admission barrier cause
    // a real second durable claim, instead of being hidden by deduplication.
    sqlx::query("DELETE FROM scheduled_workflow_fires")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workflow_runs")
        .execute(&pool)
        .await
        .unwrap();
    engine.run_scheduled_tick().await;
    let later_claims: i64 = sqlx::query_scalar("SELECT count(*) FROM scheduled_workflow_fires")
        .fetch_one(&pool)
        .await
        .unwrap();
    let finished = drain.snapshot();
    drop(engine);
    drop(db);
    pool.close().await;
    // The same generated, private test database created above; no shared DB.
    let drop_sql = format!("DROP DATABASE \"{name}\"");
    sqlx::query(sqlx::AssertSqlSafe(drop_sql.as_str()))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
    assert!(
        admitted,
        "scheduler must own work before the real durable claim"
    );
    assert_eq!(parked.background_inflight, 1);
    assert!(!parked.safe_to_stop);
    assert_eq!(completed, 1);
    assert_eq!(
        later_claims, 0,
        "fenced tick must not consume a durable fire"
    );
    assert!(finished.safe_to_stop);
}
