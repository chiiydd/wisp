//! L4 – Orchestration engine.
//!
//! Assembles `CleanPlan`s from L3 cleaners, handles confirmation callbacks,
//! executes actions concurrently, streams `ProgressEvent`s, and writes the
//! audit log.

use wisp_core::types::{CleanPlan, ProgressEvent};
use wisp_core::CoreResult;

// Re-export so callers only need to depend on wisp-engine.
pub use wisp_cleaners::all_cleaners;

/// Engine configuration supplied at construction time.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub dry_run: bool,
    pub prefer_trash: bool,
    pub auto_approve_up_to: wisp_core::types::RiskLevel,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            dry_run: false,
            prefer_trash: true,
            auto_approve_up_to: wisp_core::types::RiskLevel::Safe,
        }
    }
}

/// Main orchestration engine.  Phase 0 stub – full implementation in Phase 4.
pub struct Engine {
    config: EngineConfig,
    distro: std::sync::Arc<dyn wisp_platform::Distro>,
}

impl Engine {
    pub fn new(config: EngineConfig, distro: std::sync::Arc<dyn wisp_platform::Distro>) -> Self {
        Self { config, distro }
    }

    /// Build a plan for the given cleaner IDs.
    ///
    /// Phase 0 stub – returns an empty plan.
    pub async fn build_plan(&self, _targets: &[&str]) -> CoreResult<CleanPlan> {
        Ok(CleanPlan {
            id: uuid::Uuid::new_v4(),
            actions: Vec::new(),
            estimated_size: 0,
            required_privileges: wisp_core::types::Privileges { requires_root: false },
            risk: wisp_core::types::RiskLevel::Trivial,
        })
    }

    /// Execute a plan, streaming events to the given sender.
    ///
    /// Phase 0 stub – immediately sends `PlanFinished`.
    pub async fn execute(
        &self,
        plan: CleanPlan,
        tx: tokio::sync::mpsc::Sender<ProgressEvent>,
    ) -> CoreResult<()> {
        let report = wisp_core::types::CleanReport {
            plan_id: plan.id,
            succeeded: 0,
            failed: 0,
            skipped: 0,
            bytes_freed: 0,
        };
        let _ = tx.send(ProgressEvent::PlanFinished(report)).await;
        Ok(())
    }
}
