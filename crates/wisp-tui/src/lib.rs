//! L5 – TUI presentation layer.
//!
//! Full implementation in Phase 6.  This stub exists so the workspace
//! compiles and the crate graph is complete.

use wisp_core::CoreResult;

/// Launch the full-screen TUI.  Phase 0 stub – not yet implemented.
///
/// # Errors
///
/// Always returns `Err` with an unsupported-platform message until Phase 6.
pub async fn run_tui() -> CoreResult<()> {
    Err(wisp_core::CoreError::UnsupportedPlatform {
        platform: "TUI not yet implemented (Phase 6)".into(),
    })
}
