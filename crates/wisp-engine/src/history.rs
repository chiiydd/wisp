//! Clean-operation history store.
//!
//! Each completed plan is appended to `~/.local/state/wisp/history.jsonl`.

use wisp_core::types::CleanReport;

/// Append a completed `CleanReport` to the history file.
pub fn append(report: &CleanReport) {
    if let Some(path) = history_path()
        && let Ok(line) = serde_json::to_string(report)
    {
        use std::io::Write;
        if let Err(e) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| writeln!(f, "{line}"))
        {
            tracing::warn!(path = %path.display(), error = %e, "failed to append history entry");
        }
    }
}

/// Read history entries, newest-first, up to `limit`.
pub fn read(limit: usize) -> Vec<CleanReport> {
    let path = match history_path() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(path = %path.display(), error = %e, "failed to read history file");
            return Vec::new();
        }
    };

    let mut entries: Vec<CleanReport> = content
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            serde_json::from_str(line)
                .map_err(|e| {
                    tracing::debug!(line = i + 1, error = %e, "skipping malformed history entry");
                })
                .ok()
        })
        .collect();
    entries.reverse();
    entries.truncate(limit);
    entries
}

fn history_path() -> Option<std::path::PathBuf> {
    let proj = directories::ProjectDirs::from("", "", "wisp")?;
    let dir = proj.data_local_dir().join("state");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("history.jsonl"))
}
