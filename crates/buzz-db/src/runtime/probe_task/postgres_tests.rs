//! The public lifecycle owns the worker and its dedicated pool across clones
//! and interrupted shutdowns. Removing cancellation, joining or pool closure
//! must leave an observable connection or prevent shutdown from finishing.

use super::*;
use crate::{test_support::ScratchDb, Db, DbConfig};

#[tokio::test]
#[ignore = "requires Postgres"]
async fn cluster_global_probe_shutdown_is_shared_and_retryable_after_cancellation() {
    let (_fixture_pool, scratch) =
        ScratchDb::migrated(&crate::test_support::database_url(), "owned_probe").await;
    let db = Db::new(&DbConfig {
        database_url: scratch.url().to_owned(),
        read_database_url: Some(scratch.url().to_owned()),
        min_connections: 0,
        max_connections: 2,
        ..DbConfig::default()
    })
    .await
    .unwrap();
    let cloned = db.clone();
    for interrupted in [false, true] {
        assert!(db.spawn_fence_probe().await.unwrap());
        assert!(cloned.spawn_fence_probe().await.unwrap());
        tokio::time::timeout(Duration::from_secs(5), async {
            while db.fence().verified_through().is_none() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("public startup must run the real heartbeat probe");
        let pool = {
            let owned = db.fence_probe.lock().await;
            owned.as_ref().unwrap().pool.clone()
        };
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() \
             AND application_name=$1",
        )
        .bind(replica_fence::PROBE_APPLICATION_NAME)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(count, 1, "clones must share one probe pool");
        if interrupted {
            let checkout = pool.acquire().await.unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(50), cloned.shutdown_fence_probe())
                    .await
                    .is_err(),
                "an outstanding checkout holds pool closure open"
            );
            assert!(
                db.fence_probe.lock().await.is_some(),
                "interruption must retain the worker and its cleanup owner"
            );
            drop(checkout);
        }
        tokio::time::timeout(Duration::from_secs(5), db.shutdown_fence_probe())
            .await
            .expect("shutdown cancels and joins the actual probe")
            .unwrap();
        cloned.shutdown_fence_probe().await.unwrap();
        assert!(pool.is_closed(), "shutdown closes the dedicated pool");
        assert_eq!(
            pool.size(),
            0,
            "no maintenance connection survives shutdown"
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() \
             AND application_name=$1",
        )
        .bind(replica_fence::PROBE_APPLICATION_NAME)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(count, 0, "probe backend is gone before shutdown returns");
        db.fence().close();
    }
    db.read_pool.as_ref().unwrap().close().await;
    db.pool().close().await;
}
