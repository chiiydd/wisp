//! `user.flatpak` – Remove unused Flatpak runtimes.

use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, ExternalCmd, RiskLevel};
use wisp_platform::Distro;

use crate::{CLEANERS, CleanCtx, CleanerEntry, PlanFuture, binary_exists};

struct FlatpakMeta;

impl CleanerMeta for FlatpakMeta {
    fn id(&self) -> CleanerId {
        CleanerId::new("user.flatpak")
    }
    fn name(&self) -> &str {
        "Flatpak unused runtimes"
    }
    fn description(&self) -> &str {
        "Uninstall unused Flatpak runtimes and extensions via `flatpak uninstall --unused`."
    }
    fn risk(&self) -> RiskLevel {
        RiskLevel::Moderate
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
        if !binary_exists("flatpak") {
            return Ok(Vec::new());
        }

        Ok(vec![CleanAction::RunExternal {
            cmd: ExternalCmd {
                program: "flatpak".into(),
                args: vec!["uninstall".into(), "--unused".into(), "-y".into()],
            },
            estimated_size: None,
        }])
    })
}

static META: FlatpakMeta = FlatpakMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };
