//! `user.browser_cache` + `user.browser_state` — browser cleanup, split into
//! a safe-by-default cache cleaner and an opt-in site-data cleaner.
//!
//! ## Audit summary
//!
//! Browser profile directories mix three classes of files:
//!
//! 1. **Pure caches** — HTTP / GPU / code / shader / startup caches. The
//!    browser regenerates them on demand. `user.browser_cache` (Trivial)
//!    handles these and runs as part of `@user` by default.
//! 2. **Site-affecting state** — cookies, open-tab sessions, Service Worker
//!    CacheStorage (offline PWA data), site IndexedDB / Local Storage.
//!    Deleting these logs you out of websites, closes saved tabs and breaks
//!    offline-capable web apps. `user.browser_state` (Dangerous) covers
//!    these; the engine's risk gate forces explicit confirmation even with
//!    `--yes`. Deletion goes to the trash so it's recoverable.
//! 3. **Genuine user data** — saved passwords, bookmarks, history,
//!    extensions, autofill. **Neither cleaner ever touches these.**

use std::path::{Path, PathBuf};

use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, DeletionVia, RiskLevel};
use wisp_platform::Distro;

use crate::{CLEANERS, CleanCtx, CleanerEntry, PlanFuture};

// ─── Browser registries ───────────────────────────────────────────────────────

/// Chromium-family browsers — same on-disk profile layout.
/// `(cache_root_rel_to_$HOME/.cache, config_root_rel_to_$HOME/.config)`
const CHROMIUM_FAMILY: &[(&str, &str)] = &[
    ("chromium", ".config/chromium"),
    ("google-chrome", ".config/google-chrome"),
    ("google-chrome-beta", ".config/google-chrome-beta"),
    ("google-chrome-unstable", ".config/google-chrome-unstable"),
    (
        "BraveSoftware/Brave-Browser",
        ".config/BraveSoftware/Brave-Browser",
    ),
    ("vivaldi", ".config/vivaldi"),
    ("microsoft-edge", ".config/microsoft-edge"),
    ("opera", ".config/opera"),
];

/// Firefox-family browsers — XDG cache + dotted profile root pairs.
const FIREFOX_FAMILY: &[(&str, &str)] = &[
    ("mozilla/firefox", ".mozilla/firefox"),
    ("librewolf", ".librewolf"),
    ("floorp", ".floorp"),
    ("waterfox", ".waterfox"),
];

/// Subdirectories under a Chromium profile that are pure caches.
const CHROMIUM_CACHE_SUBDIRS: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "ShaderCache",
    "Crashpad",
    "Crash Reports",
    "Service Worker/ScriptCache",
    "optimization_guide_model_store",
    "Application Cache",
];

/// Subdirectories under a Firefox profile that are pure caches.
const FIREFOX_CACHE_SUBDIRS_PROFILE: &[&str] = &[
    "cache2",
    "startupCache",
    "thumbnails",
    "OfflineCache",
    "jumpListCache",
    "safebrowsing",
    "shader-cache",
];

/// Files / dirs under a Chromium profile that hold site state (cookies,
/// sessions, offline PWA data). **No passwords, bookmarks, history,
/// extensions or autofill.**
const CHROMIUM_STATE_PATHS: &[&str] = &[
    "Cookies",
    "Cookies-journal",
    "Sessions",
    "Session Storage",
    "IndexedDB",
    "Local Storage",
    "Service Worker/CacheStorage",
    "Service Worker/Database",
    "File System",
    "Storage/ext",
];

/// Files / dirs under a Firefox profile that hold site state.
const FIREFOX_STATE_PATHS: &[&str] = &[
    "cookies.sqlite",
    "cookies.sqlite-shm",
    "cookies.sqlite-wal",
    "cookies.sqlite-journal",
    "sessionstore.jsonlz4",
    "sessionstore-backups",
    "sessionCheckpoints.json",
    "storage/default",
    "storage/temporary",
];

// ─── Profile enumeration ──────────────────────────────────────────────────────

fn home() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Enumerate Chromium profile directories: `Default`, `Profile N`, `Guest Profile`.
fn chromium_profiles(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in rd.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        if name_s == "Default" || name_s == "Guest Profile" || name_s.starts_with("Profile ") {
            out.push(entry.path());
        }
    }
    out
}

