//! `user.thumbnails` – `~/.cache/thumbnails` cleaner.

use camino::Utf8PathBuf;
use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, DeletionVia, RiskLevel};
use wisp_platform::Distro;

use crate::{CLEANERS, CleanCtx, CleanerEntry, PlanFuture};

struct ThumbnailsMeta;

impl CleanerMeta for ThumbnailsMeta {
    fn id(&self) -> CleanerId {
        CleanerId::new("user.thumbnails")
    }
    fn name(&self) -> &str {
        "Thumbnail cache"
    }
    fn description(&self) -> &str {
        "Delete the freedesktop.org thumbnail cache from ~/.cache/thumbnails."
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

fn plan<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return Ok(Vec::new()),
        };
        let dir = home.join(".cache").join("thumbnails");
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let size = wisp_core::trash::path_size(&dir);
        let utf8 = Utf8PathBuf::from_path_buf(dir)
            .map_err(|_| wisp_core::CoreError::Config("non-UTF-8 path".into()))?;
        Ok(vec![CleanAction::Delete {
            path: utf8,
            size,
            via: DeletionVia::Direct,
        }])
    })
}

static META: ThumbnailsMeta = ThumbnailsMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };
