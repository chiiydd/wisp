//! L5 – TUI presentation layer.
//!
//! Provides a full-screen terminal UI built on ratatui + crossterm.
//! Entry point: `run_tui()`.

pub mod app;
pub mod chrome;
pub mod pages;
pub mod theme;
pub mod widgets;

use std::sync::Arc;

use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use wisp_core::CoreResult;
use wisp_engine::{Engine, EngineConfig};
use wisp_platform::detect_distro;

use app::App;

/// Launch the full-screen TUI.
pub async fn run_tui() -> CoreResult<()> {
    let distro = Arc::from(detect_distro());
    let engine = Arc::new(Engine::new(EngineConfig::default(), distro));

    enable_raw_mode().map_err(wisp_core::CoreError::Io)?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(wisp_core::CoreError::Io)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)
        .map_err(|e| wisp_core::CoreError::Io(std::io::Error::other(e.to_string())))?;

    let result = App::new(engine).run(&mut terminal).await;

    // Always restore terminal even on error
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    result
}
