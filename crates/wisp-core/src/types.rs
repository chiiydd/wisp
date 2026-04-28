//! Canonical shared data types (Section 6 of the design doc).
//!
//! All fields of these types are treated as **breaking changes** in semver.

use std::future::Future;
use std::pin::Pin;

use camino::Utf8PathBuf;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use slotmap::{DefaultKey, SlotMap};
use uuid::Uuid;

use wisp_platform::Distro;

// ─── Risk & grouping ──────────────────────────────────────────────────────────

/// Safety classification for a cleaner or a single action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// Thumbnails, HTTP caches – safe to delete unconditionally.
    Trivial,
    /// Pacman cache, journal logs – safe in normal conditions.
    Safe,
    /// Orphan packages, unused flatpak data – user should understand impact.
    Moderate,
    /// `/tmp`, `docker system prune -a` – must be explicitly confirmed.
    Dangerous,
}

/// Logical grouping of cleaners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CleanerGroup {
    System,
    User,
    Dev,
}

// ─── Cleaner identity ─────────────────────────────────────────────────────────

/// Stable string identifier for a cleaner, e.g. `"arch.pacman"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CleanerId(pub CompactString);

impl CleanerId {
    pub fn new(id: impl Into<CompactString>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CleanerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ─── CleanerMeta trait (sync, object-safe) ───────────────────────────────────

/// Synchronous, object-safe half of a cleaner – used for listing, filtering,
/// and display.  Does **not** touch the filesystem.
pub trait CleanerMeta: Send + Sync {
    fn id(&self) -> CleanerId;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn risk(&self) -> RiskLevel;
    fn requires_root(&self) -> bool;
    /// Whether this cleaner is applicable on the given distribution.
    fn supported_on(&self, distro: &dyn Distro) -> bool;
    fn group(&self) -> CleanerGroup;
}

// ─── Privileges ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Privileges {
    pub requires_root: bool,
}

// ─── CleanAction ─────────────────────────────────────────────────────────────

/// How a file should be removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletionVia {
    Trash,
    Direct,
}

/// A command descriptor for `RunExternal` actions (replaces non-serialisable
/// `std::process::Command`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalCmd {
    pub program: String,
    pub args: Vec<String>,
}

/// A single concrete action that the Engine will execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CleanAction {
    Delete {
        path: Utf8PathBuf,
        /// Pre-computed size in bytes; 0 means unknown.
        size: u64,
        via: DeletionVia,
    },
    RunExternal {
        cmd: ExternalCmd,
        estimated_size: Option<u64>,
    },
}

// ─── Plan ─────────────────────────────────────────────────────────────────────

/// A complete, validated cleaning plan ready for Engine execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanPlan {
    pub id: Uuid,
    pub actions: Vec<CleanAction>,
    /// Per-action risk level — parallel to `actions` (same length).
    /// Lets the UI / confirmer filter or skip by risk without having to
    /// re-derive it from the cleaner that produced each action.
    /// `#[serde(default)]` keeps old serialized plans deserializable.
    #[serde(default)]
    pub risks: Vec<RiskLevel>,
    /// Sum of known `size` fields; may undercount when sizes are unknown.
    pub estimated_size: u64,
    pub required_privileges: Privileges,
    /// Highest `RiskLevel` among all actions.
    pub risk: RiskLevel,
}

/// Lightweight summary emitted as the first `ProgressEvent`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanPlanSummary {
    pub id: Uuid,
    pub action_count: usize,
    pub estimated_size: u64,
    pub risk: RiskLevel,
}

impl From<&CleanPlan> for CleanPlanSummary {
    fn from(p: &CleanPlan) -> Self {
        Self {
            id: p.id,
            action_count: p.actions.len(),
            estimated_size: p.estimated_size,
            risk: p.risk,
        }
    }
}

// ─── Progress events ──────────────────────────────────────────────────────────

/// Opaque identifier for a single in-flight action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActionId(pub u64);

/// Per-action outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ActionResult {
    Success { bytes_freed: u64 },
    Skipped { reason: String },
    Failed { error: String },
}

/// Events emitted by the Engine over a channel.
///
/// Both the streaming JSONL output format and the TUI consume this enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ProgressEvent {
    PlanBuilt(CleanPlanSummary),
    ActionStarted { id: ActionId },
    ActionProgress { id: ActionId, bytes_done: u64 },
    ActionFinished { id: ActionId, result: ActionResult },
    PlanFinished(CleanReport),
    Warning(String),
}

// ─── Report ───────────────────────────────────────────────────────────────────

/// Aggregate result for an entire plan execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanReport {
    pub plan_id: Uuid,
    pub succeeded: usize,
    pub failed: usize,
    pub skipped: usize,
    pub bytes_freed: u64,
    /// Unix epoch seconds; 0 for records written before this field was added.
    #[serde(default)]
    pub timestamp: u64,
}

// ─── Output envelope ──────────────────────────────────────────────────────────

/// Structured error info for `OutputEnvelope`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub message: String,
    pub code: Option<String>,
}

/// Wrapper for `--output json` responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputEnvelope<T> {
    pub version: String,
    pub command: String,
    pub data: T,
    pub warnings: Vec<String>,
    pub errors: Vec<ErrorInfo>,
}

impl<T: Serialize> OutputEnvelope<T> {
    pub fn new(command: impl Into<String>, data: T) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            command: command.into(),
            data,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }
}

// ─── Confirmation ─────────────────────────────────────────────────────────────

/// Response from a `Confirmer` to an `Engine` confirmation request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirmation {
    Approved,
    Denied,
    /// Approve this and all subsequent requests for the current plan.
    ApprovedAll,
}

/// What the Engine asks the presentation layer to confirm.
#[derive(Debug, Clone)]
pub struct ConfirmRequest {
    pub plan_id: Uuid,
    pub action: CleanAction,
    pub risk: RiskLevel,
}

/// Interface that L5 (presentation) implements so L4 (engine) can ask for
/// confirmation without knowing which UI is active.
///
/// Uses manual `Pin<Box<dyn Future>>` return to remain `dyn`-compatible
/// without the `async-trait` crate.
pub trait Confirmer: Send + Sync {
    fn ask<'a>(
        &'a self,
        req: ConfirmRequest,
    ) -> Pin<Box<dyn Future<Output = Confirmation> + Send + 'a>>;
}

// ─── Scan tree ────────────────────────────────────────────────────────────────

/// Slotmap key for a node in a `ScanTree`.
pub type ScanKey = DefaultKey;

/// A single node (file or directory) in a scanned directory tree.
#[derive(Debug, Clone)]
pub struct ScanNode {
    pub path: Utf8PathBuf,
    /// Recursive byte count (directory) or file size.
    pub size: u64,
    pub children: Vec<ScanKey>,
    pub is_dir: bool,
}

/// Index-based directory tree produced by the filesystem scanner.
#[derive(Debug, Default)]
pub struct ScanTree {
    pub nodes: SlotMap<ScanKey, ScanNode>,
    pub root: Option<ScanKey>,
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_level_ordering_matches_severity() {
        assert!(RiskLevel::Trivial < RiskLevel::Safe);
        assert!(RiskLevel::Safe < RiskLevel::Moderate);
        assert!(RiskLevel::Moderate < RiskLevel::Dangerous);
        // The whole chain is total-ordered so max() works as expected.
        assert_eq!(
            *[
                RiskLevel::Trivial,
                RiskLevel::Moderate,
                RiskLevel::Safe,
                RiskLevel::Dangerous,
            ]
            .iter()
            .max()
            .unwrap(),
            RiskLevel::Dangerous
        );
    }

    #[test]
    fn risk_level_serialises_lowercase() {
        let json = serde_json::to_string(&RiskLevel::Dangerous).unwrap();
        assert_eq!(json, "\"dangerous\"");
        let back: RiskLevel = serde_json::from_str("\"trivial\"").unwrap();
        assert_eq!(back, RiskLevel::Trivial);
    }

    #[test]
    fn deletion_via_is_copy() {
        let v = DeletionVia::Trash;
        // If DeletionVia weren't Copy this line wouldn't compile — explicitly
        // exercising the property the cleaner code relies on.
        let copy = v;
        assert_eq!(v, copy);
    }

    #[test]
    fn deletion_via_serialises_snake_case() {
        let json = serde_json::to_string(&DeletionVia::Direct).unwrap();
        assert_eq!(json, "\"direct\"");
    }

    #[test]
    fn cleaner_group_serialises_lowercase() {
        let json = serde_json::to_string(&CleanerGroup::System).unwrap();
        assert_eq!(json, "\"system\"");
    }

    #[test]
    fn cleaner_id_display_round_trips() {
        let id = CleanerId::new("user.browser_cache");
        assert_eq!(id.as_str(), "user.browser_cache");
        assert_eq!(id.to_string(), "user.browser_cache");
    }

    #[test]
    fn cleaner_id_equality_and_hash() {
        use std::collections::HashSet;
        let a = CleanerId::new("dev.cargo");
        let b = CleanerId::new("dev.cargo");
        assert_eq!(a, b);

        let mut set = HashSet::new();
        set.insert(a);
        // Hash-based dedup must collapse equal IDs.
        assert!(!set.insert(b));
    }

    #[test]
    fn clean_action_delete_round_trips_through_json() {
        let action = CleanAction::Delete {
            path: Utf8PathBuf::from("/tmp/wisp-test"),
            size: 1234,
            via: DeletionVia::Trash,
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: CleanAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn clean_plan_summary_extracts_action_count_and_risk() {
        use uuid::Uuid;

        let plan = CleanPlan {
            id: Uuid::nil(),
            actions: vec![
                CleanAction::Delete {
                    path: Utf8PathBuf::from("/tmp/a"),
                    size: 10,
                    via: DeletionVia::Direct,
                },
                CleanAction::Delete {
                    path: Utf8PathBuf::from("/tmp/b"),
                    size: 20,
                    via: DeletionVia::Direct,
                },
            ],
            risks: vec![RiskLevel::Trivial, RiskLevel::Moderate],
            estimated_size: 30,
            required_privileges: Privileges {
                requires_root: false,
            },
            risk: RiskLevel::Moderate,
        };
        let summary = CleanPlanSummary::from(&plan);
        assert_eq!(summary.action_count, 2);
        assert_eq!(summary.estimated_size, 30);
        assert_eq!(summary.risk, RiskLevel::Moderate);
    }
}
