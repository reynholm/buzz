//! Read-only Git subprocess ownership through response completion and reaping.

use super::transport::TimedByteStream;

pub(super) fn spawn_buffered_read<R: Send + 'static>(
    mut child: Child,
    resources: R,
    work: WorkGuard,
    timeout: Duration,
) -> tokio::task::JoinHandle<io::Result<(std::process::ExitStatus, R)>> {
    tokio::spawn(async move {
        match tokio::time::timeout(timeout, child.wait()).await {
            Ok(Ok(status)) => {
                work.complete();
                Ok((status, resources))
            }
            Ok(Err(error)) => Err(error), // Unresolved reaping retains the fault.
            Err(_) => {
                child.kill().await?; // kill awaits actual reaping.
                drop(resources);
                work.complete();
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "git ref advertisement timed out",
                ))
            }
        }
    })
}
use crate::deploy_drain::WorkGuard;
use axum::body::Body;
use futures_util::{Stream, StreamExt};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::process::Child;
use tokio_util::{io::ReaderStream, sync::CancellationToken};

pub(super) struct ManagedRead {
    inner: TimedByteStream<ReaderStream<tokio::process::ChildStdout>>,
    finished: CancellationToken,
}

impl ManagedRead {
    pub(super) fn spawn<R: Send + 'static>(
        mut child: Child,
        body: Body,
        resources: R,
        work: WorkGuard,
        timeout: Duration,
    ) -> io::Result<Self> {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("git stdout is not piped"))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("git stdin is not piped"))?;
        let finished = CancellationToken::new();
        let cleanup = finished.clone();
        // This task owns both the process and its stdin future. Cancelling a
        // read is safe, but completion requires dropping stdin and waitpid.
        tokio::spawn(async move {
            let result = work
                .scope(async {
                    let pump = async move {
                        let mut stream = body.into_data_stream();
                        while let Some(chunk) = stream.next().await {
                            match chunk {
                                Ok(bytes) => {
                                    if tokio::io::AsyncWriteExt::write_all(&mut stdin, &bytes)
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                                Err(error) => {
                                    tracing::warn!(%error, "git read request body failed");
                                    break;
                                }
                            }
                        }
                    };
                    tokio::pin!(pump);
                    let deadline = tokio::time::sleep(timeout);
                    tokio::pin!(deadline);
                    let mut input_done = false;
                    loop {
                        tokio::select! {
                            _ = &mut pump, if !input_done => input_done = true,
                            status = child.wait() => break status,
                            _ = cleanup.cancelled() => {
                                child.start_kill()?;
                                break child.wait().await;
                            }
                            _ = &mut deadline => {
                                cleanup.cancel();
                                child.start_kill()?;
                                break child.wait().await;
                            }
                        }
                    }
                    // The pump (including its body and stdin) is dropped here.
                })
                .await;
            // Child exit can precede consumption of buffered stdout. Retain
            // the concurrency permit and workspace until both lifetimes end.
            cleanup.cancelled().await;
            drop(resources);
            match result {
                Ok(_) => work.complete(),
                Err(error) => tracing::error!(%error, "git read process cleanup failed"),
            }
        });
        Ok(Self {
            inner: TimedByteStream::new(ReaderStream::new(stdout), timeout),
            finished,
        })
    }
}

impl Stream for ManagedRead {
    type Item = io::Result<bytes::Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let result = Pin::new(&mut self.inner).poll_next(cx);
        if matches!(result, Poll::Ready(None | Some(Err(_)))) {
            self.finished.cancel();
        }
        result
    }
}

impl Drop for ManagedRead {
    fn drop(&mut self) {
        self.finished.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy_drain::{DrainController, WorkKind};
    use axum::body::Body;
    use futures_util::StreamExt;
    use std::{sync::Arc, time::Duration};
    use tokio::process::Command;

    async fn git_child() -> (tempfile::TempDir, tokio::process::Child) {
        let repo = tempfile::tempdir().unwrap();
        assert!(Command::new("git")
            .args(["init", "--bare", "--quiet"])
            .arg(repo.path())
            .status()
            .await
            .unwrap()
            .success());
        let child = Command::new("git")
            .args(["upload-pack", "--stateless-rpc"])
            .arg(repo.path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        (repo, child)
    }

    fn fence(drain: &DrainController) {
        let op = uuid::Uuid::new_v4();
        let boot = drain.begin(op).unwrap().boot_id;
        drain.fence_http(op, boot, "test-edge-retired").unwrap();
    }

    async fn wait_safe(drain: &DrainController) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !drain.snapshot().safe_to_stop {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("read subprocess must be reaped and its stdin joined");
    }

    #[tokio::test]
    async fn fetch_stdin_inherits_parent_after_http_fence() {
        let (repo, child) = git_child().await;
        let path = repo.path().to_owned();
        let drain = DrainController::new();
        let parent = drain.try_admit_root(WorkKind::Http).unwrap();
        let work = drain.admit_child(&parent, WorkKind::Git).unwrap();
        let (entered, ready) = tokio::sync::oneshot::channel();
        let release = Arc::new(tokio::sync::Notify::new());
        let body = Body::from_stream(futures_util::stream::once({
            let drain = drain.clone();
            let release = release.clone();
            async move {
                entered.send(()).unwrap();
                release.notified().await;
                drain
                    .admit_current_child(WorkKind::Background)
                    .unwrap()
                    .complete();
                Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"0000"))
            }
        }));
        let mut stream =
            ManagedRead::spawn(child, body, repo, work, Duration::from_secs(5)).unwrap();
        ready.await.unwrap();
        fence(&drain);
        parent.complete();
        assert_eq!(drain.snapshot().git_inflight, 1);
        assert!(!drain.snapshot().safe_to_stop);
        release.notify_one();
        while let Some(chunk) = stream.next().await {
            chunk.unwrap();
        }
        wait_safe(&drain).await;
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn dropped_fetch_retains_resources_until_stdin_is_dropped_and_child_reaped() {
        let (repo, child) = git_child().await;
        let path = repo.path().to_owned();
        let pid = child.id().unwrap();
        let drain = DrainController::new();
        let parent = drain.try_admit_root(WorkKind::Http).unwrap();
        let work = drain.admit_child(&parent, WorkKind::Git).unwrap();
        let (alive, gone) = tokio::sync::oneshot::channel::<()>();
        let body = Body::from_stream(futures_util::stream::once(async move {
            let _alive = alive;
            std::future::pending::<Result<bytes::Bytes, std::io::Error>>().await
        }));
        let permits = Arc::new(tokio::sync::Semaphore::new(1));
        let permit = permits.clone().acquire_owned().await.unwrap();
        let stream =
            ManagedRead::spawn(child, body, (repo, permit), work, Duration::from_secs(5)).unwrap();
        fence(&drain);
        parent.complete();
        drop(stream);
        // On the single-thread runtime the cleanup has not run yet.
        assert!(!drain.snapshot().safe_to_stop);
        assert!(path.exists());
        assert_eq!(permits.available_permits(), 0);
        wait_safe(&drain).await;
        assert!(gone.await.is_err());
        assert!(!path.exists());
        assert_eq!(permits.available_permits(), 1);
        assert!(!Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .unwrap()
            .success());
    }

    #[tokio::test]
    async fn unpolled_fetch_deadline_reaps_process_and_releases_input() {
        let (repo, child) = git_child().await;
        let drain = DrainController::new();
        let parent = drain.try_admit_root(WorkKind::Http).unwrap();
        let work = drain.admit_child(&parent, WorkKind::Git).unwrap();
        let body = Body::from_stream(futures_util::stream::pending::<
            Result<bytes::Bytes, std::io::Error>,
        >());
        let mut stream =
            ManagedRead::spawn(child, body, repo, work, Duration::from_millis(20)).unwrap();
        fence(&drain);
        parent.complete();
        wait_safe(&drain).await;
        assert_eq!(
            stream.next().await.unwrap().unwrap_err().kind(),
            std::io::ErrorKind::TimedOut
        );
    }

    #[tokio::test]
    async fn cancelled_buffered_caller_does_not_drop_running_child_ownership() {
        let (repo, mut child) = git_child().await;
        // Hold stdin open so the real Git process waits until the deadline.
        let stdin = child.stdin.take().unwrap();
        let path = repo.path().to_owned();
        let drain = DrainController::new();
        let parent = drain.try_admit_root(WorkKind::Http).unwrap();
        let work = drain.admit_child(&parent, WorkKind::Git).unwrap();
        let task = spawn_buffered_read(child, (repo, stdin), work, Duration::from_millis(30));
        fence(&drain);
        parent.complete();
        drop(task);
        assert!(!drain.snapshot().safe_to_stop);
        assert!(path.exists());
        wait_safe(&drain).await;
        assert!(!path.exists());
    }
}
