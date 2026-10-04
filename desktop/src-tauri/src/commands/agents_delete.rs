//! Commit a prepared deletion without letting enqueue failure roll back assignment cleanup.
use super::*;
/// The same native coordinator is exercised with injected process/key effects in isolated tests.
pub(crate) fn commit_prepared_agent_delete_with<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    permit: &crate::managed_agents::device_authority::DeletionAuthority,
    operation: &crate::managed_agents::device_home_operations::HomeOperation,
    records: &mut Vec<ManagedAgentRecord>,
    stop: impl FnOnce(&mut Vec<ManagedAgentRecord>) -> Result<(), String>,
    cleanup_key: impl FnOnce(),
) -> Result<(), String> {
    let base = managed_agents_base_dir(app)?;
    let replay = run_managed_agent_deletion(&base, permit.pubkey(), records, |records| {
        stop(records)?;
        crate::managed_agents::device_home_operations::delete::commit_home_delete_snapshot_locked(
            app, operation,
        )
    })?;
    cleanup_key();
    tombstone_managed_agent_pending(app, state, permit, &replay)
}
#[cfg(test)]
#[path = "agents_delete/tests.rs"]
mod tests;
