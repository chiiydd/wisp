//! `dev.pip` – pip wheel and HTTP cache cleaner.

use camino::Utf8PathBuf;
use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, DeletionVia, RiskLevel};
use wisp_platform::Distro;

use crate::{CleanCtx, CleanerEntry, PlanFuture, CLEANERS};

struct PipMeta;

impl CleanerMeta for PipMeta {
    fn id(&self) -> CleanerId { CleanerId::new("dev.pip") }
    fn name(&self) -> &str { "pip cache" }
    fn description(&self) -> &str {
        "Delete the pip HTTP and wheel cache from ~/.cache/pip."
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
        let dir = home.join(".cache").join("pip");
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let size = wisp_core::trash::path_size(&dir);
        let utf8 = Utf8PathBuf::from_path_buf(dir)
            .map_err(|_| wisp_core::CoreError::Config("non-UTF-8 home path".into()))?;
        Ok(vec![CleanAction::Delete { path: utf8, size, via: DeletionVia::Direct }])
    })
}

static META: PipMeta = PipMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };
