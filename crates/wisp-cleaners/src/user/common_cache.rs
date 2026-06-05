//! `user.common_cache` – common rebuildable desktop caches.

use std::path::Path;

use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, DeletionVia, RiskLevel};
use wisp_platform::Distro;

use crate::{CLEANERS, CleanCtx, CleanerEntry, PlanFuture, delete_subdirs_under, home_dir};

const COMMON_CACHE_RELS: &[&str] = &[
    ".cache/fontconfig",
    ".cache/mesa_shader_cache",
    ".cache/mesa_shader_cache_db",
    ".cache/nvidia",
    ".cache/GLCache",
    ".cache/gstreamer-1.0",
];

struct CommonCacheMeta;

impl CleanerMeta for CommonCacheMeta {
    fn id(&self) -> CleanerId {
        CleanerId::new("user.common_cache")
    }
    fn name(&self) -> &str {
        "Common user caches"
    }
    fn description(&self) -> &str {
        "Remove common rebuildable desktop caches such as fontconfig and shader caches."
    }
    fn risk(&self) -> RiskLevel {
        RiskLevel::Trivial
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

fn collect_common_cache_actions(home: &Path) -> Vec<CleanAction> {
    delete_subdirs_under(home, COMMON_CACHE_RELS, DeletionVia::Direct)
}

fn plan<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        let Some(home) = home_dir() else {
            return Ok(Vec::new());
        };
        Ok(collect_common_cache_actions(&home))
    })
}

static META: CommonCacheMeta = CommonCacheMeta;

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
    fn common_cache_collects_only_present_rebuildable_paths() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path();
        std::fs::create_dir_all(home.join(".cache/fontconfig")).unwrap();
        std::fs::create_dir_all(home.join(".cache/mesa_shader_cache")).unwrap();

        let actions = collect_common_cache_actions(home);
        let paths = collect_paths(&actions);

        assert!(paths.iter().any(|p| p.ends_with(".cache/fontconfig")));
        assert!(
            paths
                .iter()
                .any(|p| p.ends_with(".cache/mesa_shader_cache"))
        );
        assert!(!paths.iter().any(|p| p.ends_with(".cache/documents")));
        for action in &actions {
            let CleanAction::Delete { via, .. } = action else {
                panic!("common cache cleaner must only emit Delete actions");
            };
            assert_eq!(*via, DeletionVia::Direct);
        }
    }
}
