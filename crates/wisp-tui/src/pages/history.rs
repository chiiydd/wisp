//! History page — list of past sessions, with selectable detail view.

use crossterm::event::{Event, KeyCode, KeyEventKind};
use humansize::{DECIMAL, format_size};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};

use wisp_core::types::CleanReport;
use wisp_engine::history;

use crate::chrome::KeyHint;
use crate::theme::Theme;

use super::PageAction;

// ─── View state ───────────────────────────────────────────────────────────────

enum HistView { List, Detail }

// ─── Page ────────────────────────────────────────────────────────────────────

pub struct HistoryPage {
    items: Vec<CleanReport>,
    list_state: ListState,
    view: HistView,
}

impl HistoryPage {
    pub fn new() -> Self {
        let items = history::read(50);
        let mut list_state = ListState::default();
        if !items.is_empty() {
            list_state.select(Some(0));
        }
        Self { items, list_state, view: HistView::List }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        match self.view {
            HistView::List   => self.render_list(f, area),
            HistView::Detail => self.render_detail(f, area),
        }
    }

    // ── List ──────────────────────────────────────────────────────────────────

    fn render_list(&mut self, f: &mut Frame, area: Rect) {
        if self.items.is_empty() {
            f.render_widget(
                Paragraph::new(" No past sessions yet — run a clean to populate this list.")
                    .style(Style::default().fg(Theme::MUTED))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(Theme::ACCENT_DIM)),
                    ),
                area,
            );
            return;
        }

        let list_items: Vec<ListItem> = self
            .items
            .iter()
            .map(|h| {
                let age = format_age(h.timestamp);
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:>12}  ", format_size(h.bytes_freed, DECIMAL)),
                        Style::default().fg(Theme::WARNING),
                    ),
                    Span::styled(format!("✓{:<4}", h.succeeded), Style::default().fg(Theme::SUCCESS)),
                    Span::styled(
                        format!("✗{:<4}", h.failed),
                        Style::default().fg(if h.failed > 0 { Theme::DANGER } else { Theme::MUTED }),
                    ),
                    Span::styled(format!("–{:<4}", h.skipped), Style::default().fg(Theme::MUTED)),
                    Span::styled(age, Style::default().fg(Theme::MUTED)),
                ]))
            })
            .collect();

        let list = List::new(list_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Theme::ACCENT_DIM))
                    .title(Span::styled(
                        " ⟳ Sessions ",
                        Style::default().fg(Theme::ACCENT).add_modifier(Modifier::BOLD),
                    )),
            )
            .highlight_style(Theme::selection())
            .highlight_symbol("▶ ");

        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    // ── Detail ────────────────────────────────────────────────────────────────

    fn render_detail(&mut self, f: &mut Frame, area: Rect) {
        let entry = match self.list_state.selected().and_then(|i| self.items.get(i)) {
            Some(r) => r,
            None => {
                f.render_widget(
                    Paragraph::new(" No entry selected.").style(Style::default().fg(Theme::MUTED)),
                    area,
                );
                return;
            }
        };

        let total = entry.succeeded + entry.failed + entry.skipped;
        let success_pct = if total > 0 { entry.succeeded * 100 / total } else { 0 };
        let border_color = if entry.failed == 0 { Theme::SUCCESS } else { Theme::WARNING };

        let date_str = format_utc(entry.timestamp);
        let id_str = format!("{:.8}…", entry.plan_id);

        let lines = vec![
            Line::from(""),
            kv("Date",    &date_str,                   Theme::FG),
            kv("Plan ID", &id_str,                     Theme::MUTED),
            Line::from(""),
            kv_bold("Freed", &format_size(entry.bytes_freed, DECIMAL), Theme::ACCENT),
            Line::from(""),
            stat_line("✓", entry.succeeded, "succeeded", Theme::SUCCESS, false),
            stat_line(
                "✗", entry.failed, "failed",
                if entry.failed > 0 { Theme::DANGER } else { Theme::MUTED },
                entry.failed > 0,
            ),
            stat_line("–", entry.skipped, "skipped", Theme::MUTED, false),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("Total {total} actions   {success_pct}% succeeded"),
                    Style::default().fg(Theme::MUTED),
                ),
            ]),
        ];

        let idx = self.list_state.selected().map(|i| i + 1).unwrap_or(0);
        let title = format!(" Session {idx} of {} ", self.items.len());

        f.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(border_color))
                    .title(Span::styled(title, Style::default().fg(border_color).add_modifier(Modifier::BOLD))),
            ),
            area,
        );
    }

    // ── Event handling ────────────────────────────────────────────────────────

    pub fn handle_event(&mut self, evt: &Event) -> PageAction {
        let Event::Key(k) = evt else { return PageAction::None };
        if k.kind != KeyEventKind::Press {
            return PageAction::None;
        }
        match self.view {
            HistView::List   => self.handle_list(k.code),
            HistView::Detail => self.handle_detail(k.code),
        }
    }

    fn handle_list(&mut self, code: KeyCode) -> PageAction {
        match code {
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.items.is_empty() {
                    let i = self.list_state.selected().unwrap_or(0);
                    self.list_state.select(Some((i + 1) % self.items.len()));
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if !self.items.is_empty() {
                    let i = self.list_state.selected().unwrap_or(0);
                    self.list_state.select(Some(
                        i.checked_sub(1).unwrap_or(self.items.len() - 1),
                    ));
                }
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                if !self.items.is_empty() {
                    self.view = HistView::Detail;
                }
            }
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Backspace => return PageAction::Pop,
            _ => {}
        }
        PageAction::None
    }

    fn handle_detail(&mut self, code: KeyCode) -> PageAction {
        match code {
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Right => {
                if !self.items.is_empty() {
                    let i = self.list_state.selected().unwrap_or(0);
                    self.list_state.select(Some((i + 1) % self.items.len()));
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if !self.items.is_empty() {
                    let i = self.list_state.selected().unwrap_or(0);
                    self.list_state.select(Some(
                        i.checked_sub(1).unwrap_or(self.items.len() - 1),
                    ));
                }
            }
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Char('q') | KeyCode::Esc | KeyCode::Backspace => {
                self.view = HistView::List;
            }
            _ => {}
        }
        PageAction::None
    }

    // ── Chrome contract ──────────────────────────────────────────────────────

    pub fn mode(&self) -> (String, Color) {
        match self.view {
            HistView::List   => ("HISTORY".into(), Theme::MODE_NORMAL),
            HistView::Detail => ("DETAIL".into(),  Theme::MODE_DETAIL),
        }
    }

    pub fn context(&self) -> Vec<Span<'static>> {
        if self.items.is_empty() {
            return vec![
                Span::styled("no sessions yet", Style::default().fg(Theme::MUTED)),
            ];
        }
        let total = self.items.len();
        let idx = self.list_state.selected().map(|i| i + 1).unwrap_or(0);
        vec![
            Span::styled(
                format!("session {idx} of {total}"),
                Style::default().fg(Theme::FG_DIM),
            ),
        ]
    }

    pub fn hints(&self) -> Vec<KeyHint> {
        match self.view {
            HistView::List => vec![
                KeyHint::new("j/k", "move"),
                KeyHint::new("⏎",   "details"),
                KeyHint::new("q",   "back"),
            ],
            HistView::Detail => vec![
                KeyHint::new("j/k", "next/prev"),
                KeyHint::new("h",   "back"),
                KeyHint::new("q",   "back"),
            ],
        }
    }
}

// ─── Detail-view helpers ─────────────────────────────────────────────────────

fn kv(key: &str, value: &str, value_color: Color) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{key:<9}"),
            Style::default().fg(Theme::MUTED),
        ),
        Span::styled(value.to_owned(), Style::default().fg(value_color)),
    ])
}

fn kv_bold(key: &str, value: &str, value_color: Color) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{key:<9}"),
            Style::default().fg(Theme::MUTED),
        ),
        Span::styled(
            value.to_owned(),
            Style::default().fg(value_color).add_modifier(Modifier::BOLD),
        ),
    ])
}

fn stat_line(icon: &str, n: usize, label: &str, color: Color, bold: bool) -> Line<'static> {
    let mut style = Style::default().fg(color);
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{icon}  {n:>5}  {label}"), style),
    ])
}

// ─── Time helpers ─────────────────────────────────────────────────────────────

fn format_age(ts: u64) -> String {
    if ts == 0 { return "—".to_owned(); }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let age = now.saturating_sub(ts);
    if age < 60            { return "just now".to_owned(); }
    if age < 3600          { return format!("{} min ago", age / 60); }
    if age < 86_400        { return format!("{} hr ago",  age / 3600); }
    if age < 86_400 * 7    { return format!("{} days ago", age / 86_400); }
    if age < 86_400 * 30   { return format!("{} weeks ago", age / (86_400 * 7)); }
    if age < 86_400 * 365  { return format!("{} months ago", age / (86_400 * 30)); }
    format!("{} years ago", age / (86_400 * 365))
}

fn format_utc(ts: u64) -> String {
    if ts == 0 { return "—".to_owned(); }
    let secs_in_day = ts % 86_400;
    let days = (ts / 86_400) as u32;
    let (y, mo, d) = days_to_ymd(days);
    let h  = secs_in_day / 3600;
    let mi = (secs_in_day % 3600) / 60;
    format!("{y:04}-{mo:02}-{d:02}  {h:02}:{mi:02} UTC")
}

fn days_to_ymd(days: u32) -> (u32, u32, u32) {
    let z      = days as i64 + 719_468;
    let era    = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe    = (z - era * 146_097) as u64;
    let yoe    = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y      = yoe as i64 + era * 400;
    let doy    = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp     = (5 * doy + 2) / 153;
    let d      = doy - (153 * mp + 2) / 5 + 1;
    let m      = if mp < 10 { mp + 3 } else { mp - 9 };
    let y      = if m <= 2 { y + 1 } else { y };
    (y as u32, m as u32, d as u32)
}
