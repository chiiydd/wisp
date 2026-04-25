//! L3 – Cleaner implementations.
//!
//! Each cleaner is a separate module.  Registration is automatic via
//! `linkme::distributed_slice` – no manual bookkeeping required.
//!
//! # Adding a new cleaner
//!
//! See `docs/adding-a-cleaner.md` for the step-by-step checklist.

use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, RiskLevel};
use wisp_platform::Distro;

// ─── Registry ────────────────────────────────────────────────────────────────

/// Static entry registered by each cleaner module.
pub struct CleanerEntry {
    pub meta: &'static (dyn CleanerMeta + 'static),
}

// SAFETY: CleanerMeta: Send + Sync, so &'static dyn CleanerMeta is Sync.
unsafe impl Sync for CleanerEntry {}

#[linkme::distributed_slice]
pub static CLEANERS: [CleanerEntry] = [..];

/// Iterate over all registered cleaners.
pub fn all_cleaners() -> &'static [CleanerEntry] {
    &CLEANERS
}

// ─── Placeholder cleaner (Phase 0) ───────────────────────────────────────────

/// Placeholder so the slice is never empty and the binary links correctly.
struct Placeholder;

impl CleanerMeta for Placeholder {
    fn id(&self) -> CleanerId { CleanerId::new("placeholder") }
    fn name(&self) -> &str { "Placeholder" }
    fn description(&self) -> &str { "Phase-0 placeholder; will be removed in Phase 2." }
    fn risk(&self) -> RiskLevel { RiskLevel::Trivial }
    fn requires_root(&self) -> bool { false }
    fn supported_on(&self, _distro: &dyn Distro) -> bool { true }
    fn group(&self) -> CleanerGroup { CleanerGroup::System }
}

static PLACEHOLDER: Placeholder = Placeholder;

#[linkme::distributed_slice(CLEANERS)]
static PLACEHOLDER_ENTRY: CleanerEntry = CleanerEntry { meta: &PLACEHOLDER };

// ─── CleanerExec – async execution interface ─────────────────────────────────
//
// `async fn in trait` is stable (Rust 1.75+) but not object-safe.
// We dispatch via the `CleanerKind` enum in wisp-engine instead of dyn.
// Each variant's `plan` method returns the list of actions for the Engine.

/// Async context provided to cleaners during planning.
pub struct CleanCtx {
    pub dry_run: bool,
    pub distro: std::sync::Arc<dyn Distro>,
}

/// Async planning interface for a cleaner.
///
/// Not `dyn`-able by design; the Engine dispatches via `CleanerKind`.
pub trait CleanerExec {
    async fn plan(&self, ctx: &CleanCtx) -> wisp_core::CoreResult<Vec<CleanAction>>;
}
