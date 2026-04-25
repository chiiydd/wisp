//! `user.browser` – Firefox and Chromium HTTP-cache cleaner.

use camino::Utf8PathBuf;
use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, DeletionVia, RiskLevel};
use wisp_platform::Distro;

use crate::{CleanCtx, CleanerEntry, PlanFuture, CLEANERS};

struct BrowserMeta;

impl CleanerMeta for BrowserMeta {
    fn id(&self) -> CleanerId { CleanerId::new("user.browser") }
    fn name(&self) -> &str { "Browser cache" }
    fn description(&self) -> &str {
        "Delete Firefox and Chromium HTTP caches from ~/.cache."
    }
    fn risk(&self) -> RiskLevel { RiskLevel::Trivial }
    fn requires_root(&self) -> bool { false }
    fn supported_on(&self, _distro: &dyn Distro) -> bool { true }
    fn group(&self) -> CleanerGroup { CleanerGroup::User }
}

fn plan<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return Ok(Vec::new()),
        };
        let cache = home.join(".cache");

        let candidates: &[&str] = &[
            "mozilla/firefox",           // Firefox cache2
            "chromium",                  // Chromium default profile
            "google-chrome",             // Chrome
            "BraveSoftware/Brave-Browser",
        ];

        let mut actions = Vec::new();
        for rel in candidates {
            let dir = cache.join(rel);
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

static META: BrowserMeta = BrowserMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };
