//! Public scope metadata for hashed retention files, including premetadata retry discovery.
use super::*;
/// Record the original non-secret owner and normalized relay in this exact database.
pub(crate) fn remember_retention_scope(
    conn: &Connection,
    relay: &str,
    owner: &str,
) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS retention_scope (
            singleton INTEGER PRIMARY KEY CHECK(singleton=1),
            relay_url TEXT NOT NULL,
            owner_pubkey TEXT NOT NULL
        );",
    )
    .map_err(|e| format!("retention scope schema: {e}"))?;
    let current: Option<(String, String)> = conn
        .query_row(
            "SELECT relay_url,owner_pubkey FROM retention_scope WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let pair = (
        normalized_relay_scope(relay).to_string(),
        owner.trim().to_ascii_lowercase(),
    );
    if current.as_ref().is_some_and(|p| p != &pair) {
        return Err("retention scope metadata mismatch".into());
    }
    conn.execute(
        "INSERT OR IGNORE INTO retention_scope VALUES (1,?1,?2)",
        params![pair.0, pair.1],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
/// Read metadata without creating or changing a historical database.
pub(crate) fn read_retention_scope(path: &Path) -> Result<Option<(String, String)>, String> {
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("retention scope read: {e}"))?;
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='retention_scope')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !exists {
        return Ok(None);
    }
    conn.query_row(
        "SELECT relay_url,owner_pubkey FROM retention_scope WHERE singleton=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
    .map_err(|e| e.to_string())
}
/// Basename plus recoverable public scope metadata, or an unresolved historical scope.
pub(crate) type KnownRetentionScope = (String, Option<(String, String)>);

/// Enumerate every known DB, preserving unresolved historical filenames for durable retry.
pub(crate) fn known_retention_scopes(base: &Path) -> Result<Vec<KnownRetentionScope>, String> {
    let dir = base.join("retention");
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.to_string()),
    };
    let mut result = vec![];
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("db") {
            continue;
        }
        let file = entry
            .file_name()
            .to_str()
            .ok_or_else(|| "retention scope filename not UTF-8".to_string())?
            .to_string();
        let scope = read_retention_scope(&path)?;
        if let Some((relay, owner)) = &scope {
            if scoped_retention_db_path(base, relay, owner) != path {
                return Err("retention scope filename mismatch".into());
            }
        }
        result.push((file, scope));
    }
    result.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(result)
}
