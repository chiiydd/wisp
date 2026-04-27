//! `dev.go` – Go module download cache cleaner.

use camino::Utf8PathBuf;
use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, DeletionVia, RiskLevel};
use wisp_platform::Distro;

use crate::{CLEANERS, CleanCtx, CleanerEntry, PlanFuture, binary_exists, home_dir};

struct GoMeta;

impl CleanerMeta for GoMeta {
    fn id(&self) -> CleanerId {
        CleanerId::new("dev.go")
    }
    fn name(&self) -> &str {
        "Go module cache"
    }
    fn description(&self) -> &str {
        "Remove the Go module download cache ($(go env GOPATH)/pkg/mod/cache)."
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
        // Skip the `go env GOPATH` subprocess when go isn't on $PATH —
        // the fallback path only matters if a stale `~/go` is left over.
        let gopath = if binary_exists("go") {
            gopath().await
        } else {
            None
        };
        let dir = match gopath {
            Some(p) => p.join("pkg/mod/cache"),
            None => {
                let Some(home) = home_dir() else {
                    return Ok(Vec::new());
                };
                home.join("go/pkg/mod/cache")
            }
        };

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

async fn gopath() -> Option<std::path::PathBuf> {
    let out = tokio::process::Command::new("go")
        .args(["env", "GOPATH"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if path.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(path))
    }
}

static META: GoMeta = GoMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };
