//! Main menu page.

use camino::Utf8PathBuf;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};

use super::{CleanGroup, PageAction};

const MENU_ITEMS: &[(&str, &str)] = &[
    ("  Analyze", "Explore disk usage interactively"),
    ("  Quick Clean (User)", "browser cache, trash, thumbnails"),
    ("  Quick Clean (System)", "pacman cache, journal, orphans"),
    ("  Quick Clean (Dev)", "cargo, npm, pip, go, docker"),
    ("  History", "View past clean sessions"),
    ("  Quit", "Exit wisp"),
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
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),  // title / banner
                Constraint::Min(10),    // menu
                Constraint::Length(2),  // footer
            ])
            .split(area);

        // ── Banner ──────────────────────────────────────────────────────────
        let banner_lines = vec![
            Line::from(Span::styled(
                " wisp  – modern disk cleanup for Arch Linux",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "  ↑/k  ↓/j  navigate    Enter  select    q  quit",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        let banner = Paragraph::new(banner_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .style(Style::default().fg(Color::Cyan)),
        );
        f.render_widget(banner, chunks[0]);

        // ── Menu ─────────────────────────────────────────────────────────────
        let items: Vec<ListItem> = MENU_ITEMS
            .iter()
            .map(|(label, desc)| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{label:<28}"), Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(*desc, Style::default().fg(Color::DarkGray)),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Main Menu "),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        f.render_stateful_widget(list, chunks[1], &mut self.list_state);

        // ── Footer ───────────────────────────────────────────────────────────
        let footer = Paragraph::new(Line::from(vec![
            Span::styled(" wisp ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(
                concat!("v", env!("CARGO_PKG_VERSION")),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                "   Ctrl-C / q to quit",
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        f.render_widget(footer, chunks[2]);
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
            KeyCode::Enter | KeyCode::Char('l') => {
                return self.activate();
            }
            KeyCode::Char('q') | KeyCode::Esc => return PageAction::Quit,
            _ => {}
        }
        PageAction::None
    }

    fn activate(&self) -> PageAction {
        match self.list_state.selected().unwrap_or(0) {
            0 => {
                // Analyze home dir by default
                let home = dirs::home_dir()
                    .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
                    .unwrap_or_else(|| Utf8PathBuf::from("/"));
                PageAction::PushAnalyzer(home)
            }
            1 => PageAction::PushCleaner(CleanGroup::User),
            2 => PageAction::PushCleaner(CleanGroup::System),
            3 => PageAction::PushCleaner(CleanGroup::Dev),
            4 => PageAction::PushHistory,
            _ => PageAction::Quit,
        }
    }
}
