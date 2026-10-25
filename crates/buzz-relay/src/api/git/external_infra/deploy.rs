//! Accepted receive-pack invokes its first real hook after the HTTP fence.
mod external_infra {
    use super::super::*;
    use std::{sync::Arc, time::Duration};
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    #[ignore = "requires isolated Postgres, Redis and MinIO"]
    async fn deployment_long_push_first_hook_after_fence_commits_cas_and_refs() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let f = Fixture::with_deployment(Some(address)).await;
        let before = f.snapshot().await;
        let source = f.scratch.path().join("source");
        let old = git(&source, &["rev-parse", "legacy"])
            .await
            .trim()
            .to_owned();
        git(
            &source,
            &["commit", "--allow-empty", "-m", "accepted before drain"],
        )
        .await;
        let new = git(&source, &["rev-parse", "HEAD"]).await.trim().to_owned();
        let mut command = tokio::process::Command::new("git");
        command
            .current_dir(&source)
            .args(["pack-objects", "--stdout", "--revs"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped());
        crate::api::git::transport::harden_git_env(&mut command);
        let mut pack = command.spawn().unwrap();
        pack.stdin
            .take()
            .unwrap()
            .write_all(format!("{new}\n^{old}\n").as_bytes())
            .await
            .unwrap();
        let pack = pack.wait_with_output().await.unwrap();
        assert!(pack.status.success());
        let line = format!("{old} {new} refs/heads/legacy\0report-status\n");
        let mut body = format!("{:04x}{line}0000", line.len() + 4).into_bytes();
        body.extend(pack.stdout);
        let app = crate::router::build_router(f.state.clone());
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        let path = format!(
            "/git/{}/{}/git-receive-pack",
            f.owner.public_key().to_hex(),
            f.repo
        );
        let authorization = token(
            &f.owner,
            "POST",
            &format!(
                "http://{}/git/{}/{}",
                f.tenant.host(),
                f.owner.public_key().to_hex(),
                f.repo
            ),
            None,
        );
        let release = Arc::new(tokio::sync::Notify::new());
        let delayed = release.clone();
        let host = f.tenant.host().to_owned();
        let push = tokio::spawn(async move {
            let stream = futures_util::stream::once(async move {
                delayed.notified().await;
                Ok::<_, std::io::Error>(body)
            });
            reqwest::Client::new()
                .post(format!("http://{address}{path}"))
                .header("host", host)
                .header("authorization", authorization)
                .header("content-type", "application/x-git-receive-pack-request")
                .body(reqwest::Body::wrap_stream(stream))
                .send()
                .await
                .unwrap()
        });
        tokio::time::timeout(Duration::from_secs(10), async {
            while f.state.deploy_drain.snapshot().git_inflight != 1 && !push.is_finished() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        if f.state.deploy_drain.snapshot().git_inflight != 1 {
            let response = push.await.unwrap();
            panic!(
                "push rejected: {} {}",
                response.status(),
                response.text().await.unwrap()
            );
        }
        assert_eq!(f.state.deploy_drain.snapshot().callback_inflight, 0);
        let op = uuid::Uuid::new_v4();
        let boot = f.state.deploy_drain.begin(op).unwrap().boot_id;
        f.state
            .deploy_drain
            .fence_http(op, boot, "direct-push-edge-retired")
            .unwrap();
        tokio::time::sleep(Duration::from_secs(36)).await;
        assert_eq!(f.state.deploy_drain.snapshot().git_inflight, 1);
        assert_eq!(f.snapshot().await.digest, before.digest);
        assert!(!f.state.deploy_drain.snapshot().safe_to_stop);
        // No command or pack byte has been sent: the installed pre-receive hook
        // can only execute for the first time after the admission fence above.
        release.notify_one();
        let response = tokio::time::timeout(Duration::from_secs(20), push)
            .await
            .unwrap()
            .unwrap();
        let status = response.status();
        let bytes = response.bytes().await.unwrap();
        let report = String::from_utf8_lossy(&bytes);
        assert_eq!(status, StatusCode::OK, "{report}");
        assert!(report.contains("ok refs/heads/legacy"), "{report}");
        let after = f.snapshot().await;
        assert_ne!(after.digest, before.digest);
        assert_eq!(after.manifest.refs["refs/heads/legacy"].to_string(), new);
        let mut query = buzz_db::event::EventQuery::for_community(f.tenant.community());
        query.kinds = Some(vec![30618]);
        query.d_tag = Some(f.repo.clone());
        assert_eq!(f.state.db.query_events(&query).await.unwrap().len(), 1);
        tokio::time::timeout(Duration::from_secs(5), async {
            while f.state.deploy_drain.snapshot().background_inflight != 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let snapshot = f.state.deploy_drain.snapshot();
        assert_eq!(snapshot.git_inflight, 0);
        assert_eq!(snapshot.callback_inflight, 0);
        assert!(
            !snapshot
                .blocking_workers
                .iter()
                .any(|name| name.contains("unresolved")),
            "{snapshot:?}"
        );
        server.abort();
        let _ = server.await;
    }
}
