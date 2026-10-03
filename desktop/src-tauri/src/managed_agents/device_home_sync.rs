//! Backend-owned owner/relay/generation hydration barrier.
use super::definition_home::EvidenceReadiness;
use crate::app_state::AppState;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::Ordering,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyncScope {
    pub owner_pubkey: String,
    pub relay_url: String,
    pub workspace_generation: u64,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeviceHomeSyncSession {
    pub token: String,
    pub owner_pubkey: String,
    pub relay_url: String,
    pub workspace_generation: u64,
}
/// Authenticated history IDs covered by a successful exhaustive application.
/// Includes superseded instance heads so buffered history echoes stay skipped.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeviceHomeHistory {
    pub covered_event_ids: Vec<String>,
}
struct ActiveSession {
    session: DeviceHomeSyncSession,
    readiness: EvidenceReadiness,
    hydrated: bool,
    hydrating: bool,
    in_flight: usize,
}
#[derive(Default)]
pub(crate) struct DeviceHomeSyncState {
    active: Option<ActiveSession>,
}

/// Capture identity, normalized relay and workspace epoch before asynchronous work.
pub(crate) fn capture_scope(state: &AppState) -> Result<SyncScope, String> {
    Ok(SyncScope {
        owner_pubkey: state.signing_keys()?.public_key().to_hex(),
        relay_url: crate::relay::relay_ws_url_with_override(state)
            .trim()
            .trim_end_matches('/')
            .to_string(),
        workspace_generation: state.workspace_apply_generation.load(Ordering::Acquire),
    })
}
impl DeviceHomeSyncSession {
    fn scope(&self) -> SyncScope {
        SyncScope {
            owner_pubkey: self.owner_pubkey.clone(),
            relay_url: self.relay_url.clone(),
            workspace_generation: self.workspace_generation,
        }
    }
}
fn with_sync<T>(
    state: &AppState,
    action: impl FnOnce(&mut DeviceHomeSyncState, &SyncScope) -> Result<T, String>,
) -> Result<T, String> {
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let scope = capture_scope(state)?;
    let mut sync = state.device_home_sync.lock().map_err(|e| e.to_string())?;
    action(&mut sync, &scope)
}
fn active<'a>(
    sync: &'a mut DeviceHomeSyncState,
    scope: &SyncScope,
    token: &str,
) -> Result<&'a mut ActiveSession, String> {
    sync.active
        .as_mut()
        .filter(|s| s.session.token == token && s.session.scope() == *scope)
        .ok_or_else(|| "device_home_sync_stale_session".into())
}
/// Start a new backend session; earlier subscription tokens immediately lose authority.
pub(crate) fn begin_session(state: &AppState) -> Result<DeviceHomeSyncSession, String> {
    with_sync(state, |sync, scope| {
        let session = DeviceHomeSyncSession {
            token: uuid::Uuid::new_v4().to_string(),
            owner_pubkey: scope.owner_pubkey.clone(),
            relay_url: scope.relay_url.clone(),
            workspace_generation: scope.workspace_generation,
        };
        sync.active = Some(ActiveSession {
            session: session.clone(),
            readiness: EvidenceReadiness::Pending,
            hydrated: false,
            hydrating: false,
            in_flight: 0,
        });
        Ok(session)
    })
}
/// Complete only exhaustive history and successfully drained backend applications.
pub(crate) fn finish_session(state: &AppState, token: &str) -> Result<(), String> {
    with_sync(state, |sync, scope| {
        let s = active(sync, scope, token)?;
        if s.readiness == EvidenceReadiness::Failed {
            return Err("device_home_sync_failed".into());
        }
        if !s.hydrated || s.hydrating || s.in_flight != 0 {
            return Err("device_home_sync_pending".into());
        }
        s.readiness = EvidenceReadiness::Ready;
        Ok(())
    })
}
/// Invalidate only the specified session, never a replacement subscription.
pub(crate) fn invalidate_session(state: &AppState, token: &str) -> Result<(), String> {
    with_sync(state, |sync, scope| {
        active(sync, scope, token)?;
        sync.active = None;
        Ok(())
    })
}
/// Readiness lookup for a caller already holding the managed store lock.
pub(crate) fn readiness_locked(
    state: &AppState,
    scope: &SyncScope,
) -> Result<EvidenceReadiness, String> {
    let sync = state.device_home_sync.lock().map_err(|e| e.to_string())?;
    Ok(sync
        .active
        .as_ref()
        .filter(|s| s.session.scope() == *scope)
        .map_or(EvidenceReadiness::Pending, |s| s.readiness))
}
/// Workspace initialization invalidates the old subscription before applying state.
pub(crate) fn reset(state: &AppState) -> Result<(), String> {
    with_sync(state, |sync, _| {
        sync.active = None;
        Ok(())
    })
}
fn fail(state: &AppState, token: &str) {
    let _ = with_sync(state, |sync, scope| {
        active(sync, scope, token)?.readiness = EvidenceReadiness::Failed;
        Ok(())
    });
}

/// An accepted application participates in the backend completion barrier.
/// Cancellation or dropping an unfinished application latches failure.
pub(crate) struct ApplyLease<'a> {
    state: &'a AppState,
    token: Option<String>,
    completed: bool,
}
impl ApplyLease<'_> {
    pub(crate) fn complete(mut self, result: &Result<(), String>) -> Result<(), String> {
        self.completed = true;
        if let Some(token) = &self.token {
            with_sync(self.state, |sync, scope| {
                let s = active(sync, scope, token)?;
                s.in_flight = s.in_flight.saturating_sub(1);
                if result.is_err() {
                    s.readiness = EvidenceReadiness::Failed;
                }
                Ok(())
            })?;
        }
        Ok(())
    }
}
impl Drop for ApplyLease<'_> {
    fn drop(&mut self) {
        if !self.completed {
            if let Some(token) = &self.token {
                fail(self.state, token);
            }
        }
    }
}
/// Token-bearing live applications must belong to the current backend scope.
/// Legacy applications also join an active barrier but cannot create readiness.
pub(crate) fn begin_apply<'a>(
    state: &'a AppState,
    token: Option<&str>,
) -> Result<ApplyLease<'a>, String> {
    let token = with_sync(state, |sync, scope| {
        let s = match token {
            Some(token) => Some(active(sync, scope, token)?),
            None => sync.active.as_mut().filter(|s| s.session.scope() == *scope),
        };
        if let Some(s) = s {
            s.in_flight += 1;
            Ok(Some(s.session.token.clone()))
        } else {
            Ok(None)
        }
    })?;
    Ok(ApplyLease {
        state,
        token,
        completed: false,
    })
}

const PAGE_LIMIT: usize = 500;
const KINDS: [u16; 5] = [30175, 30176, 30177, 30178, 5];
fn d_tag(event: &nostr::Event) -> Option<&str> {
    event.tags.iter().find_map(|t| {
        let t = t.as_slice();
        (t.first().map(String::as_str) == Some("d"))
            .then(|| t.get(1).map(String::as_str))
            .flatten()
    })
}
fn ordered_heads(events: Vec<nostr::Event>) -> Vec<nostr::Event> {
    let mut heads = HashMap::new();
    for event in &events {
        if event.kind.as_u16() != 30177 {
            continue;
        }
        if let Some(tag) = d_tag(event) {
            let key = (event.pubkey, tag.to_ascii_lowercase());
            let winner = heads.get(&key).is_none_or(|old: &&nostr::Event| {
                event.created_at > old.created_at
                    || (event.created_at == old.created_at && event.id < old.id)
            });
            if winner {
                heads.insert(key, event);
            }
        }
    }
    let selected: HashSet<_> = heads.values().map(|e| e.id).collect();
    let events: Vec<_> = events
        .into_iter()
        .filter(|e| e.kind.as_u16() != 30177 || d_tag(e).is_none() || selected.contains(&e.id))
        .collect();
    let (mut constituents, catalog): (Vec<_>, Vec<_>) =
        events.into_iter().partition(|e| e.kind.as_u16() != 30178);
    constituents.extend(catalog);
    constituents
}
/// Exhaustively fetch, dedupe and sequentially reconcile history. The command
/// supplies captured-key transport and the production inbound dispatcher.
/// No standard mutex is held during a fetch or reconciliation await.
pub(crate) async fn hydrate_history<F, FF, A, AF>(
    state: &AppState,
    token: &str,
    mut fetch: F,
    mut apply: A,
) -> Result<DeviceHomeHistory, String>
where
    F: FnMut(serde_json::Value) -> FF,
    FF: std::future::Future<Output = Result<Vec<nostr::Event>, String>>,
    A: FnMut(nostr::Event) -> AF,
    AF: std::future::Future<Output = Result<(), String>>,
{
    let scope = with_sync(state, |sync, scope| {
        let s = active(sync, scope, token)?;
        if s.hydrating || s.hydrated || s.readiness == EvidenceReadiness::Failed {
            return Err("device_home_sync_session_not_pending".into());
        }
        s.hydrating = true;
        Ok(scope.clone())
    })?;
    let result=async {
        let mut until=None;let mut seen=HashSet::new();let mut collected=Vec::new();
        loop {
            with_sync(state,|sync,current|{active(sync,current,token)?;Ok(())})?;
            let mut filter=serde_json::json!({"kinds":KINDS,"authors":[scope.owner_pubkey],"limit":PAGE_LIMIT});
            if let Some(second)=until {filter["until"]=serde_json::json!(second);}
            let page=fetch(filter).await?;
            with_sync(state,|sync,current|{active(sync,current,token)?;Ok(())})?;
            let size=page.len();let mut oldest=u64::MAX;let mut added=0;
            for event in page {
                if event.pubkey.to_hex()!=scope.owner_pubkey || !KINDS.contains(&event.kind.as_u16()) {return Err("device_home_sync_unexpected_history_event".into());}
                event.verify().map_err(|e|format!("device_home_sync_invalid_event: {e}"))?;
                oldest=oldest.min(event.created_at.as_secs());
                if seen.insert(event.id){collected.push(event);added+=1;}
            }
            if size<PAGE_LIMIT {break;}
            if size>PAGE_LIMIT || until.is_some_and(|u|oldest>=u) || added==0 {return Err("device_home_sync_dense_history_boundary".into());}
            until=Some(oldest);
        }
        for event in ordered_heads(collected) {
            with_sync(state,|sync,current|{active(sync,current,token)?;Ok(())})?;
            let lease=begin_apply(state,Some(token))?;
            let result=apply(event).await;
            lease.complete(&result)?;result?;
        }
        with_sync(state,|sync,current|{let s=active(sync,current,token)?;if s.readiness==EvidenceReadiness::Failed {return Err("device_home_sync_failed".into());}s.hydrated=true;s.hydrating=false;Ok(())})?;
        let mut covered_event_ids:Vec<_>=seen.into_iter().map(|id|id.to_hex()).collect();covered_event_ids.sort();
        Ok(DeviceHomeHistory{covered_event_ids})
    }.await;
    if result.is_err() {
        fail(state, token);
    }
    result
}
#[cfg(test)]
mod tests;
