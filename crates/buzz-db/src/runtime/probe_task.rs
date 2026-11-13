//! Owned lifetime for the replica probe and its independent connection pool.

use std::{sync::Arc, time::Duration};

use sqlx::PgPool;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::replica_fence::{self, ReplicaFence};
use crate::{DbError, Result};

#[derive(Debug)]
pub(super) struct ProbeTask {
    cancel: CancellationToken,
    join: JoinHandle<()>,
    pool: PgPool,
    joined: bool,
}

impl ProbeTask {
    pub(super) fn spawn(pool: PgPool, fence: Arc<ReplicaFence>, interval: Duration) -> Self {
        let cancel = CancellationToken::new();
        let stopping = cancel.clone();
        let probe_pool = pool.clone();
        let join = tokio::spawn(async move {
            tokio::select! {
                biased;
                _ = stopping.cancelled() => {},
                _ = replica_fence::run_probe(probe_pool.clone(), fence, interval) => {},
            }
            // Stopping the loop alone leaves SQLx's minimum-connection
            // maintenance alive. Close after the cancelled future releases its
            // in-flight transaction and checkout.
            probe_pool.close().await;
        });
        Self {
            cancel,
            join,
            pool,
            joined: false,
        }
    }

    pub(super) async fn shutdown(&mut self) -> Result<()> {
        self.cancel.cancel();
        let outcome = if self.joined {
            Ok(())
        } else {
            let outcome = (&mut self.join).await;
            self.joined = true;
            outcome
        };
        // Also close if the worker panicked before its normal cleanup.
        self.pool.close().await;
        outcome.map_err(|error| DbError::InvalidData(format!("replica probe task failed: {error}")))
    }
}

impl Drop for ProbeTask {
    fn drop(&mut self) {
        self.cancel.cancel();
        if !self.joined {
            self.join.abort();
        }
    }
}

#[cfg(test)]
mod postgres_tests;
