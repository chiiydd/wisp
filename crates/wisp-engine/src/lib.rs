//! L4 – Orchestration engine.
//!
//! Assembles `CleanPlan`s from L3 cleaners, handles confirmation, executes
//! actions, streams `ProgressEvent`s, writes the audit log, and persists
//! history.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{instrument, warn};
use uuid::Uuid;

use wisp_cleaners::{CLEANERS, CleanCtx, CleanerEntry};
use wisp_core::types::{
    ActionId, ActionResult, CleanAction, CleanPlan, CleanPlanSummary, CleanReport, CleanerGroup,
    ConfirmRequest, Confirmation, DeletionVia, Privileges, ProgressEvent, RiskLevel,
};

pub mod audit;
pub mod history;

pub use wisp_cleaners::all_cleaners;

// ─── Re-exports for L5 consumers ─────────────────────────────────────────────
// L5 crates (wisp-tui, wisp-cli) must not depend on wisp-core or wisp-platform
// directly — everything they need is re-exported here.
pub use wisp_core::config;
pub use wisp_core::fs;
pub use wisp_core::scanner;
pub use wisp_core::types;
pub use wisp_core::{CoreError, CoreResult};
pub use wisp_platform::detect_distro;
pub use wisp_platform::{Distro, DistroKind};

// ─── Built-in confirmers ──────────────────────────────────────────────────────

use std::future::Future;
use std::pin::Pin;

/// A `Confirmer` that approves every request automatically.
pub struct AutoApproveConfirmer;

