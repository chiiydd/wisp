//! `system.tmp` – `/tmp` old-file cleaner.
//!
//! Removes files and directories in `/tmp` that have not been accessed in
//! more than `MAX_AGE_DAYS` days.

use camino::Utf8PathBuf;
use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, DeletionVia, RiskLevel};
use wisp_platform::Distro;

use crate::{CleanCtx, CleanerEntry, PlanFuture, CLEANERS};

const MAX_AGE_DAYS: u64 = 7;
const TMP: &str = "/tmp";

struct TmpMeta;

impl CleanerMeta for TmpMeta {
    fn id(&self) -> CleanerId { CleanerId::new("system.tmp") }
    fn name(&self) -> &str { "Temporary files (/tmp)" }
    fn description(&self) -> &str {
        "Remove files and directories in /tmp not accessed in the last 7 days."
    }
    fn risk(&self) -> RiskLevel { RiskLevel::Dangerous }
    fn requires_root(&self) -> bool { false }
    fn supported_on(&self, _distro: &dyn Distro) -> bool { true }
    fn group(&self) -> CleanerGroup { CleanerGroup::System }
}

fn plan<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        let now = std::time::SystemTime::now();
        let max_age = std::time::Duration::from_secs(MAX_AGE_DAYS * 86_400);
        let mut actions = Vec::new();

        let dir = std::path::Path::new(TMP);
        if !dir.exists() {
            return Ok(actions);
        }

        for entry in std::fs::read_dir(dir).map_err(wisp_core::CoreError::Io)? {
            let entry = entry.map_err(wisp_core::CoreError::Io)?;
            let path = entry.path();
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            // Skip top-level symlinks (e.g., /tmp/.X11-unix)
            if meta.file_type().is_symlink() {
                continue;
            }

            let last_access = meta
                .accessed()
                .or_else(|_| meta.modified())
                .unwrap_or(now);

            let age = now.duration_since(last_access).unwrap_or_default();
            if age < max_age {
                continue;
            }

            let size = wisp_core::trash::path_size(&path);

            if let Ok(utf8) = Utf8PathBuf::from_path_buf(path) {
                actions.push(CleanAction::Delete {
                    path: utf8,
                    size,
                    via: DeletionVia::Direct,
                });
            }
        }

        Ok(actions)
    })
}

static META: TmpMeta = TmpMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };
