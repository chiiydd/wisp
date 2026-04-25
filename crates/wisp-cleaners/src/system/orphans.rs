//! `arch.orphans` – Remove orphan packages (installed as deps, no longer needed).

use wisp_core::types::{
    CleanAction, CleanerGroup, CleanerId, CleanerMeta, ExternalCmd, RiskLevel,
};
use wisp_platform::{Distro, DistroKind};

use crate::{CleanCtx, CleanerEntry, PlanFuture, CLEANERS};

struct OrphansMeta;

impl CleanerMeta for OrphansMeta {
    fn id(&self) -> CleanerId { CleanerId::new("arch.orphans") }
    fn name(&self) -> &str { "Orphan packages" }
    fn description(&self) -> &str {
        "Remove packages installed as dependencies that are no longer required by any installed package."
    }
    fn risk(&self) -> RiskLevel { RiskLevel::Moderate }
    fn requires_root(&self) -> bool { true }
    fn supported_on(&self, distro: &dyn Distro) -> bool {
        distro.kind() == DistroKind::Arch
    }
    fn group(&self) -> CleanerGroup { CleanerGroup::System }
}

fn plan<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        let orphans = list_orphans().await?;
        if orphans.is_empty() {
            return Ok(Vec::new());
        }

        let mut args = vec!["-Rns".to_owned(), "--noconfirm".to_owned()];
        args.extend(orphans);

        Ok(vec![CleanAction::RunExternal {
            cmd: ExternalCmd { program: "pacman".into(), args },
            estimated_size: None,
        }])
    })
}

async fn list_orphans() -> wisp_core::CoreResult<Vec<String>> {
    let out = tokio::process::Command::new("pacman")
        .args(["-Qtdq"])
        .output()
        .await
        .map_err(wisp_core::CoreError::Io)?;

    if !out.status.success() {
        return Ok(Vec::new());
    }

    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

static META: OrphansMeta = OrphansMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };
