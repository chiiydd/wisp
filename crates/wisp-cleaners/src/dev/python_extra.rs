//! `dev.python_extra` – Python tooling caches beyond pip.

use std::path::Path;

use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, DeletionVia, RiskLevel};
use wisp_platform::Distro;

use crate::{CLEANERS, CleanCtx, CleanerEntry, PlanFuture, delete_subdirs_under, home_dir};

const PYTHON_EXTRA_CACHE_RELS: &[&str] = &[
    ".cache/uv",
    ".cache/pypoetry",
    ".cache/pipx",
    ".cache/ruff",
    ".cache/mypy",
    ".cache/pytest",
];

struct PythonExtraMeta;

impl CleanerMeta for PythonExtraMeta {
    fn id(&self) -> CleanerId {
        CleanerId::new("dev.python_extra")
    }
    fn name(&self) -> &str {
        "Python tool caches"
    }
    fn description(&self) -> &str {
        "Remove rebuildable Python tooling caches beyond the pip cache."
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

fn collect_python_extra_cache_actions(home: &Path) -> Vec<CleanAction> {
    delete_subdirs_under(home, PYTHON_EXTRA_CACHE_RELS, DeletionVia::Direct)
}

fn plan<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        let Some(home) = home_dir() else {
            return Ok(Vec::new());
        };
        Ok(collect_python_extra_cache_actions(&home))
    })
}

static META: PythonExtraMeta = PythonExtraMeta;

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
    fn python_extra_collects_common_tool_caches() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path();
        std::fs::create_dir_all(home.join(".cache/uv")).unwrap();
        std::fs::create_dir_all(home.join(".cache/pypoetry")).unwrap();
        std::fs::create_dir_all(home.join(".cache/ruff")).unwrap();

        let actions = collect_python_extra_cache_actions(home);
        let paths = collect_paths(&actions);

        assert!(paths.iter().any(|p| p.ends_with(".cache/uv")));
        assert!(paths.iter().any(|p| p.ends_with(".cache/pypoetry")));
        assert!(paths.iter().any(|p| p.ends_with(".cache/ruff")));
        for action in &actions {
            let CleanAction::Delete { via, .. } = action else {
                panic!("python extra cleaner must only emit Delete actions");
            };
            assert_eq!(*via, DeletionVia::Direct);
        }
    }
}
