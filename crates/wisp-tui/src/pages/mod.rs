//! Page router — each variant holds one full-screen page.
//!
//! Pages no longer draw their own header/footer chrome.  Instead they
//! implement `mode()`, `context()`, and `hints()`, which the App composes
//! into the title bar (top) and statusline (bottom).

pub mod analyzer;
pub mod cleaner;
pub mod history;
pub mod home;

use std::sync::Arc;

use camino::Utf8PathBuf;
use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::text::Span;

use wisp_engine::Engine;

use crate::chrome::KeyHint;

use self::analyzer::AnalyzerPage;
use self::cleaner::CleanerPage;
use self::history::HistoryPage;
use self::home::HomePage;

/// Which group to show in the cleaner page.
#[derive(Clone, Copy, Debug)]
pub enum CleanGroup {
    All,
    System,
    User,
    Dev,
    /// Bundled LinuxQQ targets — covers both `linuxqq_cache` (Safe) and
    /// `linuxqq_media` (Dangerous) so the user can review them together
    /// in the plan view and skip the dangerous half if they want.
    LinuxQq,
}

impl CleanGroup {
    /// Targets passed to `Engine::build_plan`. Returns a slice so a
    /// single menu entry can resolve to multiple cleaner ids — used for
    /// `LinuxQq`, which bundles two cleaners.
    pub fn as_targets(&self) -> &'static [&'static str] {
        match self {
            CleanGroup::All => &["@all"],
            CleanGroup::System => &["@system"],
            CleanGroup::User => &["@user"],
            CleanGroup::Dev => &["@dev"],
            CleanGroup::LinuxQq => &["linuxqq_cache", "linuxqq_media"],
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            CleanGroup::All => "All",
            CleanGroup::System => "System",
            CleanGroup::User => "User",
            CleanGroup::Dev => "Dev",
            CleanGroup::LinuxQq => "LinuxQQ",
        }
    }
}

/// Actions a page can return to the app loop.
pub enum PageAction {
    None,
    Quit,
    Push(Page),
    Pop,
    Replace(Page),
    PushAnalyzer(Utf8PathBuf),
    PushCleaner(CleanGroup),
    PushHistory,
}

/// The active full-screen page.
pub enum Page {
    Home(HomePage),
    Analyzer(AnalyzerPage),
    Cleaner(CleanerPage),
    History(HistoryPage),
}

impl Page {
    pub fn home() -> Self {
        Page::Home(HomePage::new())
    }
    pub fn analyzer(path: Utf8PathBuf, engine: Arc<Engine>) -> Self {
        Page::Analyzer(AnalyzerPage::new(path, engine))
    }
    pub fn cleaner(group: CleanGroup, engine: Arc<Engine>) -> Self {
        Page::Cleaner(CleanerPage::new(group, engine))
    }
    pub fn history() -> Self {
        Page::History(HistoryPage::new())
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        match self {
            Page::Home(p) => p.render(f, area),
            Page::Analyzer(p) => p.render(f, area),
            Page::Cleaner(p) => p.render(f, area),
            Page::History(p) => p.render(f, area),
        }
    }

    pub async fn handle_event(&mut self, evt: &Event) -> PageAction {
        match self {
            Page::Home(p) => p.handle_event(evt),
            Page::Analyzer(p) => p.handle_event(evt).await,
            Page::Cleaner(p) => p.handle_event(evt).await,
            Page::History(p) => p.handle_event(evt),
        }
    }

    pub fn tick(&mut self) {
        match self {
            Page::Analyzer(p) => p.tick(),
            Page::Cleaner(p) => p.tick(),
            _ => {}
        }
    }

    // ── Chrome contract ──────────────────────────────────────────────────────

    /// Page name shown on the right of the title bar.
    pub fn title(&self) -> &'static str {
        match self {
            Page::Home(_) => "Home",
            Page::Analyzer(_) => "Analyzer",
            Page::Cleaner(_) => "Cleaner",
            Page::History(_) => "History",
        }
    }

    /// Mode badge (left of statusline): label + background colour.
    pub fn mode(&self) -> (String, Color) {
        match self {
            Page::Home(p) => p.mode(),
            Page::Analyzer(p) => p.mode(),
            Page::Cleaner(p) => p.mode(),
            Page::History(p) => p.mode(),
        }
    }

    /// Middle of statusline: context info (path, counts, etc).
    pub fn context(&self) -> Vec<Span<'static>> {
        match self {
            Page::Home(p) => p.context(),
            Page::Analyzer(p) => p.context(),
            Page::Cleaner(p) => p.context(),
            Page::History(p) => p.context(),
        }
    }

    /// Right of statusline: keybinding hints for the current state.
    pub fn hints(&self) -> Vec<KeyHint> {
        match self {
            Page::Home(p) => p.hints(),
            Page::Analyzer(p) => p.hints(),
            Page::Cleaner(p) => p.hints(),
            Page::History(p) => p.hints(),
        }
    }
}