impl wisp_core::types::Confirmer for AutoApproveConfirmer {
    fn ask<'a>(
        &'a self,
        _req: ConfirmRequest,
    ) -> Pin<Box<dyn Future<Output = Confirmation> + Send + 'a>> {
        Box::pin(async { Confirmation::Approved })
    }
}

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
    ///
    /// Cleaner `plan()` calls run **concurrently** — most are I/O-bound
    /// (subprocess invocations like `pacman -Qtdq`, `docker system df`,
    /// `journalctl --disk-usage`, plus directory walks via `path_size`),
    /// so running them in parallel eliminates the serialised fork
    /// latency. The order of the resulting `actions` matches the order
    /// of the resolved targets so dry-run output stays stable.
    #[instrument(name = "wisp.plan", skip(self), fields(targets = ?targets))]
    pub async fn build_plan(&self, targets: &[&str]) -> CoreResult<CleanPlan> {
        let ctx = CleanCtx {
            dry_run: self.config.dry_run,
            distro: self.distro.clone(),
        };
        let entries = resolve_targets(targets);

        let supported: Vec<&'static CleanerEntry> = entries
            .into_iter()
            .filter(|e| e.meta.supported_on(self.distro.as_ref()))
            .collect();

        // Spawn each cleaner's plan future and await them all together.
        // `futures::future::join_all` polls the set concurrently on the
        // current task without requiring `'static` futures, so the borrow
        // of `&ctx` is fine.
        let plan_results = futures::future::join_all(supported.iter().map(|entry| {
            let ctx = &ctx;
            async move {
                let span = tracing::info_span!("cleaner.plan", id = %entry.meta.id());
                let _g = span.enter();
                let r = (entry.plan)(ctx).await;
                (*entry, r)
            }
        }))
        .await;

        let mut actions: Vec<CleanAction> = Vec::new();
        let mut risks: Vec<RiskLevel> = Vec::new();
        let mut warnings = Vec::new();
        let mut max_risk = RiskLevel::Trivial;
        let mut requires_root = false;

        for (entry, result) in plan_results {
            match result {
                Ok(mut acts) => {
                    let cleaner_risk = entry.meta.risk();
                    if cleaner_risk > max_risk {
                        max_risk = cleaner_risk;
                    }
                    if entry.meta.requires_root() {
                        requires_root = true;
                    }
                    risks.extend(std::iter::repeat_n(cleaner_risk, acts.len()));
                    actions.append(&mut acts);
                }
                Err(e) => {
                    let msg = format!("{}: {e}", entry.meta.id());
                    warn!(id = %entry.meta.id(), error = %e, "cleaner plan failed");
                    warnings.push(msg);
                }
            }
        }

        if !self.config.prefer_trash {
            for action in &mut actions {
                if let CleanAction::Delete { via, .. } = action {
                    *via = DeletionVia::Direct;
                }
            }
        }

        let estimated_size = actions
            .iter()
            .map(|a| match a {
                CleanAction::Delete { size, .. } => *size,
                CleanAction::RunExternal { estimated_size, .. } => estimated_size.unwrap_or(0),
            })
            .sum();

        Ok(CleanPlan {
            id: Uuid::new_v4(),
            actions,
            risks,
            estimated_size,
            required_privileges: Privileges { requires_root },
            risk: max_risk,
            warnings,
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
        let _ = tx
            .send(ProgressEvent::PlanBuilt(CleanPlanSummary::from(&plan)))
            .await;

        let mut succeeded = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;
        let mut bytes_freed = 0u64;

        let mut auto_approve_all = false;

        for (idx, action) in plan.actions.iter().enumerate() {
            let id = ActionId(idx as u64);
            let _ = tx.send(ProgressEvent::ActionStarted { id }).await;

            // Determine if confirmation is needed
            let action_risk = plan.risks.get(idx).copied().unwrap_or(plan.risk);
            let needs_confirm = !auto_approve_all && action_risk > self.config.auto_approve_up_to;

            if needs_confirm {
                let span = tracing::info_span!("wisp.confirm", id = id.0);
                let _g = span.enter();
                let req = ConfirmRequest {
                    plan_id: plan.id,
                    action: action.clone(),
                    risk: action_risk,
                };
                match confirmer.ask(req).await {
                    Confirmation::ApprovedAll => {
                        auto_approve_all = true;
                    }
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

            let result = self.exec_action(id, action).await;

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
                            result: ActionResult::Failed {
                                error: e.to_string(),
                            },
                        })
                        .await;
                }
            }
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let report = CleanReport {
            plan_id: plan.id,
            succeeded,
            failed,
            skipped,
            bytes_freed,
            timestamp,
        };

        // Persist history entry
        history::append(&report);

        let _ = tx.send(ProgressEvent::PlanFinished(report.clone())).await;
        Ok(report)
    }

    // ─── Single action execution ──────────────────────────────────────────────

    #[instrument(name = "action", skip(self, action), fields(id = action_id.0))]
    async fn exec_action(&self, action_id: ActionId, action: &CleanAction) -> CoreResult<u64> {
        match action {
            CleanAction::Delete { path, size, via } => {
                let std_path = path.as_std_path();

                let effective_via = if self.config.prefer_trash && matches!(via, DeletionVia::Trash)
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

            CleanAction::RunExternal {
                cmd,
                estimated_size,
            } => {
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
                    return Err(wisp_core::CoreError::Io(std::io::Error::other(format!(
                        "{} failed: {stderr}",
                        cmd.program
                    ))));
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
                result.extend(
                    all.iter()
                        .filter(|e| e.meta.group() == CleanerGroup::System),
                );
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
                        s == name || s.ends_with(&format!(".{name}")) || s == format!("arch.{name}")
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

#[cfg(test)]
mod resolve_targets_tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;

    fn ids(entries: &[&'static CleanerEntry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| e.meta.id().as_str().to_owned())
            .collect()
    }

    #[derive(Default)]
    struct RecordingConfirmer {
        risks: Mutex<Vec<RiskLevel>>,
    }

    impl wisp_core::types::Confirmer for RecordingConfirmer {
        fn ask<'a>(
            &'a self,
            req: ConfirmRequest,
        ) -> Pin<Box<dyn Future<Output = Confirmation> + Send + 'a>> {
            self.risks.lock().unwrap().push(req.risk);
            Box::pin(async { Confirmation::Approved })
        }
    }

    #[tokio::test]
    async fn execute_confirms_only_actions_above_threshold_by_action_risk() {
        let engine = Engine::new(
            EngineConfig {
                dry_run: true,
                prefer_trash: true,
                auto_approve_up_to: RiskLevel::Safe,
            },
            Arc::from(detect_distro()),
        );
        let plan = CleanPlan {
            id: Uuid::nil(),
            actions: vec![
                CleanAction::RunExternal {
                    cmd: wisp_core::types::ExternalCmd {
                        program: "safe-action".into(),
                        args: Vec::new(),
                    },
                    estimated_size: Some(1),
                },
                CleanAction::RunExternal {
                    cmd: wisp_core::types::ExternalCmd {
                        program: "dangerous-action".into(),
                        args: Vec::new(),
                    },
                    estimated_size: Some(2),
                },
            ],
            risks: vec![RiskLevel::Safe, RiskLevel::Dangerous],
            estimated_size: 3,
            required_privileges: Privileges {
                requires_root: false,
            },
            risk: RiskLevel::Dangerous,
            warnings: Vec::new(),
        };
        let confirmer = Arc::new(RecordingConfirmer::default());
        let (tx, _rx) = mpsc::channel(16);

        let report = engine.execute(plan, confirmer.clone(), tx).await.unwrap();

        assert_eq!(report.succeeded, 2);
        assert_eq!(*confirmer.risks.lock().unwrap(), vec![RiskLevel::Dangerous]);
    }

    #[test]
    fn at_all_returns_every_registered_cleaner() {
        let resolved = resolve_targets(&["@all"]);
        assert_eq!(resolved.len(), CLEANERS.len());
    }

    #[test]
    fn at_user_returns_only_user_group() {
        let resolved = resolve_targets(&["@user"]);
        assert!(
            !resolved.is_empty(),
            "@user should match at least one cleaner"
        );
        assert!(
            resolved
                .iter()
                .all(|e| e.meta.group() == CleanerGroup::User)
        );
    }

    #[test]
    fn at_dev_returns_only_dev_group() {
        let resolved = resolve_targets(&["@dev"]);
        assert!(!resolved.is_empty());
        assert!(resolved.iter().all(|e| e.meta.group() == CleanerGroup::Dev));
    }

    #[test]
    fn suffix_match_resolves_short_name() {
        // "browser_cache" should match "user.browser_cache".
        let resolved = resolve_targets(&["browser_cache"]);
        let names = ids(&resolved);
        assert!(names.iter().any(|s| s == "user.browser_cache"));
    }

    #[test]
    fn exact_id_match_takes_precedence_or_at_least_works() {
        let resolved = resolve_targets(&["user.thumbnails"]);
        let names = ids(&resolved);
        assert_eq!(names, vec!["user.thumbnails"]);
    }

    #[test]
    fn unknown_target_resolves_to_nothing() {
        let resolved = resolve_targets(&["definitely_not_a_real_cleaner_xyz"]);
        assert!(resolved.is_empty());
    }

    #[test]
    fn dedup_preserves_first_occurrence_order() {
        // "@user" includes user.thumbnails; explicitly naming it again must
        // not add a duplicate.
        let resolved = resolve_targets(&["@user", "thumbnails"]);
        let names = ids(&resolved);
        let count = names
            .iter()
            .filter(|s| s.as_str() == "user.thumbnails")
            .count();
        assert_eq!(count, 1, "user.thumbnails should appear exactly once");
    }

    #[test]
    fn order_follows_input_targets() {
        // Naming a specific cleaner before @user means it appears first; @user
        // then contributes the remaining user-group cleaners.
        let resolved = resolve_targets(&["thumbnails", "@user"]);
        let names = ids(&resolved);
        assert_eq!(names.first().map(String::as_str), Some("user.thumbnails"));
    }

    #[test]
    fn empty_target_list_returns_empty() {
        assert!(resolve_targets(&[]).is_empty());
    }
}
