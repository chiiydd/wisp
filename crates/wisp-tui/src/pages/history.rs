//! History page – shows past clean sessions.

use crossterm::event::{Event, KeyCode, KeyEventKind};
use humansize::{DECIMAL, format_size};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};

use wisp_core::types::CleanReport;
use wisp_engine::history;

use super::PageAction;

pub struct HistoryPage {
    items: Vec<CleanReport>,
    list_state: ListState,
}

impl HistoryPage {
    pub fn new() -> Self {
        let items = history::read(50);
        let mut list_state = ListState::default();
        if !items.is_empty() {
            list_state.select(Some(0));
        }
        Self { items, list_state }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(area);

        let list_items: Vec<ListItem> = self
            .items
            .iter()
            .map(|h| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:>12}  ", format_size(h.bytes_freed, DECIMAL)),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(
                        format!("✓{:3} ", h.succeeded),
                        Style::default().fg(Color::Green),
                    ),
                    Span::styled(
                        format!("✗{:3} ", h.failed),
                        Style::default().fg(Color::Red),
                    ),
                    Span::styled(
                        format!("–{:3} ", h.skipped),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        h.plan_id.to_string(),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]))
            })
            .collect();

        let title = format!(" History ({} sessions) ", self.items.len());
        let list = List::new(list_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(title),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        f.render_stateful_widget(list, chunks[0], &mut self.list_state);

        let footer = Paragraph::new(" j/↓ k/↑ navigate   q back ")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(footer, chunks[1]);
    }

    pub fn handle_event(&mut self, evt: &Event) -> PageAction {
        let Event::Key(k) = evt else { return PageAction::None };
        if k.kind != KeyEventKind::Press {
            return PageAction::None;
        }
        match k.code {
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.items.is_empty() {
                    let i = self.list_state.selected().unwrap_or(0);
                    self.list_state.select(Some((i + 1) % self.items.len()));
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if !self.items.is_empty() {
                    let i = self.list_state.selected().unwrap_or(0);
                    self.list_state
                        .select(Some(i.checked_sub(1).unwrap_or(self.items.len() - 1)));
                }
            }
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Backspace => {
                return PageAction::Pop;
            }
            _ => {}
        }
        PageAction::None
    }
}
