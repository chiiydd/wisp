//! `dev.javascript` – JavaScript toolchain package and build caches.

use std::path::Path;

use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, DeletionVia, RiskLevel};
use wisp_platform::Distro;

use crate::{CLEANERS, CleanCtx, CleanerEntry, PlanFuture, delete_subdirs_under, home_dir};

const JAVASCRIPT_CACHE_RELS: &[&str] = &[
    ".cache/yarn",
    ".cache/pnpm",
    ".local/share/pnpm/store",
    ".npm/_cacache",
    ".cache/node-gyp",
    ".cache/electron",
    ".cache/electron-builder",
    ".cache/Cypress",
    ".cache/ms-playwright",
    ".bun/install/cache",
];

struct JavaScriptMeta;

impl CleanerMeta for JavaScriptMeta {
    fn id(&self) -> CleanerId {
        CleanerId::new("dev.javascript")
    }
    fn name(&self) -> &str {
        "JavaScript toolchain caches"
    }
    fn description(&self) -> &str {
        "Remove rebuildable JavaScript package-manager and build-tool caches."
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
        CleanerGroup::Dev
    }
}

fn collect_javascript_cache_actions(home: &Path) -> Vec<CleanAction> {
    delete_subdirs_under(home, JAVASCRIPT_CACHE_RELS, DeletionVia::Direct)
}

fn plan<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        let Some(home) = home_dir() else {
            return Ok(Vec::new());
        };
        Ok(collect_javascript_cache_actions(&home))
    })
}

static META: JavaScriptMeta = JavaScriptMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };

#[cfg(test)]
mod tests {
    use super::*;
    use wisp_core::types::{CleanAction, DeletionVia};

    fn collect_paths(actions: &[CleanAction]) -> Vec<String> {
        actions
            .iter()
            .filter_map(|action| match action {
                CleanAction::Delete { path, .. } => Some(path.as_str().to_owned()),
                CleanAction::RunExternal { .. } => None,
            })
            .collect()
    }

    #[test]
    fn javascript_collects_common_toolchain_caches() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path();
        std::fs::create_dir_all(home.join(".cache/yarn")).unwrap();
        std::fs::create_dir_all(home.join(".local/share/pnpm/store")).unwrap();
        std::fs::create_dir_all(home.join(".bun/install/cache")).unwrap();

        let actions = collect_javascript_cache_actions(home);
        let paths = collect_paths(&actions);

        assert!(paths.iter().any(|p| p.ends_with(".cache/yarn")));
        assert!(paths
            .iter()
            .any(|p| p.ends_with(".local/share/pnpm/store")));
        assert!(paths
            .iter()
            .any(|p| p.ends_with(".bun/install/cache")));
        for action in &actions {
            let CleanAction::Delete { via, .. } = action else {
                panic!("javascript cleaner must only emit Delete actions");
            };
            assert_eq!(*via, DeletionVia::Direct);
        }
    }
}
