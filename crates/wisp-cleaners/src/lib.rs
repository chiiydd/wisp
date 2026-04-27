//! L3 – Cleaner implementations.
//!
//! Each cleaner is a self-contained module.  Registration is automatic via
//! `linkme::distributed_slice` – no manual bookkeeping required.
//!
//! See `docs/adding-a-cleaner.md` for the step-by-step guide.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use camino::Utf8PathBuf;
use wisp_core::CoreResult;
use wisp_core::types::{CleanAction, CleanerMeta, DeletionVia};
use wisp_platform::Distro;

pub mod dev;
pub mod system;
pub mod user;

// ─── Shared helpers (used by L3 cleaners) ─────────────────────────────────────
//
// Several cleaners repeat the same boilerplate: `dirs::home_dir()` lookup,
// "exists check + path_size + Delete action" for a list of relative paths,
// and "is this binary on $PATH?". The functions below centralise that
// pattern so individual cleaner modules stay focused on what they clean,
// not on filesystem mechanics.

/// Resolve the user's home directory, returning `None` (rather than
/// panicking) when the lookup fails.  Cleaners typically `?`-bail or
/// short-circuit on `None` since there's nothing to clean without a home.
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

/// For each path in `rels` (relative to `$HOME`), emit a `Delete` action if
/// the path exists. `via` controls trash vs direct deletion.
///
/// Returns an empty vector if `$HOME` can't be resolved or if every
/// candidate is missing.
pub fn delete_home_subdirs(rels: &[&str], via: DeletionVia) -> Vec<CleanAction> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    let mut actions = Vec::with_capacity(rels.len());
    for rel in rels {
        let dir = home.join(rel);
        if !dir.exists() {
            continue;
        }
        let size = wisp_core::trash::path_size(&dir);
        if let Ok(utf8) = Utf8PathBuf::from_path_buf(dir) {
            actions.push(CleanAction::Delete {
                path: utf8,
                size,
                via,
            });
        }
    }
    actions
}

/// Is `name` an executable on `$PATH`? Walks `$PATH` in-process — much
/// cheaper than forking `which` (which several cleaners did) and avoids
/// depending on `which` being installed.
pub fn binary_exists(name: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if is_executable_file(&candidate) {
            return true;
        }
    }
    false
}

#[cfg(unix)]
fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .map(|m| m.is_file() && (m.permissions().mode() & 0o111) != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(p: &Path) -> bool {
    p.is_file()
}

// ─── Execution context ────────────────────────────────────────────────────────

/// Context passed to every cleaner's `plan` function.
#[derive(Clone)]
pub struct CleanCtx {
    /// When `true`, cleaners must not read expensive state that is only
    /// relevant during actual execution.  Mostly advisory; the Engine
    /// enforces the dry-run fence at the deletion level.
    pub dry_run: bool,
    /// Detected distribution.
    pub distro: Arc<dyn Distro>,
}

// ─── Async plan future ────────────────────────────────────────────────────────

/// Boxed async future produced by a cleaner's plan function.
///
/// The `'a` lifetime is tied to `&'a CleanCtx` so the future may borrow
/// from the context.
pub type PlanFuture<'a> = Pin<Box<dyn Future<Output = CoreResult<Vec<CleanAction>>> + Send + 'a>>;

// ─── Registry entry ───────────────────────────────────────────────────────────

/// A compiled-in cleaner entry stored in the `CLEANERS` distributed slice.
///
/// Split into two halves as per the design doc:
/// - `meta` – synchronous, `dyn`-able, used for listing/filtering/display.
/// - `plan` – async function pointer, manually boxed to stay object-safe.
pub struct CleanerEntry {
    /// Synchronous metadata (id, name, risk, group, …).
    pub meta: &'static (dyn CleanerMeta + 'static),
    /// Async planning function.  Returns the list of actions without
    /// touching the filesystem.
    pub plan: for<'a> fn(&'a CleanCtx) -> PlanFuture<'a>,
}

// SAFETY: meta is &'static (Send+Sync); plan is a fn-pointer (always Send+Sync).
unsafe impl Send for CleanerEntry {}
unsafe impl Sync for CleanerEntry {}

// ─── Distributed slice ────────────────────────────────────────────────────────

#[linkme::distributed_slice]
pub static CLEANERS: [CleanerEntry] = [..];

/// Iterate over all registered cleaners.
pub fn all_cleaners() -> &'static [CleanerEntry] {
    &CLEANERS
}
