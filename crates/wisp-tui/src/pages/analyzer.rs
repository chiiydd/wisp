//! Disk analyzer page.
//!
//! Left pane: sorted directory list with sizes.
//! Right pane: simple bar chart for relative sizes.
//! Navigates with j/k/Enter/h (vi-style).

use std::sync::Arc;

use camino::Utf8PathBuf;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use humansize::{DECIMAL, format_size};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Gauge, List, ListItem, ListState, Paragraph,
};

use wisp_core::scanner::{ScanOptions, scan_directory};
use wisp_core::types::{ScanNode, ScanTree};
use wisp_engine::Engine;

use super::PageAction;

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    path: Utf8PathBuf,
    size: u64,
    is_dir: bool,
}

enum ScanState {
    Idle,
    Scanning,
    Done(ScanTree),
    Error(String),
}

pub struct AnalyzerPage {
    #[allow(dead_code)]
    engine: Arc<Engine>,
    root: Utf8PathBuf,
    entries: Vec<Entry>,
    list_state: ListState,
    scan_state: ScanState,
    total_size: u64,
}

impl AnalyzerPage {
    pub fn new(path: Utf8PathBuf, engine: Arc<Engine>) -> Self {
        let mut page = Self {
            engine,
            root: path.clone(),
            entries: Vec::new(),
            list_state: ListState::default(),
            scan_state: ScanState::Idle,
            total_size: 0,
        };
        page.start_scan(path);
        page
    }

    fn start_scan(&mut self, path: Utf8PathBuf) {
        self.root = path.clone();
        self.entries.clear();
        self.total_size = 0;
        self.scan_state = ScanState::Scanning;
        // Scan is triggered async in tick() on the first idle poll.
        // We store the path so tick() can pick it up.
    }

    pub fn tick(&mut self) {
        // Trigger blocking scan on first tick after Scanning state is set.
        // We use std::thread here to avoid needing async in tick.
        if matches!(self.scan_state, ScanState::Scanning) {
            self.scan_state = ScanState::Idle; // prevent re-entry

            let root = self.root.clone();
            // Run synchronous scan; we accept blocking the event loop briefly
            // for simplicity. A proper async version would use tokio channels.
            let opts = ScanOptions { max_depth: Some(1), min_size: None, follow_symlinks: false };
            match std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .build()
                    .expect("tokio rt");
                rt.block_on(scan_directory(root, opts))
            }).join() {
                Ok(Ok(tree)) => {
                    self.load_tree(&tree);
                    self.scan_state = ScanState::Done(tree);
                }
                Ok(Err(e)) => {
                    self.scan_state = ScanState::Error(e.to_string());
                }
                Err(_) => {
                    self.scan_state = ScanState::Error("scan thread panicked".into());
                }
            }
        }
    }

    fn load_tree(&mut self, tree: &ScanTree) {
        let root_key = match tree.root {
            Some(k) => k,
            None => return,
        };
        let root_node = &tree.nodes[root_key];
        self.total_size = root_node.size;

        let mut entries: Vec<Entry> = root_node
            .children
            .iter()
            .map(|&ck| {
                let node: &ScanNode = &tree.nodes[ck];
                Entry {
                    name: node
                        .path
                        .file_name()
                        .unwrap_or(node.path.as_str())
                        .to_owned(),
                    path: node.path.clone(),
                    size: node.size,
                    is_dir: node.is_dir,
                }
            })
            .collect();

        entries.sort_by(|a, b| b.size.cmp(&a.size));
        self.entries = entries;

        if !self.entries.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(2)])
            .split(area);

        // ── Breadcrumb bar ────────────────────────────────────────────────
        let breadcrumb = Paragraph::new(Line::from(vec![
            Span::styled(" Analyze: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                self.root.as_str().to_owned(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ({})", format_size(self.total_size, DECIMAL)),
                Style::default().fg(Color::Yellow),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        );
        f.render_widget(breadcrumb, chunks[0]);

        match &self.scan_state {
            ScanState::Idle | ScanState::Scanning => {
                let spinner = Paragraph::new(" Scanning…")
                    .style(Style::default().fg(Color::Yellow));
                f.render_widget(spinner, chunks[1]);
            }
            ScanState::Error(e) => {
                let msg =
                    Paragraph::new(format!(" Error: {e}")).style(Style::default().fg(Color::Red));
                f.render_widget(msg, chunks[1]);
            }
            ScanState::Done(_) => {
                self.render_entries(f, chunks[1]);
            }
        }

        let footer = Paragraph::new(
            " j/↓ k/↑ move   Enter/l enter dir   h/← back   q back  ",
        )
        .style(Style::default().fg(Color::DarkGray));
        f.render_widget(footer, chunks[2]);
    }

    fn render_entries(&mut self, f: &mut Frame, area: Rect) {
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);

        // ── Left: sorted list ─────────────────────────────────────────────
        let items: Vec<ListItem> = self
            .entries
            .iter()
            .map(|e| {
                let size_str = format!("{:>10}", format_size(e.size, DECIMAL));
                let mark = if e.is_dir { "/" } else { "" };
                ListItem::new(Line::from(vec![
                    Span::styled(size_str, Style::default().fg(Color::Yellow)),
                    Span::raw("  "),
                    Span::styled(
                        format!("{}{mark}", e.name),
                        if e.is_dir {
                            Style::default().fg(Color::Cyan)
                        } else {
                            Style::default()
                        },
                    ),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Files & Dirs "),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        f.render_stateful_widget(list, panes[0], &mut self.list_state);

        // ── Right: bar chart (top 10) ────────────────────────────────────
        let bar_area = panes[1];
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Size distribution ");
        let inner = block.inner(bar_area);
        f.render_widget(block, bar_area);

        if self.total_size == 0 || self.entries.is_empty() {
            return;
        }

        let n = self.entries.len().min(inner.height as usize);
        if n == 0 {
            return;
        }

        let row_h = 1u16;
        for (i, entry) in self.entries.iter().take(n).enumerate() {
            let ratio = entry.size as f64 / self.total_size as f64;
            let label = format!(
                "{} {}",
                format_size(entry.size, DECIMAL),
                if entry.is_dir {
                    format!("{}/", entry.name)
                } else {
                    entry.name.clone()
                }
            );
            let gauge_rect = Rect {
                x: inner.x,
                y: inner.y + i as u16 * row_h,
                width: inner.width,
                height: row_h,
            };
            let gauge = Gauge::default()
                .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
                .ratio(ratio.min(1.0))
                .label(label);
            f.render_widget(gauge, gauge_rect);
        }
    }

    pub async fn handle_event(&mut self, evt: &Event) -> PageAction {
        let Event::Key(k) = evt else { return PageAction::None };
        if k.kind != KeyEventKind::Press {
            return PageAction::None;
        }
        match k.code {
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.entries.is_empty() {
                    let i = self.list_state.selected().unwrap_or(0);
                    self.list_state.select(Some((i + 1) % self.entries.len()));
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if !self.entries.is_empty() {
                    let i = self.list_state.selected().unwrap_or(0);
                    self.list_state.select(Some(
                        i.checked_sub(1).unwrap_or(self.entries.len() - 1),
                    ));
                }
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                if let Some(sel) = self.list_state.selected() {
                    if let Some(entry) = self.entries.get(sel) {
                        if entry.is_dir {
                            self.start_scan(entry.path.clone());
                        }
                    }
                }
            }
            KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left => {
                if let Some(parent) = self.root.parent().map(Utf8PathBuf::from) {
                    if parent != self.root {
                        self.start_scan(parent);
                        return PageAction::None;
                    }
                }
                return PageAction::Pop;
            }
            KeyCode::Char('q') | KeyCode::Esc => return PageAction::Pop,
            _ => {}
        }
        PageAction::None
    }
}
