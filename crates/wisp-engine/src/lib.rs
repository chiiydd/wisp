//! L4 – Orchestration engine.
//!
//! Assembles `CleanPlan`s from L3 cleaners, handles confirmation, executes
//! actions, streams `ProgressEvent`s, writes the audit log, and persists
//! history.

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{instrument, warn};
use uuid::Uuid;

use wisp_cleaners::{CleanCtx, CleanerEntry, CLEANERS};
use wisp_core::types::{
    ActionId, ActionResult, CleanAction, CleanPlan, CleanPlanSummary, CleanReport, CleanerGroup,
    Confirmation, ConfirmRequest, DeletionVia, Privileges, ProgressEvent, RiskLevel,
};
use wisp_core::CoreResult;
use wisp_platform::Distro;

pub mod audit;
pub mod history;

pub use wisp_cleaners::all_cleaners;

// ─── Engine config ────────────────────────────────────────────────────────────

/// Configuration supplied when creating an `Engine`.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// If `true`, no filesystem mutations are performed.
    pub dry_run: bool,
    /// Send deleted files to the trash instead of direct deletion.
    pub prefer_trash: bool,
    /// Automatically approve actions whose risk ≤ this level.
    pub auto_approve_up_to: RiskLevel,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            dry_run: false,
            prefer_trash: true,
            auto_approve_up_to: RiskLevel::Safe,
        }
    }
}

// ─── Engine ───────────────────────────────────────────────────────────────────

/// The orchestration engine.  All cleaning operations go through this struct.
pub struct Engine {
    pub config: EngineConfig,
    pub distro: Arc<dyn Distro>,
}

impl Engine {
    pub fn new(config: EngineConfig, distro: Arc<dyn Distro>) -> Self {
        Self { config, distro }
    }

    // ─── Plan building ────────────────────────────────────────────────────────

    /// Build a `CleanPlan` for the given target names / groups.
    ///
    /// Targets may be exact cleaner IDs (`"arch.pacman"`) or group aliases
    /// (`"@user"`, `"@system"`, `"@dev"`, `"@all"`).
    #[instrument(name = "wisp.plan", skip(self), fields(targets = ?targets))]
    pub async fn build_plan(&self, targets: &[&str]) -> CoreResult<CleanPlan> {
        let ctx = CleanCtx { dry_run: self.config.dry_run, distro: self.distro.clone() };
        let entries = resolve_targets(targets);

        let mut actions: Vec<CleanAction> = Vec::new();
        let mut max_risk = RiskLevel::Trivial;
        let mut requires_root = false;

        for entry in entries {
            let span = tracing::info_span!("cleaner.plan", id = %entry.meta.id());
            let _g = span.enter();

            if !entry.meta.supported_on(self.distro.as_ref()) {
                continue;
            }

            match (entry.plan)(&ctx).await {
                Ok(mut acts) => {
                    if entry.meta.risk() > max_risk {
                        max_risk = entry.meta.risk();
                    }
                    if entry.meta.requires_root() {
                        requires_root = true;
                    }
                    actions.append(&mut acts);
                }
                Err(e) => {
                    warn!(id = %entry.meta.id(), error = %e, "cleaner plan failed");
                }
            }
        }

        let estimated_size = actions
            .iter()
            .map(|a| match a {
                CleanAction::Delete { size, .. } => *size,
                CleanAction::RunExternal { estimated_size, .. } => {
                    estimated_size.unwrap_or(0)
                }
            })
            .sum();

        Ok(CleanPlan {
            id: Uuid::new_v4(),
            actions,
            estimated_size,
            required_privileges: Privileges { requires_root },
            risk: max_risk,
        })
    }

    // ─── Execution ────────────────────────────────────────────────────────────

    /// Execute a plan, streaming `ProgressEvent`s to `tx`.
    ///
    /// Confirmation is requested via `confirmer` for actions above the
    /// auto-approve threshold.
    #[instrument(name = "wisp.execute", skip(self, plan, confirmer, tx),
                 fields(plan_id = %plan.id))]
    pub async fn execute(
        &self,
        plan: CleanPlan,
        confirmer: Arc<dyn wisp_core::types::Confirmer>,
        tx: mpsc::Sender<ProgressEvent>,
    ) -> CoreResult<CleanReport> {
        let _ = tx.send(ProgressEvent::PlanBuilt(CleanPlanSummary::from(&plan))).await;

        let mut succeeded = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;
        let mut bytes_freed = 0u64;

        let mut auto_approve_all = false;

        for (idx, action) in plan.actions.iter().enumerate() {
            let id = ActionId(idx as u64);
            let _ = tx.send(ProgressEvent::ActionStarted { id }).await;

            // Determine if confirmation is needed
            let needs_confirm = !auto_approve_all
                && plan.risk > self.config.auto_approve_up_to;

            if needs_confirm {
                let req = ConfirmRequest {
                    plan_id: plan.id,
                    action: action.clone(),
                    risk: plan.risk,
                };
                match confirmer.ask(req).await {
                    Confirmation::ApprovedAll => { auto_approve_all = true; }
                    Confirmation::Approved => {}
                    Confirmation::Denied => {
                        skipped += 1;
                        let _ = tx
                            .send(ProgressEvent::ActionFinished {
                                id,
                                result: ActionResult::Skipped {
                                    reason: "denied by user".into(),
                                },
                            })
                            .await;
                        continue;
                    }
                }
            }

            let result = self.exec_action(action).await;

            match result {
                Ok(freed) => {
                    succeeded += 1;
                    bytes_freed += freed;
                    audit::write_entry(action, freed, self.config.dry_run);
                    let _ = tx
                        .send(ProgressEvent::ActionFinished {
                            id,
                            result: ActionResult::Success { bytes_freed: freed },
                        })
                        .await;
                }
                Err(e) => {
                    failed += 1;
                    let _ = tx
                        .send(ProgressEvent::ActionFinished {
                            id,
                            result: ActionResult::Failed { error: e.to_string() },
                        })
                        .await;
                }
            }
        }

        let report = CleanReport {
            plan_id: plan.id,
            succeeded,
            failed,
            skipped,
            bytes_freed,
        };

        // Persist history entry
        history::append(&report);

        let _ = tx.send(ProgressEvent::PlanFinished(report.clone())).await;
        Ok(report)
    }

    // ─── Single action execution ──────────────────────────────────────────────

    #[instrument(name = "action", skip(self, action))]
    async fn exec_action(&self, action: &CleanAction) -> CoreResult<u64> {

        match action {
            CleanAction::Delete { path, size, via } => {
                let std_path = path.as_std_path();

                let effective_via = if self.config.prefer_trash
                    && matches!(via, DeletionVia::Trash)
                {
                    DeletionVia::Trash
                } else {
                    DeletionVia::Direct
                };

                match effective_via {
                    DeletionVia::Trash => {
                        wisp_core::trash::send_to_trash(std_path, self.config.dry_run)?;
                    }
                    DeletionVia::Direct => {
                        wisp_core::trash::delete_direct(std_path, self.config.dry_run)?;
                    }
                }
                Ok(*size)
            }

            CleanAction::RunExternal { cmd, estimated_size } => {
                if self.config.dry_run {
                    tracing::info!(
                        program = %cmd.program,
                        args = ?cmd.args,
                        "dry-run: would run external command"
                    );
                    return Ok(estimated_size.unwrap_or(0));
                }

                let out = tokio::process::Command::new(&cmd.program)
                    .args(&cmd.args)
                    .output()
                    .await
                    .map_err(wisp_core::CoreError::Io)?;

                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    return Err(wisp_core::CoreError::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("{} failed: {stderr}", cmd.program),
                    )));
                }
                Ok(estimated_size.unwrap_or(0))
            }
        }
    }
}

// ─── Target resolution ────────────────────────────────────────────────────────

/// Resolve target strings to cleaner entries.
pub fn resolve_targets(targets: &[&str]) -> Vec<&'static CleanerEntry> {
    let all: &'static [CleanerEntry] = &CLEANERS;
    let mut result: Vec<&'static CleanerEntry> = Vec::new();

    for target in targets {
        match *target {
            "@all" => result.extend(all),
            "@system" => {
                result.extend(all.iter().filter(|e| e.meta.group() == CleanerGroup::System));
            }
            "@user" => {
                result.extend(all.iter().filter(|e| e.meta.group() == CleanerGroup::User));
            }
            "@dev" => {
                result.extend(all.iter().filter(|e| e.meta.group() == CleanerGroup::Dev));
            }
            name => {
                // Exact match first, then suffix match (e.g. "pacman" → "arch.pacman")
                let matched = all
                    .iter()
                    .filter(|e| {
                        let id = e.meta.id();
                        let s = id.as_str();
                        s == name
                            || s.ends_with(&format!(".{name}"))
                            || s == &format!("arch.{name}")
                    })
                    .collect::<Vec<_>>();
                result.extend(matched);
            }
        }
    }

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    result.retain(|e| seen.insert(e.meta.id().as_str().to_owned()));

    result
}
