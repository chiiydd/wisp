//! L2 file-system abstractions: path safety, blacklists, and dry-run fencing.
//!
//! **Nothing above L2 should call `std::fs` or `tokio::fs` directly.**
//! All file operations must go through this module so that the blacklist and
//! dry-run checks are always enforced.

use std::path::{Path, PathBuf};

use crate::errors::{CoreError, CoreResult};

// ─── Blacklist design ─────────────────────────────────────────────────────────
//
//  Two tiers with different semantics:
//
//  EXACT_BLACKLIST   — protect the directory *itself* but allow its children.
//                      E.g. /home: the directory must not be deleted, but
//                      /home/user/.cache/… is fair game for cleaners.
//
//  PREFIX_BLACKLIST  — protect the directory AND every descendant.
//                      Used for core OS directories (/etc, /usr, …) where
//                      no child should ever be touched by a cleaner.
//
//  WHITELIST         — paths inside a PREFIX_BLACKLIST that are explicitly
//                      allowed (e.g. /var/cache/pacman/pkg inside /var).
//
//  Check order: whitelist → exact → prefix → user-home guard.

/// Directories that must never be deleted themselves, but whose *children*
/// are allowed (managed by individual cleaners).
static EXACT_BLACKLIST: &[&str] = &["/", "/home", "/tmp"];

/// Directories where both the path and **all descendants** are off-limits.
static PREFIX_BLACKLIST: &[&str] = &[
    "/bin",
    "/boot",
    "/dev",
    "/etc",
    "/lib",
    "/lib64",
    "/proc",
    "/root",   // root's home: treat as system area
    "/run",
    "/sbin",
    "/srv",
    "/sys",
    "/usr",
    "/var",
];

/// Paths inside a PREFIX_BLACKLIST that individual cleaners are allowed to target.
static WHITELIST: &[&str] = &[
    "/var/cache/pacman/pkg",
    "/var/log/journal",
];

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

    // ── 1. Whitelist (highest priority) ─────────────────────────────────
    for &allowed in WHITELIST {
        if s == allowed || s.starts_with(&format!("{allowed}/")) {
            return Ok(());
        }
    }

    // ── 2. Exact blacklist — protect the directory itself only ───────────
    for &exact in EXACT_BLACKLIST {
        if s == exact {
            return Err(CoreError::BlacklistedPath { path: s.into_owned() });
        }
    }

    // ── 3. Prefix blacklist — protect directory AND all descendants ──────
    for &prefix in PREFIX_BLACKLIST {
        if s == prefix || s.starts_with(&format!("{prefix}/")) {
            return Err(CoreError::BlacklistedPath { path: s.into_owned() });
        }
    }

    // ── 4. Protect the running user's home directory itself ──────────────
    //    (allow /home/user/.cache/… but not /home/user itself)
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() && s == home.as_str() {
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
    fn etc_and_children_are_blacklisted() {
        assert!(check_blacklist(Path::new("/etc")).is_err());
        assert!(check_blacklist(Path::new("/etc/passwd")).is_err());
    }

    #[test]
    fn home_dir_itself_is_blacklisted() {
        assert!(check_blacklist(Path::new("/home")).is_err());
    }

    #[test]
    fn home_subdir_is_allowed() {
        // /home/user/.cache/… must NOT be blocked — this is what cleaners target
        assert!(check_blacklist(Path::new("/home/user/.cache/chromium")).is_ok());
        assert!(check_blacklist(Path::new("/home/user/.local/share/Trash/files/foo")).is_ok());
    }

    #[test]
    fn var_is_blacklisted_but_pacman_cache_is_allowed() {
        assert!(check_blacklist(Path::new("/var/lib/pacman")).is_err());
        assert!(check_blacklist(Path::new("/var/cache/pacman/pkg")).is_ok());
    }

    #[test]
    fn tmp_itself_blocked_but_children_allowed() {
        assert!(check_blacklist(Path::new("/tmp")).is_err());
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
