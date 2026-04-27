//! `dev.cargo` – Cargo registry and git cache cleaner.

use wisp_core::types::{CleanerGroup, CleanerId, CleanerMeta, DeletionVia, RiskLevel};
use wisp_platform::Distro;

use crate::{CLEANERS, CleanCtx, CleanerEntry, PlanFuture, delete_home_subdirs};

struct CargoMeta;

impl CleanerMeta for CargoMeta {
    fn id(&self) -> CleanerId {
        CleanerId::new("dev.cargo")
    }
    fn name(&self) -> &str {
        "Cargo cache"
    }
    fn description(&self) -> &str {
        "Remove ~/.cargo/registry/cache and ~/.cargo/registry/src (re-downloaded on demand)."
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
        Ok(delete_home_subdirs(
            &[".cargo/registry/cache", ".cargo/registry/src"],
            DeletionVia::Direct,
        ))
    })
}

static META: CargoMeta = CargoMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };
