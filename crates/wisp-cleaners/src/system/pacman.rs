//! `arch.pacman` – Arch Linux package download-cache cleaner.
//!
//! Uses `paccache` (from `pacman-contrib`) if available; falls back to
//! manual scanning of `/var/cache/pacman/pkg/`.

use camino::Utf8PathBuf;
use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, DeletionVia, RiskLevel};
use wisp_platform::{Distro, DistroKind};

use crate::{CleanCtx, CleanerEntry, PlanFuture, CLEANERS};

// ─── Metadata ─────────────────────────────────────────────────────────────────

struct PacmanMeta;

impl CleanerMeta for PacmanMeta {
    fn id(&self) -> CleanerId { CleanerId::new("arch.pacman") }
    fn name(&self) -> &str { "Pacman cache" }
    fn description(&self) -> &str {
        "Remove old package versions from /var/cache/pacman/pkg, keeping 2 per package."
    }
    fn risk(&self) -> RiskLevel { RiskLevel::Safe }
    fn requires_root(&self) -> bool { true }
    fn supported_on(&self, distro: &dyn Distro) -> bool {
        distro.kind() == DistroKind::Arch
    }
    fn group(&self) -> CleanerGroup { CleanerGroup::System }
}

// ─── Plan ─────────────────────────────────────────────────────────────────────

fn plan<'a>(ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move { build_plan(ctx).await })
}

async fn build_plan(_ctx: &CleanCtx) -> wisp_core::CoreResult<Vec<CleanAction>> {
    // Prefer paccache for accurate version-aware pruning
    if command_exists("paccache").await {
        return plan_via_paccache().await;
    }
    plan_manual().await
}

/// List files to delete by running `paccache -dqk2` (dry-list, keep 2).
async fn plan_via_paccache() -> wisp_core::CoreResult<Vec<CleanAction>> {
    let out = tokio::process::Command::new("paccache")
        .args(["-dqk2"])
        .output()
        .await
        .map_err(wisp_core::CoreError::Io)?;

    if !out.status.success() {
        tracing::warn!("paccache -dqk2 failed; falling back to manual scan");
        return plan_manual().await;
    }

    let actions = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|p| {
            let path = std::path::Path::new(p.trim());
            let size = path.metadata().map(|m| m.len()).unwrap_or(0);
            let utf8 = Utf8PathBuf::from_path_buf(path.to_owned()).ok()?;
            Some(CleanAction::Delete { path: utf8, size, via: DeletionVia::Direct })
        })
        .collect();
    Ok(actions)
}

/// Fallback: scan the cache directory and keep the 2 newest pkg files per name.
async fn plan_manual() -> wisp_core::CoreResult<Vec<CleanAction>> {
    use std::collections::HashMap;

    const CACHE: &str = "/var/cache/pacman/pkg";
    let dir = std::path::Path::new(CACHE);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    // Group files by package base-name (strip version+arch+ext suffixes)
    let mut by_name: HashMap<String, Vec<(std::path::PathBuf, u64)>> = HashMap::new();

    for entry in std::fs::read_dir(dir).map_err(wisp_core::CoreError::Io)? {
        let entry = entry.map_err(wisp_core::CoreError::Io)?;
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_owned(),
            None => continue,
        };
        // Skip signature files – they are cleaned together with their package
        if name.ends_with(".sig") {
            continue;
        }
        let base = pkg_base_name(&name).unwrap_or(&name).to_owned();
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        by_name.entry(base).or_default().push((path, size));
    }

    let mut actions = Vec::new();
    for (_base, mut files) in by_name {
        // Sort by modification time descending (newest first)
        files.sort_by(|a, b| {
            let ta = a.0.metadata().and_then(|m| m.modified()).ok();
            let tb = b.0.metadata().and_then(|m| m.modified()).ok();
            tb.cmp(&ta)
        });
        // Keep the 2 newest; schedule the rest for deletion
        for (path, size) in files.into_iter().skip(2) {
            if let Ok(utf8) = Utf8PathBuf::from_path_buf(path) {
                actions.push(CleanAction::Delete { path: utf8, size, via: DeletionVia::Direct });
            }
        }
    }
    Ok(actions)
}

/// Extract the package base-name from a `.pkg.tar.*` filename.
fn pkg_base_name(filename: &str) -> Option<&str> {
    // Format: {name}-{epoch:version-rel}-{arch}.pkg.tar.{ext}
    // Strip everything from the last `-{arch}` backwards to the 3rd-last `-`
    let without_ext = filename.split(".pkg.tar.").next()?;
    // Strip arch suffix
    let before_arch = without_ext.rfind('-').map(|i| &without_ext[..i])?;
    // Strip pkgrel
    let before_rel = before_arch.rfind('-').map(|i| &before_arch[..i])?;
    // Strip version
    let before_ver = before_rel.rfind('-').map(|i| &before_rel[..i])?;
    Some(before_ver)
}

// ─── Registration ─────────────────────────────────────────────────────────────

static META: PacmanMeta = PacmanMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };

// ─── Helpers ──────────────────────────────────────────────────────────────────

async fn command_exists(cmd: &str) -> bool {
    tokio::process::Command::new("which")
        .arg(cmd)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkg_base_name_parses_correctly() {
        assert_eq!(pkg_base_name("linux-6.9.0-1-x86_64.pkg.tar.zst"), Some("linux"));
        assert_eq!(pkg_base_name("chromium-125.0.6422.60-1-x86_64.pkg.tar.zst"), Some("chromium"));
        assert_eq!(pkg_base_name("python-3.12.3-1-x86_64.pkg.tar.zst"), Some("python"));
    }
}
