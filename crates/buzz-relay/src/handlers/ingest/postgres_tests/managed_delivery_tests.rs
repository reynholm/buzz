//! Recovery regressions use the production writer constructor and its commit-time floor.
//! A separate unguarded fixture pool is used only to seed events accepted before the outage.
use super::*;
use buzz_core::managed_delivery::ReportStamp;
use buzz_db::channel::{ChannelType, ChannelVisibility};
use nostr::{Keys, Tag, Timestamp};

#[tokio::test]
#[ignore = "requires Postgres and Redis"]
async fn managed_old_absent_report_recovers_with_production_writer() {
    old_report_recovery(false, false).await;
}

#[tokio::test]
#[ignore = "requires Postgres and Redis"]
async fn managed_old_accepted_report_recovers_with_production_writer() {
    old_report_recovery(true, false).await;
}

#[tokio::test]
#[ignore = "requires Postgres and Redis"]
async fn managed_old_deleted_original_never_resurrects() {
    old_report_recovery(true, true).await;
}

async fn old_report_recovery(already_accepted: bool, deleted: bool) {
    let state = authenticated_http_state().await;
    let pool = state.db.pool().clone();
    let host = format!("managed-recovery-{}.test", Uuid::new_v4().simple());
    let community = state
        .db
        .ensure_configured_community(&host)
        .await
        .unwrap()
        .id;
    let tenant = TenantContext::resolved(community, &host);
    let author = Keys::generate();
    let channel = Uuid::new_v4();
    state
        .db
        .create_channel_with_id(
            community,
            channel,
            "managed recovery",
            ChannelType::Stream,
            ChannelVisibility::Open,
            None,
            author.public_key().as_bytes(),
            None,
        )
        .await
        .unwrap();
    let tracer: Arc<dyn buzz_conformance::Tracer> = Arc::new(VecTracer::default());
    let auth = || IngestAuth::Http {
        pubkey: author.public_key(),
        scopes: vec![Scope::MessagesWrite],
        auth_method: HttpAuthMethod::Nip98,
    };
    let root = EventBuilder::new(Kind::Custom(9), "original task")
        .tag(Tag::parse(["h", &channel.to_string()]).unwrap())
        .sign_with_keys(&author)
        .unwrap();
    ingest_event_inner(&state, &tracer, &tenant, root.clone(), auth())
        .await
        .unwrap();
    {
        let key = Uuid::new_v4().to_string();
        let event = EventBuilder::new(Kind::Custom(9), "original final report")
            .custom_created_at(Timestamp::from(Timestamp::now().as_secs() - 3600))
            .tags([
                Tag::parse(["h", &channel.to_string()]).unwrap(),
                Tag::parse(["e", &root.id.to_hex(), "", "root"]).unwrap(),
                Tag::parse(["e", &root.id.to_hex(), "", "reply"]).unwrap(),
                Tag::parse(["buzz-delivery", &key]).unwrap(),
            ])
            .sign_with_keys(&author)
            .unwrap();
        if already_accepted {
            seed_previously_accepted_event(&state, &tracer, &tenant, event.clone(), auth()).await;
        }
        if !already_accepted {
            let rejected =
                ingest_event_inner(&state, &tracer, &tenant, event.clone(), auth()).await;
            assert!(
                matches!(rejected, Err(IngestError::Internal(message)) if message.contains("replica-fence floor"))
            );
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM managed_deliveries WHERE community_id=$1 AND author=$2 AND delivery_key=$3")
                .bind(community.as_uuid()).bind(author.public_key().to_bytes().as_slice())
                .bind(Uuid::parse_str(&key).unwrap()).fetch_one(&pool).await.unwrap();
            assert_eq!(
                count, 0,
                "deferred floor failure must roll back canonical binding"
            );
        }
        if deleted {
            sqlx::query("UPDATE events SET deleted_at=now() WHERE community_id=$1 AND id=$2")
                .bind(community.as_uuid())
                .bind(event.id.as_bytes().as_slice())
                .execute(&pool)
                .await
                .unwrap();
        }
        let candidate = EventBuilder::new(event.kind, &event.content)
            .tags(event.tags.clone())
            .allow_self_tagging()
            .sign_with_keys(&author)
            .unwrap();
        let recovery = EventBuilder::new(
            Kind::Custom(20003),
            serde_json::json!({"original":event,"candidate":ReportStamp::from(&candidate)})
                .to_string(),
        )
        .tag(Tag::parse(["h", &channel.to_string()]).unwrap())
        .sign_with_keys(&author)
        .unwrap();
        let receipt = ingest_event_inner(&state, &tracer, &tenant, recovery.clone(), auth())
            .await
            .expect("recovery must elect one report without disarming the writer floor");
        let chosen = if already_accepted { &event } else { &candidate };
        assert!(receipt.accepted);
        assert_eq!(receipt.event_id, chosen.id.to_hex());
        let counts: (i32, i32) = sqlx::query_as("SELECT reply_count, descendant_count FROM thread_metadata WHERE community_id=$1 AND event_id=$2")
            .bind(community.as_uuid()).bind(root.id.as_bytes().as_slice()).fetch_one(&pool).await.unwrap();
        let replay = ingest_event_inner(&state, &tracer, &tenant, recovery, auth())
            .await
            .unwrap();
        assert!(replay.accepted);
        assert!(replay.message.starts_with("managed-delivery:"));
        let stored = state
            .db
            .get_event_by_id(community, chosen.id.as_bytes())
            .await
            .unwrap();
        if deleted {
            assert!(stored.is_none(), "deleted report must remain hidden");
        } else {
            assert_eq!(&stored.unwrap().event, chosen);
        }
        let next_candidate = EventBuilder::new(candidate.kind, &candidate.content)
            .tags(candidate.tags.clone())
            .allow_self_tagging()
            .custom_created_at(Timestamp::from(Timestamp::now().as_secs() + 1))
            .sign_with_keys(&author)
            .unwrap();
        let next_wrapper = EventBuilder::new(
            Kind::Custom(20003),
            serde_json::json!({"original":event,"candidate":ReportStamp::from(&next_candidate)})
                .to_string(),
        )
        .tag(Tag::parse(["h", &channel.to_string()]).unwrap())
        .sign_with_keys(&author)
        .unwrap();
        let (a, b) = tokio::join!(
            ingest_event_inner(&state, &tracer, &tenant, next_wrapper.clone(), auth()),
            ingest_event_inner(&state, &tracer, &tenant, next_wrapper, auth())
        );
        assert_eq!(a.unwrap().event_id, chosen.id.to_hex());
        assert_eq!(b.unwrap().event_id, chosen.id.to_hex());
        let report_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE community_id=$1 AND tags @> $2")
                .bind(community.as_uuid())
                .bind(serde_json::json!([["buzz-delivery", key]]))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(report_count, 1);
        let replay_counts: (i32, i32) = sqlx::query_as("SELECT reply_count, descendant_count FROM thread_metadata WHERE community_id=$1 AND event_id=$2")
            .bind(community.as_uuid()).bind(root.id.as_bytes().as_slice()).fetch_one(&pool).await.unwrap();
        assert_eq!(
            counts, replay_counts,
            "duplicate replay must not increment thread counters"
        );
    }
}

async fn authenticated_http_state() -> Arc<crate::state::AppState> {
    let mut config = crate::config::Config::for_test();
    config.database_url = crate::test_support::database_url();
    config.redis_url = std::env::var("REDIS_URL").expect("local test Redis");
    config.require_auth_token = true;
    config.require_relay_membership = false;
    let db = buzz_db::Db::new(&buzz_db::DbConfig {
        database_url: config.database_url.clone(),
        max_connections: 4,
        ..Default::default()
    })
    .await
    .unwrap();
    buzz_db::migration::run_migrations(db.pool()).await.unwrap();
    let pool = db.pool().clone();
    let floor: String = sqlx::query_scalar("SELECT current_setting('buzz.created_at_floor')")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        floor, "960",
        "the recovery path must retain the production writer floor"
    );
    let redis = deadpool_redis::Config::from_url(&config.redis_url)
        .create_pool(Some(deadpool_redis::Runtime::Tokio1))
        .unwrap();
    let pubsub = Arc::new(
        buzz_pubsub::PubSubManager::new(&config.redis_url, redis.clone())
            .await
            .unwrap(),
    );
    let audit = buzz_audit::AuditService::new(pool.clone());
    let auth = buzz_auth::AuthService::new(config.auth.clone());
    let search = buzz_search::SearchService::new(pool.clone());
    let workflows = Arc::new(buzz_workflow::WorkflowEngine::new(
        db.clone(),
        buzz_workflow::WorkflowConfig::default(),
    ));
    let media = buzz_media::MediaStorage::new(&config.media).unwrap();
    let (state, _) = crate::state::AppState::new(
        config,
        db,
        redis,
        audit,
        pubsub,
        auth,
        search,
        workflows,
        Keys::generate(),
        media,
    );
    Arc::new(state)
}

