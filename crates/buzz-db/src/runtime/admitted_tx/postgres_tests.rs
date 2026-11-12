//! Savepoint rollbacks preserve atomic event, mention and durable push work.

use super::*;
use crate::replaceable::{ParameterizedReplacePrecondition, ParameterizedReplaceStatus};
use crate::{Db, DbConfig};
use nostr::{EventBuilder, Keys, Kind, Tag, Timestamp};

#[tokio::test]
#[ignore = "requires Postgres"]
async fn savepoint_failure_and_duplicate_preserve_only_surviving_durable_push_work() {
    let db = Db::new(&DbConfig {
        database_url: crate::test_support::database_url(),
        push_enabled: true,
        ..DbConfig::default()
    })
    .await
    .unwrap();
    let community = CommunityId::from_uuid(Uuid::new_v4());
    sqlx::query("INSERT INTO communities (id, host) VALUES ($1, $2)")
        .bind(community.as_uuid())
        .bind(format!("push-savepoint-{community}.example"))
        .execute(db.pool())
        .await
        .unwrap();
    let keys = Keys::generate();
    let mention = Keys::generate().public_key().to_hex();
    // The generic transaction-bound replacement seam supports arbitrary kinds.
    // Use a push-eligible kind here so enqueue before savepoint release is
    // observable; ordinary NIP-33 kinds are not push eligible.
    let event = |slot: &str, timestamp: u64| {
        EventBuilder::new(Kind::Custom(9), slot)
            .tags([
                Tag::identifier(slot),
                Tag::parse(["p", mention.as_str()]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(timestamp))
            .sign_with_keys(&keys)
            .unwrap()
    };
    let retained = event("retained", 1_769_904_000);
    let failed = event("failed", 1_772_323_200);
    let duplicate = event("duplicate", 1_775_001_600);
    let later = event("later", 1_777_593_600);
    // A tombstoned row makes the replacement's insert take its actual
    // ON CONFLICT / ROLLBACK TO SAVEPOINT arm, not its early live-head no-op.
    crate::event::insert_event(db.pool(), community, &duplicate, None)
        .await
        .unwrap();
    sqlx::query("UPDATE events SET deleted_at = now() WHERE community_id = $1 AND id = $2")
        .bind(community.as_uuid())
        .bind(duplicate.id.as_bytes().as_slice())
        .execute(db.pool())
        .await
        .unwrap();
    // Test-local constraint injects an indexing failure after the event INSERT,
    // without changing production triggers or any persistent schema contract.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "ALTER TABLE event_mentions ADD CONSTRAINT reject_failed_timestamp_test \
         CHECK (event_id <> decode('{}', 'hex'))",
        hex::encode(failed.id.as_bytes()),
    )))
    .execute(db.pool())
    .await
    .unwrap();
    crate::push::replace_active_lease(
        db.pool(),
        community,
        &keys.public_key().to_bytes(),
        "savepoint-installation",
        crate::push::LeaseVersion {
            source_event_id: &[3; 32],
            source_created_at: 1,
            generation: 1,
            expires_at: chrono::Utc::now().timestamp() + 3600,
        },
        crate::push::ActiveLease {
            endpoint_hash: &[4; 32],
            endpoint_grant: "test-grant",
            max_class: "default",
            subscriptions: &serde_json::json!([]),
        },
    )
    .await
    .unwrap();
    let mut tx = db.begin_event_write_transaction(community).await.unwrap();
    assert!(
        crate::event::insert_event_in_transaction(&mut tx, &retained, None)
            .await
            .unwrap()
            .1
    );
    let pending: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT event_id FROM push_match_queue WHERE community_id=$1 ORDER BY event_id",
    )
    .bind(community.as_uuid())
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(pending, vec![retained.id.as_bytes().to_vec()]);
    let error = db
        .replace_parameterized_event_in_transaction(
            &mut tx,
            &failed,
            "failed",
            None,
            ParameterizedReplacePrecondition::Unconditional,
        )
        .await
        .expect_err("index failure rolls replacement back to savepoint");
    assert!(error.to_string().contains("reject_failed_timestamp_test"));
    let pending: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT event_id FROM push_match_queue WHERE community_id=$1 ORDER BY event_id",
    )
    .bind(community.as_uuid())
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(
        pending,
        vec![retained.id.as_bytes().to_vec()],
        "failed savepoint leaves no durable enqueue"
    );
    let replay = db
        .replace_parameterized_event_in_transaction(
            &mut tx,
            &duplicate,
            "duplicate",
            None,
            ParameterizedReplacePrecondition::Unconditional,
        )
        .await
        .unwrap();
    assert_eq!(replay.status, ParameterizedReplaceStatus::Duplicate);
    let pending: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT event_id FROM push_match_queue WHERE community_id=$1 ORDER BY event_id",
    )
    .bind(community.as_uuid())
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(
        pending,
        vec![retained.id.as_bytes().to_vec()],
        "duplicate savepoint leaves no durable enqueue"
    );
    let result = db
        .replace_parameterized_event_in_transaction(
            &mut tx,
            &later,
            "later",
            None,
            ParameterizedReplacePrecondition::Unconditional,
        )
        .await
        .unwrap();
    assert_eq!(result.status, ParameterizedReplaceStatus::Inserted);
    let visible: i64 =
        sqlx::query_scalar("SELECT count(*) FROM push_match_queue WHERE community_id=$1")
            .bind(community.as_uuid())
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(visible, 0, "push work stays uncommitted with its event");
    tx.commit().await.unwrap();
    let queued: Vec<(Vec<u8>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT e.id,e.created_at FROM push_match_queue q JOIN events e \
         ON e.community_id=q.community_id AND e.id=q.event_id \
         WHERE q.community_id=$1 ORDER BY e.created_at",
    )
    .bind(community.as_uuid())
    .fetch_all(db.pool())
    .await
    .unwrap();
    assert_eq!(queued.len(), 2);
    for ((id, created_at), event) in queued.iter().zip([&retained, &later]) {
        assert_eq!(id, event.id.as_bytes().as_slice());
        assert_eq!(created_at.timestamp(), event.created_at.as_secs() as i64);
    }
}