/// Enumerate Firefox profile directories — anything containing `prefs.js` or
/// `times.json`. Uses presence-based detection rather than name patterns so
/// renamed profiles still work.
fn firefox_profiles(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in rd.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let p = entry.path();
        if p.join("prefs.js").exists() || p.join("times.json").exists() {
            out.push(p);
        }
    }
    out
}

// ─── Action builders ──────────────────────────────────────────────────────────

use crate::push_delete;

fn collect_cache_actions(home: &Path) -> Vec<CleanAction> {
    let mut actions = Vec::new();
    let cache = home.join(".cache");

    // Chromium-family: cache lives both in $HOME/.cache/<browser>/<profile>/...
    // and in $HOME/.config/<browser>/<profile>/<cache subdirs>.
    for (cache_rel, config_rel) in CHROMIUM_FAMILY {
        let cache_root = cache.join(cache_rel);
        for profile in chromium_profiles(&cache_root) {
            for sub in CHROMIUM_CACHE_SUBDIRS {
                push_delete(&mut actions, profile.join(sub), DeletionVia::Direct);
            }
        }

        let config_root = home.join(config_rel);
        for profile in chromium_profiles(&config_root) {
            for sub in CHROMIUM_CACHE_SUBDIRS {
                push_delete(&mut actions, profile.join(sub), DeletionVia::Direct);
            }
        }
    }

    // Firefox-family: cache strictly under $HOME/.cache/<browser>/<profile>/.
    for (cache_rel, _) in FIREFOX_FAMILY {
        let cache_root = cache.join(cache_rel);
        for profile in firefox_profiles(&cache_root) {
            for sub in FIREFOX_CACHE_SUBDIRS_PROFILE {
                push_delete(&mut actions, profile.join(sub), DeletionVia::Direct);
            }
        }
    }

    actions
}

fn collect_state_actions(home: &Path) -> Vec<CleanAction> {
    let mut actions = Vec::new();

    for (_, config_rel) in CHROMIUM_FAMILY {
        let config_root = home.join(config_rel);
        for profile in chromium_profiles(&config_root) {
            for sub in CHROMIUM_STATE_PATHS {
                push_delete(&mut actions, profile.join(sub), DeletionVia::Trash);
            }
        }
    }

    for (_, config_rel) in FIREFOX_FAMILY {
        let config_root = home.join(config_rel);
        for profile in firefox_profiles(&config_root) {
            for sub in FIREFOX_STATE_PATHS {
                push_delete(&mut actions, profile.join(sub), DeletionVia::Trash);
            }
        }
    }

    actions
}

// ─── Cleaner: user.browser_cache ──────────────────────────────────────────────

struct BrowserCacheMeta;

impl CleanerMeta for BrowserCacheMeta {
    fn id(&self) -> CleanerId {
        CleanerId::new("user.browser_cache")
    }
    fn name(&self) -> &str {
        "Browser caches"
    }
    fn description(&self) -> &str {
        "HTTP / GPU / code / shader / startup caches for Firefox- and Chromium-family browsers. \
         Browsers rebuild these on demand; safe to delete."
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

fn plan_cache<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        let Some(h) = home() else {
            return Ok(Vec::new());
        };
        Ok(collect_cache_actions(&h))
    })
}

static META_CACHE: BrowserCacheMeta = BrowserCacheMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY_CACHE: CleanerEntry = CleanerEntry {
    meta: &META_CACHE,
    plan: plan_cache,
};

// ─── Cleaner: user.browser_state ──────────────────────────────────────────────

struct BrowserStateMeta;

impl CleanerMeta for BrowserStateMeta {
    fn id(&self) -> CleanerId {
        CleanerId::new("user.browser_state")
    }
    fn name(&self) -> &str {
        "Browser site data"
    }
    fn description(&self) -> &str {
        "Cookies, open-tab sessions, Service Worker CacheStorage and site IndexedDB / \
         Local Storage. Deleting will log you out of websites, close saved tabs and reset \
         offline-capable web apps. Does NOT touch saved passwords, bookmarks, history, \
         extensions or autofill. Files are sent to the trash."
    }
    fn risk(&self) -> RiskLevel {
        RiskLevel::Dangerous
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

fn plan_state<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        let Some(h) = home() else {
            return Ok(Vec::new());
        };
        Ok(collect_state_actions(&h))
    })
}

static META_STATE: BrowserStateMeta = BrowserStateMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY_STATE: CleanerEntry = CleanerEntry {
    meta: &META_STATE,
    plan: plan_state,
};
