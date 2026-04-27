//! `user.thumbnails` – `~/.cache/thumbnails` cleaner.

use wisp_core::types::{CleanerGroup, CleanerId, CleanerMeta, DeletionVia, RiskLevel};
use wisp_platform::Distro;

use crate::{CLEANERS, CleanCtx, CleanerEntry, PlanFuture, delete_home_subdirs};

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
        Ok(delete_home_subdirs(
            &[".cache/thumbnails"],
            DeletionVia::Direct,
        ))
    })
}

static META: ThumbnailsMeta = ThumbnailsMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };
