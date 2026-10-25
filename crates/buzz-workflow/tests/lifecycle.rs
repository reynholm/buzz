use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use buzz_workflow::lifecycle::{RunGuard, RunLease};

tokio::task_local! {
    static RUN_CONTEXT: usize;
}

struct Lease {
    completed: Arc<AtomicUsize>,
    dropped: Arc<AtomicUsize>,
    known: bool,
}

impl RunLease for Lease {
    fn with_context(&self, callback: &mut dyn FnMut()) {
        RUN_CONTEXT.sync_scope(42, callback);
    }

    fn complete(mut self: Box<Self>) {
        self.known = true;
        self.completed.fetch_add(1, Ordering::SeqCst);
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if !self.known {
            self.dropped.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[tokio::test]
async fn run_context_covers_each_poll_and_completion_is_explicit() {
    let completed = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicUsize::new(0));
    let guard = RunGuard::new(Box::new(Lease {
        completed: completed.clone(),
        dropped: dropped.clone(),
        known: false,
    }));
    guard
        .scope(async {
            assert_eq!(RUN_CONTEXT.get(), 42);
            tokio::task::yield_now().await;
            assert_eq!(RUN_CONTEXT.get(), 42);
        })
        .await;
    assert!(RUN_CONTEXT.try_with(|_| ()).is_err());
    assert_eq!(completed.load(Ordering::SeqCst), 0);
    guard.complete();
    assert_eq!(completed.load(Ordering::SeqCst), 1);
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancelled_run_never_acknowledges_completion() {
    let completed = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicUsize::new(0));
    let (started, ready) = tokio::sync::oneshot::channel();
    let guard = RunGuard::new(Box::new(Lease {
        completed: completed.clone(),
        dropped: dropped.clone(),
        known: false,
    }));
    let task = tokio::spawn(async move {
        guard
            .scope(async {
                started.send(()).unwrap();
                std::future::pending::<()>().await;
            })
            .await;
        guard.complete();
    });
    ready.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(completed.load(Ordering::SeqCst), 0);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}
