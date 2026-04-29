//! Audit log writer.
//!
//! All deletion events are appended as JSONL to
//! `~/.local/state/wisp/audit.log`.

use wisp_core::types::CleanAction;

/// Append a deletion event to the audit log.  Errors are logged at `warn`
/// level but never propagated so that a log-write failure never aborts a clean
/// operation.
pub fn write_entry(action: &CleanAction, bytes_freed: u64, dry_run: bool) {
    if let Some(path) = audit_log_path()
        && let Ok(content) = build_entry(action, bytes_freed, dry_run)
    {
        use std::io::Write;
        if let Err(e) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| writeln!(f, "{content}"))
        {
            tracing::warn!(path = %path.display(), error = %e, "failed to write audit log entry");
        }
    }
}

fn build_entry(
    action: &CleanAction,
    bytes_freed: u64,
    dry_run: bool,
) -> serde_json::Result<String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let value = match action {
        CleanAction::Delete { path, via, .. } => serde_json::json!({
            "ts": now,
            "kind": "delete",
            "path": path.as_str(),
            "via": format!("{via:?}").to_lowercase(),
            "bytes": bytes_freed,
            "dry_run": dry_run,
        }),
        CleanAction::RunExternal { cmd, .. } => serde_json::json!({
            "ts": now,
            "kind": "external",
            "program": cmd.program,
            "args": cmd.args,
            "bytes": bytes_freed,
            "dry_run": dry_run,
        }),
    };

    serde_json::to_string(&value)
}

fn audit_log_path() -> Option<std::path::PathBuf> {
    let proj = directories::ProjectDirs::from("", "", "wisp")?;
    // Use data_local_dir as the XDG state equivalent
    let dir = proj.data_local_dir().join("state");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("audit.log"))
}
