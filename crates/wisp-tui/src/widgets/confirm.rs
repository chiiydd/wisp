//! Modal confirmation dialog.

use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

pub enum ConfirmResult {
    Pending,
    Confirmed,
    Cancelled,
}

pub struct ConfirmDialog {
    message: String,
    /// If true, the user must type "yes" in full.
    require_typed: bool,
    input: String,
}

impl ConfirmDialog {
    pub fn new(message: String, require_typed: bool) -> Self {
        Self {
            message,
            require_typed,
            input: String::new(),
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let dialog_w = (area.width * 2 / 3).clamp(40, 70);
        let dialog_h = if self.require_typed { 9 } else { 7 };
        let x = area.x + (area.width.saturating_sub(dialog_w)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_h)) / 2;
        let dialog_rect = Rect {
            x,
            y,
            width: dialog_w,
            height: dialog_h,
        };

        f.render_widget(Clear, dialog_rect);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(Color::Red))
            .title(" Confirm ");

        let inner = block.inner(dialog_rect);
        f.render_widget(block, dialog_rect);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // message
                Constraint::Length(1), // spacer
                Constraint::Length(1), // input prompt (if needed)
                Constraint::Length(1), // spacer
                Constraint::Length(1), // y/n hint
            ])
            .split(inner);

        let msg = Paragraph::new(self.message.as_str())
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );
        f.render_widget(msg, chunks[0]);

        if self.require_typed {
            let prompt = Paragraph::new(Line::from(vec![
                Span::styled(
                    "Type 'yes' to confirm: ",
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    self.input.as_str(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("█", Style::default().fg(Color::Cyan)),
            ]));
            f.render_widget(prompt, chunks[2]);
        }

        let hint = Paragraph::new(if self.require_typed {
            "Enter to confirm   Esc to cancel"
        } else {
            "y / Enter to confirm   n / Esc to cancel"
        })
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
        f.render_widget(hint, chunks[4]);
    }

    pub fn handle_event(&mut self, evt: &Event) -> ConfirmResult {
        let Event::Key(k) = evt else {
            return ConfirmResult::Pending;
        };
        if k.kind != KeyEventKind::Press {
            return ConfirmResult::Pending;
        }

        if self.require_typed {
            match k.code {
                KeyCode::Char(c) => {
                    self.input.push(c);
                }
                KeyCode::Backspace => {
                    self.input.pop();
                }
                KeyCode::Enter if self.input.trim() == "yes" => {
                    return ConfirmResult::Confirmed;
                }
                KeyCode::Esc => return ConfirmResult::Cancelled,
                _ => {}
            }
        } else {
            match k.code {
                KeyCode::Char('y') | KeyCode::Enter => return ConfirmResult::Confirmed,
                KeyCode::Char('n') | KeyCode::Esc => return ConfirmResult::Cancelled,
                _ => {}
            }
        }
        ConfirmResult::Pending
    }
}
