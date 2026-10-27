//! Exercise the shipped migration command on its real detached PostgreSQL backend.

use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

use buzz_db::deletion::SCHEMA_DESTRUCTION_LOCK_KEY;
use serde_json::Value;
use sqlx::migrate::Migrate;
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, PgPool};
use tokio::process::{Child, Command};
use tokio::time::timeout;

struct Database {
    admin: PgPool,
    pool: PgPool,
    name: String,
    url: String,
}

impl Database {
    async fn create() -> Self {
        let base = std::env::var("BUZZ_TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .expect("isolated test database URL");
        let admin = PgPoolOptions::new()
            .max_connections(3)
            .connect(&base)
            .await
            .expect("admin pool");
        let name = format!("buzz_migration_cli_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(AssertSqlSafe(format!("CREATE DATABASE {name}")))
            .execute(&admin)
            .await
            .expect("create isolated migration database");
        let mut url = url::Url::parse(&base).expect("database URL");
        url.set_path(&format!("/{name}"));
        url.query_pairs_mut().append_pair("application_name", &name);
        let url = url.to_string();
        let pool = PgPoolOptions::new()
            .max_connections(3)
            .connect(&url)
            .await
            .expect("test pool");
        Self {
            admin,
            pool,
            name,
            url,
        }
    }

    fn child(&self, lock_ms: u32, statement_ms: u32) -> Child {
        // Nextest supplies the relocated executable when running a test archive.
        let binary = std::env::var_os("NEXTEST_BIN_EXE_buzz_admin")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_buzz-admin").into());
        Command::new(binary)
            .args([
                "migrate",
                "--lock-timeout-ms",
                &lock_ms.to_string(),
                "--statement-timeout-ms",
                &statement_ms.to_string(),
            ])
            .env("DATABASE_URL", &self.url)
            .env("BUZZ_DB_LOCK_TIMEOUT_MS", "2000")
            .env("BUZZ_DB_STATEMENT_TIMEOUT_MS", "2000")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("real migration CLI")
    }

    async fn cleanup(self) {
        self.pool.close().await;
        sqlx::query(AssertSqlSafe(format!(
            "DROP DATABASE {} WITH (FORCE)",
            self.name
        )))
        .execute(&self.admin)
        .await
        .expect("drop only owned isolated database");
        self.admin.close().await;
    }
}

async fn finish(mut child: Child) -> (bool, Output, Duration) {
    let start = Instant::now();
    let timed_out = timeout(Duration::from_secs(2), child.wait()).await.is_err();
    if timed_out {
        child
            .kill()
            .await
            .expect("kill only owned stalled CLI subprocess");
    }
    let output = child.wait_with_output().await.expect("CLI output");
    (timed_out, output, start.elapsed())
}

fn evidence(output: &Output, stage: &str) -> Value {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|record| {
            (record["fields"]["event"] == "migration_timeouts"
                && record["fields"]["stage"] == stage)
                .then(|| record["fields"].clone())
        })
        .unwrap_or_else(|| panic!("missing effective migration backend evidence at {stage}"))
}

fn assert_budgets(value: &Value, lock_ms: u32, statement_ms: u32) {
    assert!(value["backend_pid"].as_i64().is_some_and(|pid| pid > 0));
    assert_eq!(value["lock_timeout_ms"], lock_ms);
    assert_eq!(value["statement_timeout_ms"], statement_ms);
}

mod postgres_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn migration_schema_lock_timeout_is_enforced_on_cli_backend() {
        let db = Database::create().await;
        let mut holder = db.pool.acquire().await.expect("schema holder");
        sqlx::query("SELECT pg_advisory_lock_shared($1)")
            .bind(SCHEMA_DESTRUCTION_LOCK_KEY)
            .execute(&mut *holder)
            .await
            .expect("hold destructive shared lock");
        let result = finish(db.child(100, 300)).await;
        sqlx::query("SELECT pg_advisory_unlock_shared($1)")
            .bind(SCHEMA_DESTRUCTION_LOCK_KEY)
            .execute(&mut *holder)
            .await
            .expect("release shared lock");
        drop(holder);
        db.cleanup().await;
        assert!(
            !result.0,
            "CLI discarded its timeout budgets and remained blocked"
        );
        assert!(!result.1.status.success());
        assert!(String::from_utf8_lossy(&result.1.stderr).contains("lock timeout"));
        assert_budgets(&evidence(&result.1, "before_schema_lock"), 100, 300);
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn migration_sqlx_lock_wait_is_bounded_after_schema_lock() {
        let db = Database::create().await;
        let mut holder = db.pool.acquire().await.expect("SQLx lock holder");
        Migrate::lock(&mut *holder)
            .await
            .expect("hold actual SQLx migration lock");
        let result = finish(db.child(100, 300)).await;
        Migrate::unlock(&mut *holder)
            .await
            .expect("release SQLx lock");
        drop(holder);
        db.cleanup().await;
        assert!(
            !result.0,
            "SQLx migration lock wait escaped the bounded policy"
        );
        assert!(!result.1.status.success());
        let before = evidence(&result.1, "before_schema_lock");
        let execution = evidence(&result.1, "before_migrations");
        assert_budgets(&before, 100, 300);
        assert_budgets(&execution, 100, 300);
        assert_eq!(before["backend_pid"], execution["backend_pid"]);
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn migration_statement_timeout_cancels_actual_cli_ddl() {
        let db = Database::create().await;
        sqlx::raw_sql("CREATE FUNCTION migration_delay() RETURNS event_trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(3); END $$; \
            CREATE EVENT TRIGGER migration_delay ON ddl_command_start WHEN TAG IN ('CREATE TABLE') EXECUTE FUNCTION migration_delay();")
            .execute(&db.pool).await.expect("install isolated slow DDL trigger");
        let result = finish(db.child(100, 300)).await;
        let mut probe = db.pool.acquire().await.expect("safety probe");
        let released: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock_shared($1)")
            .bind(SCHEMA_DESTRUCTION_LOCK_KEY)
            .fetch_one(&mut *probe)
            .await
            .expect("probe safety lock");
        if released {
            sqlx::query("SELECT pg_advisory_unlock_shared($1)")
                .bind(SCHEMA_DESTRUCTION_LOCK_KEY)
                .execute(&mut *probe)
                .await
                .expect("release safety probe");
        }
        drop(probe);
        db.cleanup().await;
        assert!(!result.0, "migration statement ignored its timeout");
        assert!(!result.1.status.success());
        assert!(String::from_utf8_lossy(&result.1.stderr).contains("statement timeout"));
        assert!(
            released,
            "failed migration left the schema safety session locked"
        );
        assert_budgets(&evidence(&result.1, "before_migrations"), 100, 300);
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn migration_exact_deploy_budgets_preserve_already_applied_checksums() {
        let db = Database::create().await;
        let first = finish(db.child(5000, 300000)).await;
        let before: Vec<(i64, Vec<u8>)> = sqlx::query_as(
            "SELECT version,checksum FROM _sqlx_migrations WHERE success ORDER BY version",
        )
        .fetch_all(&db.pool)
        .await
        .expect("first applied checksums");
        let second = finish(db.child(5000, 300000)).await;
        let after: Vec<(i64, Vec<u8>)> = sqlx::query_as(
            "SELECT version,checksum FROM _sqlx_migrations WHERE success ORDER BY version",
        )
        .fetch_all(&db.pool)
        .await
        .expect("second applied checksums");
        db.cleanup().await;
        assert!(
            !first.0 && first.1.status.success(),
            "initial bounded migration failed"
        );
        assert!(
            !second.0 && second.1.status.success(),
            "already-applied migration failed"
        );
        assert!(!before.is_empty());
        assert_eq!(before, after);
        for output in [&first.1, &second.1] {
            let before = evidence(output, "before_schema_lock");
            let execution = evidence(output, "before_migrations");
            assert_budgets(&before, 5000, 300000);
            assert_budgets(&execution, 5000, 300000);
            assert_eq!(before["backend_pid"], execution["backend_pid"]);
        }
    }

    async fn pending_last_migration(db: &Database) {
        let initial = finish(db.child(5000, 300000)).await;
        assert!(
            !initial.0 && initial.1.status.success(),
            "initialize frozen migration history"
        );
        // Roll back only this test database's last fixture state. Frozen SQL and
        // recorded checksums in the repository remain unchanged.
        sqlx::raw_sql(
            "DROP TABLE managed_deliveries; DELETE FROM _sqlx_migrations WHERE version = 56; \
                       CREATE TABLE migration_read_write_probe (id int PRIMARY KEY)",
        )
        .execute(&db.pool)
        .await
        .expect("prepare pending real migration 0056");
    }

    async fn wait_for_ddl(db: &Database) -> i32 {
        timeout(Duration::from_secs(3), async {
            loop {
                let pid: Option<i32> = sqlx::query_scalar(
                    "SELECT pid FROM pg_stat_activity WHERE datname = current_database() \
                     AND wait_event_type = 'Lock' AND query LIKE '%CREATE TABLE managed_deliveries%'")
                    .fetch_optional(&db.pool).await.expect("observe actual pending migration DDL");
                if let Some(pid) = pid { break pid; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.expect("CLI reaches frozen expand DDL")
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn migration_table_lock_timeout_leaves_no_waiter_and_queries_continue() {
        let db = Database::create().await;
        pending_last_migration(&db).await;
        let mut blocker = db.pool.begin().await.expect("DDL blocker");
        sqlx::query("LOCK TABLE communities IN SHARE UPDATE EXCLUSIVE MODE")
            .execute(&mut *blocker)
            .await
            .expect("hold referenced table lock");
        let child = db.child(500, 1500);
        let pid = wait_for_ddl(&db).await;
        let result = finish(child).await;
        let waiting: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE pid = $1)")
                .bind(pid)
                .fetch_one(&db.pool)
                .await
                .expect("check migration backend cleanup");
        // A failed expand must not leave an orphan that later mutates schema.
        blocker.rollback().await.expect("release table lock");
        sqlx::query("INSERT INTO migration_read_write_probe VALUES (1)")
            .execute(&db.pool)
            .await
            .expect("write after timeout");
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_read_write_probe")
            .fetch_one(&db.pool)
            .await
            .expect("read after timeout");
        let applied: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version = 56)")
                .fetch_one(&db.pool)
                .await
                .expect("failed migration not marked applied");
        db.cleanup().await;
        assert!(
            !result.0 && !result.1.status.success(),
            "actual DDL lock must time out"
        );
        assert!(String::from_utf8_lossy(&result.1.stderr).contains("lock timeout"));
        assert!(!waiting, "failed DDL backend remained alive");
        assert!(!applied);
        assert_eq!(count, 1);
        assert_budgets(&evidence(&result.1, "before_migrations"), 500, 1500);
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn migration_cli_cancellation_keeps_safety_lock_until_backend_finishes() {
        let db = Database::create().await;
        pending_last_migration(&db).await;
        let mut blocker = db.pool.begin().await.expect("DDL blocker");
        sqlx::query("LOCK TABLE communities IN SHARE UPDATE EXCLUSIVE MODE")
            .execute(&mut *blocker)
            .await
            .expect("hold expand DDL");
        let mut child = db.child(5000, 300000);
        let pid = wait_for_ddl(&db).await;
        child.kill().await.expect("cancel only owned CLI");
        let _ = child.wait_with_output().await.expect("reap owned CLI");
        let mut probe = db.pool.acquire().await.expect("destructive holder probe");
        for _ in 0..5 {
            let shared: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock_shared($1)")
                .bind(SCHEMA_DESTRUCTION_LOCK_KEY)
                .fetch_one(&mut *probe)
                .await
                .expect("probe exclusion");
            assert!(
                !shared,
                "cancelled CLI exposed safety lock while its backend still executes DDL"
            );
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        blocker
            .rollback()
            .await
            .expect("let same backend complete DDL");
        timeout(Duration::from_secs(3), async {
            loop {
                let alive: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE pid = $1)",
                )
                .bind(pid)
                .fetch_one(&mut *probe)
                .await
                .expect("poll backend termination");
                if !alive {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("backend completes on closed client socket");
        let shared: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock_shared($1)")
            .bind(SCHEMA_DESTRUCTION_LOCK_KEY)
            .fetch_one(&mut *probe)
            .await
            .expect("probe after backend finishes");
        assert!(shared);
        sqlx::query("SELECT pg_advisory_unlock_shared($1)")
            .bind(SCHEMA_DESTRUCTION_LOCK_KEY)
            .execute(&mut *probe)
            .await
            .expect("release probe");
        drop(probe);
        db.cleanup().await;
    }
}
