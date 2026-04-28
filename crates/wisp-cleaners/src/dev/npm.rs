//! `dev.npm` – npm cache cleaner.

use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, ExternalCmd, RiskLevel};
use wisp_platform::Distro;

use crate::{CLEANERS, CleanCtx, CleanerEntry, PlanFuture, binary_exists};

struct NpmMeta;

impl CleanerMeta for NpmMeta {
    fn id(&self) -> CleanerId {
        CleanerId::new("dev.npm")
    }
    fn name(&self) -> &str {
        "npm cache"
    }
    fn description(&self) -> &str {
        "Clear the npm package cache via `npm cache clean --force`."
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

fn plan<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        if !binary_exists("npm") {
            return Ok(Vec::new());
        }

        Ok(vec![CleanAction::RunExternal {
            cmd: ExternalCmd {
                program: "npm".into(),
                args: vec!["cache".into(), "clean".into(), "--force".into()],
            },
            estimated_size: None,
        }])
    })
}

static META: NpmMeta = NpmMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };

#[cfg(test)]
mod tests {
    //! Plan-structure check. We don't exec `npm cache clean --force`:
    //! actually invoking npm in tests would mutate real cache state, and
    //! command execution is the engine's responsibility (CleanAction is
    //! just a value).
    use super::*;
    use crate::CleanCtx;
    use std::sync::Arc;

    fn make_ctx() -> CleanCtx {
        CleanCtx {
            dry_run: true,
            distro: Arc::from(wisp_platform::detect_distro()),
        }
    }

    #[tokio::test]
    async fn plan_empty_when_npm_missing() {
        if crate::binary_exists("npm") {
            return;
        }
        assert!(plan(&make_ctx()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn plan_emits_cache_clean_force_when_npm_present() {
        if !crate::binary_exists("npm") {
            return;
        }
        let actions = plan(&make_ctx()).await.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            CleanAction::RunExternal { cmd, .. } => {
                assert_eq!(cmd.program, "npm");
                assert_eq!(cmd.args, vec!["cache", "clean", "--force"]);
            }
            CleanAction::Delete { .. } => panic!("npm cleaner must emit RunExternal"),
        }
    }
}
