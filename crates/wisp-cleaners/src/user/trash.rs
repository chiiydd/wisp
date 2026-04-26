//! `user.trash` – Empty the user's trash can.

use camino::Utf8PathBuf;
use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, DeletionVia, RiskLevel};
use wisp_platform::Distro;

use crate::{CLEANERS, CleanCtx, CleanerEntry, PlanFuture};

struct TrashMeta;

impl CleanerMeta for TrashMeta {
    fn id(&self) -> CleanerId {
        CleanerId::new("user.trash")
    }
    fn name(&self) -> &str {
        "Trash can"
    }
    fn description(&self) -> &str {
        "Permanently delete all files in ~/.local/share/Trash."
    }
    fn risk(&self) -> RiskLevel {
        RiskLevel::Safe
    }
    fn requires_root(&self) -> bool {
        false
    }
    fn supported_on(&self, _distro: &dyn Distro) -> bool {
        true
    }
    fn group(&self) -> CleanerGroup {
        CleanerGroup::User
    }
}

fn plan<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return Ok(Vec::new()),
        };

        let trash_dirs: &[std::path::PathBuf] = &[
            home.join(".local/share/Trash/files"),
            home.join(".local/share/Trash/info"),
        ];

        let mut actions = Vec::new();
        for dir in trash_dirs {
            if !dir.exists() {
                continue;
            }
            for entry in std::fs::read_dir(dir).map_err(wisp_core::CoreError::Io)? {
                let entry = entry.map_err(wisp_core::CoreError::Io)?;
                let path = entry.path();
                let size = wisp_core::trash::path_size(&path);
                if let Ok(utf8) = Utf8PathBuf::from_path_buf(path) {
                    actions.push(CleanAction::Delete {
                        path: utf8,
                        size,
                        via: DeletionVia::Direct,
                    });
                }
            }
        }
        Ok(actions)
    })
}

static META: TrashMeta = TrashMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };
