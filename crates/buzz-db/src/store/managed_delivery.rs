//! Atomic canonical report selection on the guarded production writer.
use buzz_core::{
    managed_delivery::{delivery_key, same_report},
    CommunityId, StoredEvent,
};
use chrono::{DateTime, Utc};
use nostr::Event;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    error::{DbError, Result},
    store::event::{
        insert_event_with_thread_metadata_tx, row_to_stored_event, ThreadMetadataParams,
    },
    Db,
};

/// Elect a report, adopting historical acceptance before inserting a fresh candidate.
pub async fn insert(
    pool: &PgPool,
    community: CommunityId,
    candidate: &Event,
    channel: Uuid,
    metadata: Option<ThreadMetadataParams<'_>>,
    original: Option<&Event>,
) -> Result<(StoredEvent, bool)> {
    let key = delivery_key(candidate)
        .ok_or_else(|| DbError::InvalidData("missing managed delivery key".into()))?;
    let original = original.unwrap_or(candidate);
    if !same_report(original, candidate) {
        return Err(DbError::InvalidData(
            "managed recovery changed the report".into(),
        ));
    }
    let author = candidate.pubkey.to_bytes();
    let mut hash = Sha256::new();
    hash.update(b"buzz-managed-delivery-v1");
    hash.update(community.as_uuid().as_bytes());
    hash.update(author);
    hash.update(key.as_bytes());
    let mut lock_bytes = [0u8; 8];
    lock_bytes.copy_from_slice(&hash.finalize()[..8]);
    let connection = crate::observability::acquire_writer(
        pool,
        crate::observability::WriterOperation::EventWrite,
    )
    .await?;
    let mut tx = sqlx::Transaction::begin(connection, None).await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(i64::from_be_bytes(lock_bytes))
        .execute(&mut *tx)
        .await?;
    let bound = sqlx::query("SELECT event, received_at, channel_id FROM managed_deliveries WHERE community_id=$1 AND author=$2 AND delivery_key=$3")
        .bind(community.as_uuid()).bind(author.as_slice()).bind(key).fetch_optional(&mut *tx).await?;
    if let Some(row) = bound {
        let chosen: Event = serde_json::from_value(row.try_get("event")?)?;
        let bound_channel: Uuid = row.try_get("channel_id")?;
        if bound_channel != channel || !same_report(&chosen, candidate) {
            return Err(DbError::ManagedDeliveryConflict(
                "delivery key is bound to a different report or route".into(),
            ));
        }
        let stored =
            StoredEvent::with_received_at(chosen, row.try_get("received_at")?, Some(channel), true);
        tx.commit().await?;
        return Ok((stored, false));
    }
    // Includes soft-deleted rows: previous acceptance must never resurrect a report.
    let historical = sqlx::query("SELECT id, pubkey, created_at, kind, tags, content, sig, received_at, channel_id FROM events WHERE community_id=$1 AND id=$2")
        .bind(community.as_uuid()).bind(original.id.as_bytes().as_slice()).fetch_optional(&mut *tx).await?;
    let result = if let Some(row) = historical {
        let stored = row_to_stored_event(row)?
            .ok_or_else(|| DbError::InvalidData("invalid historical report".into()))?;
        if stored.channel_id != Some(channel) || !same_report(&stored.event, candidate) {
            return Err(DbError::InvalidData(
                "historical report does not match recovery".into(),
            ));
        }
        (stored, false)
    } else {
        insert_event_with_thread_metadata_tx(&mut tx, community, candidate, Some(channel), metadata)
            .await?
    };
    if result.1 {
        crate::runtime::insert_mentions_in_transaction(
            &mut tx,
            community,
            &result.0.event,
            Some(channel),
        )
        .await?;
    }
    let received_at: DateTime<Utc> = result.0.received_at;
    sqlx::query("INSERT INTO managed_deliveries (community_id, author, delivery_key, channel_id, event, received_at) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(community.as_uuid()).bind(author.as_slice()).bind(key).bind(channel)
        .bind(serde_json::to_value(&result.0.event)?).bind(received_at).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(result)
}

impl Db {
    /// Recover an expired report using one guarded transaction and a shared completion lock.
    pub async fn recover_managed_delivery(
        &self,
        community: CommunityId,
        candidate: &Event,
        channel: Uuid,
        metadata: Option<ThreadMetadataParams<'_>>,
        original: &Event,
    ) -> Result<(StoredEvent, bool)> {
        insert(
            self.pool(),
            community,
            candidate,
            channel,
            metadata,
            Some(original),
        )
        .await
    }
}
