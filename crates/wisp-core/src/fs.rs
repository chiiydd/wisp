//! L2 file-system abstractions: path safety, blacklists, and dry-run fencing.
//!
//! **Nothing above L2 should call `std::fs` or `tokio::fs` directly.**
//! All file operations must go through this module so that the blacklist and
//! dry-run checks are always enforced.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use crate::errors::{CoreError, CoreResult};

// ─── Hard-coded blacklist ────────────────────────────────────────────────────

/// Paths that are never touched regardless of what any cleaner requests.
static BLACKLISTED_PREFIXES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "/",
        "/bin",
        "/boot",
        "/dev",
        "/etc",
        "/home",
        "/lib",
        "/lib64",
        "/proc",
        "/root",
        "/run",
        "/sbin",
        "/sys",
        "/usr",
        "/var",
    ]
    .into()
});

/// Paths within a blacklisted prefix that are explicitly allowed for cleaning.
static WHITELISTED_PATHS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "/var/cache/pacman/pkg",
        "/var/log/journal",
    ]
    .into()
});

// ─── Path validation ──────────────────────────────────────────────────────────

/// Canonicalise `path` and verify it is safe to operate on.
///
/// Returns the canonicalised `PathBuf` on success.
pub fn validate_path(path: &Path) -> CoreResult<PathBuf> {
    let canonical = path.canonicalize().map_err(CoreError::Io)?;
    check_blacklist(&canonical)?;
    Ok(canonical)
}

/// Check whether a **canonicalised** path is blacklisted.
pub fn check_blacklist(canonical: &Path) -> CoreResult<()> {
    let s = canonical.to_string_lossy();

    // If the path is in the whitelist it's always OK.
    if WHITELISTED_PATHS.contains(s.as_ref()) {
        return Ok(());
    }

    for prefix in BLACKLISTED_PREFIXES.iter() {
        // Exact match or proper prefix (path starts with "/etc/")
        if s == *prefix || s.starts_with(&format!("{prefix}/")) {
            return Err(CoreError::BlacklistedPath { path: s.into_owned() });
        }
    }
    Ok(())
}

/// Detect obvious path-traversal attempts (`..` components) before
/// canonicalisation.
pub fn check_no_traversal(path: &Path) -> CoreResult<()> {
    use std::path::Component;
    if path.components().any(|c| c == Component::ParentDir) {
        return Err(CoreError::PathTraversal { path: path.to_string_lossy().into_owned() });
    }
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_blacklisted() {
        assert!(check_blacklist(Path::new("/")).is_err());
    }

    #[test]
    fn etc_is_blacklisted() {
        assert!(check_blacklist(Path::new("/etc/passwd")).is_err());
    }

    #[test]
    fn tmp_is_allowed() {
        assert!(check_blacklist(Path::new("/tmp/wisp-test")).is_ok());
    }

    #[test]
    fn traversal_detected() {
        assert!(check_no_traversal(Path::new("/home/user/../root")).is_err());
    }

    #[test]
    fn normal_path_no_traversal() {
        assert!(check_no_traversal(Path::new("/home/user/.cache")).is_ok());
    }
}
