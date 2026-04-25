//! `dev.cargo` – Cargo registry and git cache cleaner.

use camino::Utf8PathBuf;
use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, DeletionVia, RiskLevel};
use wisp_platform::Distro;

use crate::{CleanCtx, CleanerEntry, PlanFuture, CLEANERS};

struct CargoMeta;

impl CleanerMeta for CargoMeta {
    fn id(&self) -> CleanerId { CleanerId::new("dev.cargo") }
    fn name(&self) -> &str { "Cargo cache" }
    fn description(&self) -> &str {
        "Remove ~/.cargo/registry/cache and ~/.cargo/registry/src (re-downloaded on demand)."
    }
    fn risk(&self) -> RiskLevel { RiskLevel::Safe }
    fn requires_root(&self) -> bool { false }
    fn supported_on(&self, _distro: &dyn Distro) -> bool { true }
    fn group(&self) -> CleanerGroup { CleanerGroup::Dev }
}

fn plan<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return Ok(Vec::new()),
        };
        let cargo = home.join(".cargo");
        let targets: &[&str] = &["registry/cache", "registry/src"];

        let mut actions = Vec::new();
        for rel in targets {
            let dir = cargo.join(rel);
            if !dir.exists() {
                continue;
            }
            let size = wisp_core::trash::path_size(&dir);
            if let Ok(utf8) = Utf8PathBuf::from_path_buf(dir) {
                actions.push(CleanAction::Delete { path: utf8, size, via: DeletionVia::Direct });
            }
        }
        Ok(actions)
    })
}

static META: CargoMeta = CargoMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };
