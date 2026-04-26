//! Main menu page.
//!
//! Body is just the centered menu list — top bar and statusline come from
//! the app chrome (`crate::chrome`).

use camino::Utf8PathBuf;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState};

use crate::chrome::KeyHint;
use crate::theme::Theme;

use super::{CleanGroup, PageAction};

/// One row in the home menu — the action it triggers is attached so the
/// activate logic doesn't depend on the row's index.
struct MenuItem {
    label:  &'static str,
    desc:   &'static str,
    action: MenuAction,
}

#[derive(Clone, Copy)]
enum MenuAction {
    Analyze,
    Clean(CleanGroup),
    History,
    Quit,
}

const MENU_ITEMS: &[MenuItem] = &[
    MenuItem { label: "Analyze",              desc: "Explore disk usage interactively",   action: MenuAction::Analyze },
    MenuItem { label: "Quick Clean (User)",   desc: "browser cache · trash · thumbnails", action: MenuAction::Clean(CleanGroup::User) },
    MenuItem { label: "Quick Clean (System)", desc: "pacman cache · journal · orphans",   action: MenuAction::Clean(CleanGroup::System) },
    MenuItem { label: "Quick Clean (Dev)",    desc: "cargo · npm · pip · go · docker",    action: MenuAction::Clean(CleanGroup::Dev) },
    MenuItem { label: "History",              desc: "View past clean sessions",           action: MenuAction::History },
    MenuItem { label: "Quit",                 desc: "Exit wisp",                          action: MenuAction::Quit },
];

pub struct HomePage {
    list_state: ListState,
}

impl HomePage {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self { list_state }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        // Centre the menu vertically; cap width.
        let menu_h = (MENU_ITEMS.len() * 2 + 2) as u16;
        let menu_w = 64u16.min(area.width.saturating_sub(4));
        let pad_v  = area.height.saturating_sub(menu_h) / 2;
        let pad_h  = (area.width.saturating_sub(menu_w)) / 2;

        let v = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(pad_v),
                Constraint::Length(menu_h),
                Constraint::Min(0),
            ])
            .split(area);
        let h = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(pad_h),
                Constraint::Length(menu_w),
                Constraint::Min(0),
            ])
            .split(v[1]);

        let items: Vec<ListItem> = MENU_ITEMS
            .iter()
            .map(|item| {
                ListItem::new(vec![
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            item.label.to_owned(),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(vec![
                        Span::raw("    "),
                        Span::styled(item.desc, Style::default().fg(Theme::MUTED)),
                    ]),
                ])
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Theme::ACCENT_DIM))
                    .title(Span::styled(
                        " ✦ Main menu ",
                        Style::default().fg(Theme::ACCENT).add_modifier(Modifier::BOLD),
                    )),
            )
            .highlight_style(Theme::selection())
            .highlight_symbol("▶ ");

        f.render_stateful_widget(list, h[1], &mut self.list_state);
    }

    pub fn handle_event(&mut self, evt: &Event) -> PageAction {
        let Event::Key(k) = evt else { return PageAction::None };
        if k.kind != KeyEventKind::Press {
            return PageAction::None;
        }
        match k.code {
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.list_state.selected().unwrap_or(0);
                self.list_state.select(Some((i + 1) % MENU_ITEMS.len()));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.list_state.selected().unwrap_or(0);
                self.list_state.select(Some(
                    i.checked_sub(1).unwrap_or(MENU_ITEMS.len() - 1),
                ));
            }
            KeyCode::Enter | KeyCode::Char('l') => return self.activate(),
            KeyCode::Char('q') | KeyCode::Esc => return PageAction::Quit,
            _ => {}
        }
        PageAction::None
    }

    fn activate(&self) -> PageAction {
        let action = MENU_ITEMS
            .get(self.list_state.selected().unwrap_or(0))
            .map(|item| item.action)
            .unwrap_or(MenuAction::Quit);
        match action {
            MenuAction::Analyze => {
                let home = dirs::home_dir()
                    .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
                    .unwrap_or_else(|| Utf8PathBuf::from("/"));
                PageAction::PushAnalyzer(home)
            }
            MenuAction::Clean(group) => PageAction::PushCleaner(group),
            MenuAction::History      => PageAction::PushHistory,
            MenuAction::Quit         => PageAction::Quit,
        }
    }

    // ── Chrome contract ──────────────────────────────────────────────────────

    pub fn mode(&self) -> (String, Color) {
        ("MENU".into(), Theme::MODE_NORMAL)
    }

    pub fn context(&self) -> Vec<Span<'static>> {
        let label = MENU_ITEMS
            .get(self.list_state.selected().unwrap_or(0))
            .map(|item| item.label)
            .unwrap_or("");
        vec![
            Span::styled(
                label.to_owned(),
                Style::default().fg(Theme::ACCENT).add_modifier(Modifier::BOLD),
            ),
        ]
    }

    pub fn hints(&self) -> Vec<KeyHint> {
        vec![
            KeyHint::new("j/k", "move"),
            KeyHint::new("⏎",   "select"),
            KeyHint::new("q",   "quit"),
        ]
    }
}
