//! Trash and deletion wrappers (L2).
//!
//! All filesystem mutation in wisp goes through these functions so that the
//! dry-run fence and the blacklist check are always in the critical path.

use std::path::Path;

use tracing::instrument;

use crate::errors::{CoreError, CoreResult};
use crate::fs::check_blacklist;

/// Move `path` to the OS trash can.
///
/// When `dry_run` is `true` the operation is logged but the file is not moved.
#[instrument(name = "fs.trash", skip(dry_run), fields(path = %path.display()))]
pub fn send_to_trash(path: &Path, dry_run: bool) -> CoreResult<()> {
    let canonical = path.canonicalize().map_err(CoreError::Io)?;
    check_blacklist(&canonical)?;

    if dry_run {
        tracing::debug!(path = %canonical.display(), "dry-run: would trash");
        return Ok(());
    }

    trash::delete(&canonical).map_err(|e| CoreError::Io(std::io::Error::other(e.to_string())))
}

/// Delete `path` directly, bypassing the trash.
///
/// When `dry_run` is `true` the operation is logged but nothing is deleted.
#[instrument(name = "fs.delete", skip(dry_run), fields(path = %path.display()))]
pub fn delete_direct(path: &Path, dry_run: bool) -> CoreResult<()> {
    let canonical = path.canonicalize().map_err(CoreError::Io)?;
    check_blacklist(&canonical)?;

    if dry_run {
        tracing::debug!(path = %canonical.display(), "dry-run: would delete");
        return Ok(());
    }

    if canonical.is_dir() {
        std::fs::remove_dir_all(&canonical).map_err(CoreError::Io)
    } else {
        std::fs::remove_file(&canonical).map_err(CoreError::Io)
    }
}

/// Compute the total size of a path (recursive for directories).
pub fn path_size(path: &Path) -> u64 {
    if path.is_file() {
        return path.metadata().map(|m| m.len()).unwrap_or(0);
    }
    if path.is_dir() {
        return jwalk::WalkDir::new(path)
            .skip_hidden(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum();
    }
    0
}
