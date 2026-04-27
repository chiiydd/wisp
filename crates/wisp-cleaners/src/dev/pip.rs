//! `dev.pip` – pip wheel and HTTP cache cleaner.

use wisp_core::types::{CleanerGroup, CleanerId, CleanerMeta, DeletionVia, RiskLevel};
use wisp_platform::Distro;

use crate::{CLEANERS, CleanCtx, CleanerEntry, PlanFuture, delete_home_subdirs};

struct PipMeta;

impl CleanerMeta for PipMeta {
    fn id(&self) -> CleanerId {
        CleanerId::new("dev.pip")
    }
    fn name(&self) -> &str {
        "pip cache"
    }
    fn description(&self) -> &str {
        "Delete the pip HTTP and wheel cache from ~/.cache/pip."
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
    Box::pin(async move { Ok(delete_home_subdirs(&[".cache/pip"], DeletionVia::Direct)) })
}

static META: PipMeta = PipMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };
