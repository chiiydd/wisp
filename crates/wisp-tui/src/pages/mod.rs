//! Page router — each variant holds one full-screen page.

pub mod analyzer;
pub mod cleaner;
pub mod history;
pub mod home;

use std::sync::Arc;

use camino::Utf8PathBuf;
use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;

use wisp_engine::Engine;

use self::analyzer::AnalyzerPage;
use self::cleaner::CleanerPage;
use self::history::HistoryPage;
use self::home::HomePage;

/// Which group to show in the cleaner page.
#[derive(Clone, Debug)]
pub enum CleanGroup {
    All,
    System,
    User,
    Dev,
}

impl CleanGroup {
    pub fn as_target(&self) -> &'static str {
        match self {
            CleanGroup::All => "@all",
            CleanGroup::System => "@system",
            CleanGroup::User => "@user",
            CleanGroup::Dev => "@dev",
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
}
