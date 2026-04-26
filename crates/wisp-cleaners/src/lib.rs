//! L3 – Cleaner implementations.
//!
//! Each cleaner is a self-contained module.  Registration is automatic via
//! `linkme::distributed_slice` – no manual bookkeeping required.
//!
//! See `docs/adding-a-cleaner.md` for the step-by-step guide.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use wisp_core::CoreResult;
use wisp_core::types::{CleanAction, CleanerMeta};
use wisp_platform::Distro;

pub mod dev;
pub mod system;
pub mod user;

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
