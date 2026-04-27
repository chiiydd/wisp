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