// Historical setup models an already-committed report, not a recovery write.
// Never use this unarmed pool for the operation under test.
async fn seed_previously_accepted_event(
    production: &crate::state::AppState,
    tracer: &Arc<dyn buzz_conformance::Tracer>,
    tenant: &TenantContext,
    event: nostr::Event,
    auth: IngestAuth,
) {
    let db_url = &production.config.database_url;
    let historical_pool = sqlx::PgPool::connect(db_url).await.unwrap();
    let historical_state = build_canvas_ingest_state(db_url, &historical_pool).await;
    ingest_event_inner(&historical_state, tracer, tenant, event.clone(), auth)
        .await
        .unwrap();
    // Simulate an event accepted before deployment of the canonical-key ledger.
    sqlx::query(
        "DELETE FROM managed_deliveries WHERE community_id=$1 AND author=$2 AND delivery_key=$3",
    )
    .bind(tenant.community().as_uuid())
    .bind(event.pubkey.to_bytes().as_slice())
    .bind(buzz_core::managed_delivery::delivery_key(&event).unwrap())
    .execute(&historical_pool)
    .await
    .unwrap();
    historical_pool.close().await;
}

#[tokio::test]
#[ignore = "requires Postgres and Redis"]
async fn managed_old_report_still_enforces_signature_route_membership_and_archive() {
    let state = authenticated_http_state().await;
    let host = format!("managed-authority-{}.test", Uuid::new_v4().simple());
    let community = state
        .db
        .ensure_configured_community(&host)
        .await
        .unwrap()
        .id;
    let tenant = TenantContext::resolved(community, &host);
    let author = Keys::generate();
    let owner = Keys::generate();
    let channel = Uuid::new_v4();
    state
        .db
        .create_channel_with_id(
            community,
            channel,
            "private managed recovery",
            ChannelType::Stream,
            ChannelVisibility::Private,
            None,
            author.public_key().as_bytes(),
            None,
        )
        .await
        .unwrap();
    let tracer: Arc<dyn buzz_conformance::Tracer> = Arc::new(VecTracer::default());
    let auth = |keys: &Keys| IngestAuth::Http {
        pubkey: keys.public_key(),
        scopes: vec![Scope::MessagesWrite],
        auth_method: HttpAuthMethod::Nip98,
    };
    let make_event = |parent: Option<String>| {
        let mut tags = vec![
            Tag::parse(["h", &channel.to_string()]).unwrap(),
            Tag::parse(["buzz-delivery", &Uuid::new_v4().to_string()]).unwrap(),
        ];
        if let Some(parent) = parent {
            tags.push(Tag::parse(["e", &parent, "", "reply"]).unwrap());
        }
        EventBuilder::new(Kind::Custom(9), "old managed report")
            .custom_created_at(Timestamp::from(Timestamp::now().as_secs() - 3600))
            .tags(tags)
            .sign_with_keys(&author)
            .unwrap()
    };
    let event = make_event(None);
    let mut invalid = event.clone();
    invalid.content.push_str("tampered");
    assert!(
        ingest_event_inner(&state, &tracer, &tenant, invalid, auth(&author))
            .await
            .is_err()
    );
    assert!(
        ingest_event_inner(&state, &tracer, &tenant, event.clone(), auth(&owner))
            .await
            .is_err()
    );
    assert!(ingest_event_inner(
        &state,
        &tracer,
        &tenant,
        make_event(Some("a".repeat(64))),
        auth(&author)
    )
    .await
    .is_err());
    seed_previously_accepted_event(&state, &tracer, &tenant, event.clone(), auth(&author)).await;
    state
        .db
        .add_member(
            community,
            channel,
            owner.public_key().as_bytes(),
            buzz_db::channel::MemberRole::Owner,
            Some(author.public_key().as_bytes()),
        )
        .await
        .unwrap();
    state
        .db
        .remove_member(
            community,
            channel,
            author.public_key().as_bytes(),
            owner.public_key().as_bytes(),
        )
        .await
        .unwrap();
    state.invalidate_membership(&tenant, channel, author.public_key().as_bytes());
    assert!(
        ingest_event_inner(&state, &tracer, &tenant, event.clone(), auth(&author))
            .await
            .is_err(),
        "even an accepted old event must pass current access before deduplication"
    );
    state
        .db
        .add_member(
            community,
            channel,
            author.public_key().as_bytes(),
            buzz_db::channel::MemberRole::Owner,
            Some(owner.public_key().as_bytes()),
        )
        .await
        .unwrap();
    state.invalidate_membership(&tenant, channel, author.public_key().as_bytes());
    state.db.archive_channel(community, channel).await.unwrap();
    let result = ingest_event_inner(&state, &tracer, &tenant, event, auth(&author)).await;
    assert!(matches!(result, Err(IngestError::Rejected(message)) if message.contains("archived")));
}

#[tokio::test]
#[ignore = "requires Postgres, Redis and built CLI in BUZZ_TEST_BIN_DIR"]
async fn managed_old_absent_journal_recovers_via_actual_cli_and_authenticated_relay() {
    old_journal_recovery(false, false, "original durable report").await;
}

#[tokio::test]
#[ignore = "requires Postgres, Redis and built CLI in BUZZ_TEST_BIN_DIR"]
async fn managed_old_accepted_journal_recovers_via_actual_cli_and_authenticated_relay() {
    old_journal_recovery(true, false, "original durable report").await;
}

#[tokio::test]
#[ignore = "requires Postgres, Redis and built CLI in BUZZ_TEST_BIN_DIR"]
async fn managed_old_lost_candidate_ack_recovers_after_another_restart() {
    old_journal_recovery(false, true, "original durable report").await;
}

#[tokio::test]
#[ignore = "requires Postgres, Redis and built CLI in BUZZ_TEST_BIN_DIR"]
async fn managed_old_maximum_escaped_report_recovers() {
    for content in [
        "\"".repeat(65536),
        "\\".repeat(65536),
        "\u{0001}".repeat(65536),
    ] {
        old_journal_recovery(false, false, &content).await;
    }
}

async fn old_journal_recovery(already_accepted: bool, accepted_candidate: bool, content: &str) {
    use sha2::{Digest, Sha256};
    let state = authenticated_http_state().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = listener.local_addr().unwrap().to_string();
    let relay = format!("http://{host}");
    let community = state
        .db
        .ensure_configured_community(&host)
        .await
        .unwrap()
        .id;
    let tenant = TenantContext::resolved(community, &host);
    let author = Keys::generate();
    let channel = Uuid::new_v4();
    state
        .db
        .create_channel_with_id(
            community,
            channel,
            "live managed recovery",
            ChannelType::Stream,
            ChannelVisibility::Private,
            None,
            author.public_key().as_bytes(),
            None,
        )
        .await
        .unwrap();
    crate::handlers::side_effects::emit_group_discovery_events(&tenant, &state, channel)
        .await
        .unwrap();
    let root = EventBuilder::new(Kind::Custom(9), "original task")
        .tag(Tag::parse(["h", &channel.to_string()]).unwrap())
        .sign_with_keys(&author)
        .unwrap();
    let tracer: Arc<dyn buzz_conformance::Tracer> = Arc::new(VecTracer::default());
    ingest_event_inner(
        &state,
        &tracer,
        &tenant,
        root.clone(),
        IngestAuth::Http {
            pubkey: author.public_key(),
            scopes: vec![Scope::MessagesWrite],
            auth_method: HttpAuthMethod::Nip98,
        },
    )
    .await
    .unwrap();
    let router = crate::router::build_router(state.clone());
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });
    let cli =
        std::path::PathBuf::from(std::env::var("BUZZ_TEST_BIN_DIR").expect("built CLI directory"))
            .join("buzz");
    {
        let dir = tempfile::tempdir().unwrap();
        let key = Uuid::new_v4().to_string();
        let signer = author.public_key().to_hex();
        let event = EventBuilder::new(Kind::Custom(9), content)
            .custom_created_at(Timestamp::from(Timestamp::now().as_secs() - 3600))
            .tags([
                Tag::parse(["h", &channel.to_string()]).unwrap(),
                Tag::parse(["e", &root.id.to_hex(), "", "root"]).unwrap(),
                Tag::parse(["e", &root.id.to_hex(), "", "reply"]).unwrap(),
                Tag::parse(["buzz-delivery", &key]).unwrap(),
            ])
            .sign_with_keys(&author)
            .unwrap();
        if already_accepted {
            seed_previously_accepted_event(
                &state,
                &tracer,
                &tenant,
                event.clone(),
                IngestAuth::Http {
                    pubkey: author.public_key(),
                    scopes: vec![Scope::MessagesWrite],
                    auth_method: HttpAuthMethod::Nip98,
                },
            )
            .await;
        }
        let aged_candidate = EventBuilder::new(event.kind, &event.content)
            .tags(event.tags.clone())
            .allow_self_tagging()
            .custom_created_at(Timestamp::from(event.created_at.as_secs() + 60))
            .sign_with_keys(&author)
            .unwrap();
        if accepted_candidate {
            let historical_pool = sqlx::PgPool::connect(&state.config.database_url)
                .await
                .unwrap();
            let historical_state =
                build_canvas_ingest_state(&state.config.database_url, &historical_pool).await;
            ingest_event_inner(
                &historical_state,
                &tracer,
                &tenant,
                aged_candidate.clone(),
                IngestAuth::Http {
                    pubkey: author.public_key(),
                    scopes: vec![Scope::MessagesWrite],
                    auth_method: HttpAuthMethod::Nip98,
                },
            )
            .await
            .unwrap();
            historical_pool.close().await;
        }
        let id = hex::encode(Sha256::digest(
            serde_json::to_vec(&(&relay, &signer, &key)).unwrap(),
        ));
        let mut journal = serde_json::json!({"version":1,"entries":{id.clone():{"relay":relay,"signer":signer,"key":key,"channel":channel.to_string(),"parent":root.id.to_hex(),"event":event,"attempts":1,"confirmed_at":null,"last_error":null}}});
        // Candidate renewal must preserve the original evidence.
        if accepted_candidate {
            journal["version"] = 2.into();
            journal["entries"][&id]["recovery_candidate"] =
                serde_json::to_value(ReportStamp::from(&aged_candidate)).unwrap();
        }
        std::fs::write(dir.path().join("journal.json"), journal.to_string()).unwrap();
        let mut elected_id = None;
        for _ in 0..2 {
            let output = tokio::process::Command::new(&cli)
                .env_remove("BUZZ_AUTH_TAG")
                .env("BUZZ_PRIVATE_KEY", author.secret_key().to_secret_hex())
                .args([
                    "--relay",
                    &relay,
                    "messages",
                    "deliver",
                    "--delivery-key",
                    &key,
                    "--channel",
                    &channel.to_string(),
                    "--reply-to",
                    &root.id.to_hex(),
                    "--journal-dir",
                    dir.path().to_str().unwrap(),
                    "--content",
                    "changed summary after restart",
                ])
                .output()
                .await
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let receipt: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            let id = receipt["event_id"].as_str().unwrap().to_owned();
            if accepted_candidate {
                assert_eq!(id, aged_candidate.id.to_hex());
            } else if already_accepted {
                assert_eq!(id, event.id.to_hex());
            } else {
                assert_ne!(id, event.id.to_hex());
            }
            if let Some(first) = &elected_id {
                assert_eq!(first, &id);
            } else {
                elected_id = Some(id);
            }
        }
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("journal.json")).unwrap())
                .unwrap();
        assert!(saved["entries"][&id]["confirmed_at"].is_u64());
        assert_eq!(
            serde_json::from_value::<nostr::Event>(saved["entries"][&id]["event"].clone()).unwrap(),
            event
        );
        let stamp: ReportStamp =
            serde_json::from_value(saved["entries"][&id]["recovery_candidate"].clone()).unwrap();
        let renewed = stamp.restore(&event).unwrap();
        assert!(
            Timestamp::now()
                .as_secs()
                .saturating_sub(renewed.created_at.as_secs())
                < 600
        );
        assert_ne!(renewed.id, aged_candidate.id);
        let elected_id = nostr::EventId::from_hex(&elected_id.unwrap()).unwrap();
        let stored = state
            .db
            .get_event_by_id(community, elected_id.as_bytes())
            .await
            .unwrap()
            .unwrap();
        assert!(buzz_core::managed_delivery::same_report(
            &event,
            &stored.event
        ));
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE community_id=$1 AND id=$2")
                .bind(community.as_uuid())
                .bind(elected_id.as_bytes().as_slice())
                .fetch_one(state.db.pool())
                .await
                .unwrap();
        assert_eq!(count, 1);
    }
    shutdown_tx.send(()).unwrap();
    server.await.unwrap();
}

#[tokio::test]
#[ignore = "requires Postgres and Redis"]
async fn managed_recovery_and_original_race_share_one_election() {
    let state = authenticated_http_state().await;
    let host = format!("managed-race-{}.test", Uuid::new_v4().simple());
    let community = state
        .db
        .ensure_configured_community(&host)
        .await
        .unwrap()
        .id;
    let tenant = TenantContext::resolved(community, &host);
    let author = Keys::generate();
    let channel = Uuid::new_v4();
    state
        .db
        .create_channel_with_id(
            community,
            channel,
            "managed race",
            ChannelType::Stream,
            ChannelVisibility::Private,
            None,
            author.public_key().as_bytes(),
            None,
        )
        .await
        .unwrap();
    let tracer: Arc<dyn buzz_conformance::Tracer> = Arc::new(VecTracer::default());
    let auth = || IngestAuth::Http {
        pubkey: author.public_key(),
        scopes: vec![Scope::MessagesWrite],
        auth_method: HttpAuthMethod::Nip98,
    };
    let root = EventBuilder::new(Kind::Custom(9), "root")
        .tag(Tag::parse(["h", &channel.to_string()]).unwrap())
        .sign_with_keys(&author)
        .unwrap();
    ingest_event_inner(&state, &tracer, &tenant, root.clone(), auth())
        .await
        .unwrap();
    for _ in 0..8 {
        let key = Uuid::new_v4().to_string();
        let original = EventBuilder::new(Kind::Custom(9), "same final report")
            .tags([
                Tag::parse(["h", &channel.to_string()]).unwrap(),
                Tag::parse(["e", &root.id.to_hex(), "", "root"]).unwrap(),
                Tag::parse(["e", &root.id.to_hex(), "", "reply"]).unwrap(),
                Tag::parse(["buzz-delivery", &key]).unwrap(),
            ])
            .sign_with_keys(&author)
            .unwrap();
        let wrap = |offset: u64| {
            let candidate = EventBuilder::new(original.kind, &original.content)
                .tags(original.tags.clone())
                .custom_created_at(Timestamp::from(original.created_at.as_secs() + offset))
                .sign_with_keys(&author)
                .unwrap();
            EventBuilder::new(
                Kind::Custom(20003),
                serde_json::json!({"original":original,"candidate":ReportStamp::from(&candidate)})
                    .to_string(),
            )
            .tag(Tag::parse(["h", &channel.to_string()]).unwrap())
            .sign_with_keys(&author)
            .unwrap()
        };
        let (a, b, c) = tokio::join!(
            ingest_event_inner(&state, &tracer, &tenant, original.clone(), auth()),
            ingest_event_inner(&state, &tracer, &tenant, wrap(1), auth()),
            ingest_event_inner(&state, &tracer, &tenant, wrap(2), auth())
        );
        let a = a.unwrap();
        let b = b.unwrap();
        let c = c.unwrap();
        assert_eq!(a.event_id, b.event_id);
        assert_eq!(b.event_id, c.event_id);
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE community_id=$1 AND tags @> $2")
                .bind(community.as_uuid())
                .bind(serde_json::json!([["buzz-delivery", key]]))
                .fetch_one(state.db.pool())
                .await
                .unwrap();
        assert_eq!(count, 1);
        let changed = EventBuilder::new(original.kind, "changed report")
            .tags(original.tags.clone())
            .sign_with_keys(&author)
            .unwrap();
        assert!(
            ingest_event_inner(&state, &tracer, &tenant, changed, auth())
                .await
                .is_err()
        );
    }
    let replies: i32 = sqlx::query_scalar(
        "SELECT reply_count FROM thread_metadata WHERE community_id=$1 AND event_id=$2",
    )
    .bind(community.as_uuid())
    .bind(root.id.as_bytes().as_slice())
    .fetch_one(state.db.pool())
    .await
    .unwrap();
    assert_eq!(replies, 8, "one thread contribution per elected report");
}

#[tokio::test]
#[ignore = "requires Postgres and Redis"]
async fn managed_ws_original_ack_correlates_after_recovery_wins() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    async fn next_json(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> serde_json::Value {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let msg = ws.next().await.unwrap().unwrap();
                if msg.is_text() {
                    return serde_json::from_str(msg.to_text().unwrap()).unwrap();
                }
            }
        })
        .await
        .expect("WS response deadline")
    }
    let state = authenticated_http_state().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = listener.local_addr().unwrap().to_string();
    let ws_url = format!("ws://{host}/");
    let community = state
        .db
        .ensure_configured_community(&host)
        .await
        .unwrap()
        .id;
    let tenant = TenantContext::resolved(community, &host);
    let author = Keys::generate();
    let channel = Uuid::new_v4();
    state
        .db
        .create_channel_with_id(
            community,
            channel,
            "managed ws",
            ChannelType::Stream,
            ChannelVisibility::Private,
            None,
            author.public_key().as_bytes(),
            None,
        )
        .await
        .unwrap();
    let tracer: Arc<dyn buzz_conformance::Tracer> = Arc::new(VecTracer::default());
    let auth = || IngestAuth::Http {
        pubkey: author.public_key(),
        scopes: vec![Scope::MessagesWrite],
        auth_method: HttpAuthMethod::Nip98,
    };
    let root = EventBuilder::new(Kind::Custom(9), "root")
        .tag(Tag::parse(["h", &channel.to_string()]).unwrap())
        .sign_with_keys(&author)
        .unwrap();
    ingest_event_inner(&state, &tracer, &tenant, root.clone(), auth())
        .await
        .unwrap();
    let original = EventBuilder::new(Kind::Custom(9), "first report")
        .tags([
            Tag::parse(["h", &channel.to_string()]).unwrap(),
            Tag::parse(["e", &root.id.to_hex(), "", "root"]).unwrap(),
            Tag::parse(["e", &root.id.to_hex(), "", "reply"]).unwrap(),
            Tag::parse(["buzz-delivery", &Uuid::new_v4().to_string()]).unwrap(),
        ])
        .sign_with_keys(&author)
        .unwrap();
    let candidate = EventBuilder::new(original.kind, &original.content)
        .tags(original.tags.clone())
        .custom_created_at(Timestamp::from(original.created_at.as_secs() + 1))
        .sign_with_keys(&author)
        .unwrap();
    let wrapper = EventBuilder::new(
        Kind::Custom(20003),
        serde_json::json!({"original":original,"candidate":ReportStamp::from(&candidate)})
            .to_string(),
    )
    .tag(Tag::parse(["h", &channel.to_string()]).unwrap())
    .sign_with_keys(&author)
    .unwrap();
    let elected = ingest_event_inner(&state, &tracer, &tenant, wrapper.clone(), auth())
        .await
        .unwrap();
    assert_eq!(elected.event_id, candidate.id.to_hex());
    let router = crate::router::build_router(state);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });
    let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();
    let challenge = loop {
        let message = next_json(&mut ws).await;
        if message[0] == "AUTH" {
            break message[1].as_str().unwrap().to_owned();
        }
    };
    let auth_event = EventBuilder::auth(challenge, nostr::RelayUrl::parse(&ws_url).unwrap())
        .sign_with_keys(&author)
        .unwrap();
    ws.send(Message::Text(
        serde_json::json!(["AUTH", auth_event]).to_string().into(),
    ))
    .await
    .unwrap();
    loop {
        let message = next_json(&mut ws).await;
        if message[0] == "OK" && message[1] == auth_event.id.to_hex() {
            assert_eq!(message[2], true);
            break;
        }
    }
    ws.send(Message::Text(
        serde_json::json!(["EVENT", original]).to_string().into(),
    ))
    .await
    .unwrap();
    let receipt = loop {
        let message = next_json(&mut ws).await;
        if message[0] == "OK" {
            break message;
        }
    };
    assert_eq!(
        receipt[1],
        original.id.to_hex(),
        "WS OK must correlate with the submitted original"
    );
    assert_eq!(receipt[2], true);
    let canonical: nostr::Event = serde_json::from_str(
        receipt[3]
            .as_str()
            .unwrap()
            .strip_prefix(buzz_core::managed_delivery::RECEIPT_PREFIX)
            .unwrap(),
    )
    .unwrap();
    canonical.verify().unwrap();
    assert_eq!(canonical.id, candidate.id);
    ws.send(Message::Text(
        serde_json::json!(["EVENT", wrapper]).to_string().into(),
    ))
    .await
    .unwrap();
    let receipt = loop {
        let message = next_json(&mut ws).await;
        if message[0] == "OK" {
            break message;
        }
    };
    assert_eq!(receipt[1], wrapper.id.to_hex());
    assert_eq!(receipt[2], true);
    let canonical: nostr::Event = serde_json::from_str(
        receipt[3]
            .as_str()
            .unwrap()
            .strip_prefix(buzz_core::managed_delivery::RECEIPT_PREFIX)
            .unwrap(),
    )
    .unwrap();
    canonical.verify().unwrap();
    assert_eq!(canonical.id, candidate.id);
    ws.close(None).await.unwrap();
    shutdown_tx.send(()).unwrap();
    server.await.unwrap();
}

#[tokio::test]
#[ignore = "requires Postgres, Redis and built CLI in BUZZ_TEST_BIN_DIR"]
async fn managed_recovery_mention_history_survives_lost_ack() {
    use sha2::{Digest, Sha256};
    let state = authenticated_http_state().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = listener.local_addr().unwrap().to_string();
    let relay = format!("http://{host}");
    let community = state
        .db
        .ensure_configured_community(&host)
        .await
        .unwrap()
        .id;
    let tenant = TenantContext::resolved(community, &host);
    let author = Keys::generate();
    let recipient = Keys::generate();
    let channel = Uuid::new_v4();
    state
        .db
        .create_channel_with_id(
            community,
            channel,
            "managed mentions",
            ChannelType::Stream,
            ChannelVisibility::Private,
            None,
            author.public_key().as_bytes(),
            None,
        )
        .await
        .unwrap();
    state
        .db
        .add_member(
            community,
            channel,
            recipient.public_key().as_bytes(),
            buzz_db::channel::MemberRole::Member,
            Some(author.public_key().as_bytes()),
        )
        .await
        .unwrap();
    crate::handlers::side_effects::emit_group_discovery_events(&tenant, &state, channel)
        .await
        .unwrap();
    let tracer: Arc<dyn buzz_conformance::Tracer> = Arc::new(VecTracer::default());
    let auth = || IngestAuth::Http {
        pubkey: author.public_key(),
        scopes: vec![Scope::MessagesWrite],
        auth_method: HttpAuthMethod::Nip98,
    };
    let root = EventBuilder::new(Kind::Custom(9), "original task")
        .tag(Tag::parse(["h", &channel.to_string()]).unwrap())
        .sign_with_keys(&author)
        .unwrap();
    ingest_event_inner(&state, &tracer, &tenant, root.clone(), auth())
        .await
        .unwrap();
    let key = Uuid::new_v4().to_string();
    let original = EventBuilder::new(Kind::Custom(9), "callback final report")
        .tags([
            Tag::parse(["h", &channel.to_string()]).unwrap(),
            Tag::parse(["e", &root.id.to_hex(), "", "root"]).unwrap(),
            Tag::parse(["e", &root.id.to_hex(), "", "reply"]).unwrap(),
            Tag::parse(["buzz-delivery", &key]).unwrap(),
            Tag::parse(["p", &recipient.public_key().to_hex()]).unwrap(),
        ])
        .custom_created_at(Timestamp::from(Timestamp::now().as_secs() - 3600))
        .sign_with_keys(&author)
        .unwrap();
    let candidate = EventBuilder::new(original.kind, &original.content)
        .tags(original.tags.clone())
        .sign_with_keys(&author)
        .unwrap();
    let wrapper = EventBuilder::new(
        Kind::Custom(20003),
        serde_json::json!({"original":original,"candidate":ReportStamp::from(&candidate)})
            .to_string(),
    )
    .tag(Tag::parse(["h", &channel.to_string()]).unwrap())
    .sign_with_keys(&author)
    .unwrap();
    // The server commits, but the journal below deliberately has no local ACK.
    let receipt = ingest_event_inner(&state, &tracer, &tenant, wrapper, auth())
        .await
        .unwrap();
    assert_eq!(receipt.event_id, candidate.id.to_hex());
    let mentions = state
        .db
        .query_feed_mentions(
            community,
            recipient.public_key().as_bytes(),
            &[channel],
            None,
            10,
        )
        .await
        .unwrap();
    assert_eq!(
        mentions.len(),
        1,
        "recovered callback must appear in Home Mentions"
    );
    assert_eq!(mentions[0].event.id, candidate.id);
    let mut query = buzz_db::EventQuery::for_community(community);
    query.channel_id = Some(channel);
    query.p_tag_hex = Some(recipient.public_key().to_hex());
    query.kinds = Some(vec![9]);
    assert_eq!(
        state.db.query_events(&query).await.unwrap()[0].event.id,
        candidate.id
    );
    let router = crate::router::build_router(state.clone());
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let signer = author.public_key().to_hex();
    let id = hex::encode(Sha256::digest(
        serde_json::to_vec(&(&relay, &signer, &key)).unwrap(),
    ));
    let journal = serde_json::json!({"version":2,"entries":{id.clone():{"relay":relay,"signer":signer,"key":key,"channel":channel.to_string(),"parent":root.id.to_hex(),"event":original,"recovery_candidate":ReportStamp::from(&candidate),"attempts":1,"confirmed_at":null,"last_error":null}}});
    std::fs::write(dir.path().join("journal.json"), journal.to_string()).unwrap();
    let cli = std::path::PathBuf::from(std::env::var("BUZZ_TEST_BIN_DIR").unwrap()).join("buzz");
    let output = tokio::process::Command::new(cli)
        .env_remove("BUZZ_AUTH_TAG")
        .env("BUZZ_PRIVATE_KEY", author.secret_key().to_secret_hex())
        .args([
            "--relay",
            &relay,
            "messages",
            "deliver",
            "--delivery-key",
            &key,
            "--channel",
            &channel.to_string(),
            "--reply-to",
            &root.id.to_hex(),
            "--journal-dir",
            dir.path().to_str().unwrap(),
            "--content",
            "changed retry summary",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let replay: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(replay["event_id"], candidate.id.to_hex());
    assert_eq!(
        replay["mention_pubkeys"],
        serde_json::json!([recipient.public_key().to_hex()])
    );
    let mentions = state
        .db
        .query_feed_mentions(
            community,
            recipient.public_key().as_bytes(),
            &[channel],
            None,
            10,
        )
        .await
        .unwrap();
    assert_eq!(mentions.len(), 1);
    assert_eq!(mentions[0].event.id, candidate.id);
    let history = state.db.query_events(&query).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].event.id, candidate.id);
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM event_mentions WHERE community_id=$1 AND event_id=$2",
    )
    .bind(community.as_uuid())
    .bind(candidate.id.as_bytes().as_slice())
    .fetch_one(state.db.pool())
    .await
    .unwrap();
    assert_eq!(count, 1, "ACK-loss replay must not duplicate mention rows");
    shutdown_tx.send(()).unwrap();
    server.await.unwrap();
}
