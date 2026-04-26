//! Central color palette for the TUI.
//!
//! All pages should pull colors from `Theme::*` so the look stays coherent
//! and is easy to retheme later.

use ratatui::style::{Color, Modifier, Style};

pub struct Theme;

impl Theme {
    // ── Brand ────────────────────────────────────────────────────────────────
    pub const ACCENT:      Color = Color::Cyan;
    pub const ACCENT_DIM:  Color = Color::Rgb(80, 130, 150);

    // ── Semantic ─────────────────────────────────────────────────────────────
    pub const SUCCESS:     Color = Color::Green;
    pub const WARNING:     Color = Color::Yellow;
    pub const DANGER:      Color = Color::Red;
    pub const INFO:        Color = Color::Blue;
    pub const MARK:        Color = Color::Magenta;

    // ── Neutral ──────────────────────────────────────────────────────────────
    pub const FG:          Color = Color::White;
    pub const FG_DIM:      Color = Color::Gray;
    pub const MUTED:       Color = Color::DarkGray;
    pub const BG:          Color = Color::Reset;

    // ── Selection ────────────────────────────────────────────────────────────
    pub const SEL_FG:      Color = Color::Black;
    pub const SEL_BG:      Color = Color::Cyan;

    // ── Mode-badge backgrounds ───────────────────────────────────────────────
    pub const MODE_NORMAL: Color = Color::Cyan;
    pub const MODE_BUSY:   Color = Color::Yellow;
    pub const MODE_DANGER: Color = Color::Red;
    pub const MODE_DONE:   Color = Color::Green;
    pub const MODE_DETAIL: Color = Color::Magenta;

    // ── Pre-built styles ─────────────────────────────────────────────────────
    pub fn accent_bold() -> Style {
        Style::default().fg(Self::ACCENT).add_modifier(Modifier::BOLD)
    }
    pub fn muted() -> Style {
        Style::default().fg(Self::MUTED)
    }
    pub fn selection() -> Style {
        Style::default().fg(Self::SEL_FG).bg(Self::SEL_BG).add_modifier(Modifier::BOLD)
    }
    pub fn danger_bold() -> Style {
        Style::default().fg(Self::DANGER).add_modifier(Modifier::BOLD)
    }
}
