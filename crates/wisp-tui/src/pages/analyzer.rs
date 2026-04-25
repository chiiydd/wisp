//! Disk analyzer page.
//!
//! ## Performance design
//!
//! A single recursive scan is launched for the chosen root.  Once it
//! completes, the full `ScanTree` is kept in memory as a `CachedScan` that
//! also carries a `HashMap<path → ScanKey>` index.
//!
//! Every subsequent navigation (drill-down or back) is a pure HashMap lookup
//! followed by iterating the children Vec — **zero I/O, O(1) per step**.
//!
//! A new scan is only started when the user navigates outside the cached
//! root (rare) or presses `r` to force a rescan.
//!
//! The scan itself runs in a dedicated rayon thread pool
//! (`jwalk::Parallelism::RayonNewPool`) so it doesn't compete with tokio.
//! Results arrive over an `mpsc` channel; `tick()` does a non-blocking
//! `try_recv()` each frame.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

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

use wisp_core::CoreResult;
use wisp_core::scanner::{ScanOptions, scan_directory};
use wisp_core::types::{ScanKey, ScanTree};
use wisp_engine::Engine;

use super::PageAction;

// ─── Entry ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    path: Utf8PathBuf,
    size: u64,
    is_dir: bool,
}

// ─── CachedScan ──────────────────────────────────────────────────────────────

/// A fully-scanned directory tree with a path→key index for O(1) navigation.
struct CachedScan {
    tree: ScanTree,
    /// Maps every node's absolute path to its `ScanKey`.
    index: HashMap<Utf8PathBuf, ScanKey>,
}

impl CachedScan {
    fn new(tree: ScanTree) -> Self {
        let index: HashMap<Utf8PathBuf, ScanKey> = tree
            .nodes
            .iter()
            .map(|(k, n)| (n.path.clone(), k))
            .collect();
        Self { tree, index }
    }

    /// Return sorted children entries and total size for `path`, if present.
    fn entries_for(&self, path: &Utf8PathBuf) -> Option<(Vec<Entry>, u64)> {
        let &key = self.index.get(path)?;
        let node = &self.tree.nodes[key];

        let mut entries: Vec<Entry> = node
            .children
            .iter()
            .map(|&ck| {
                let child = &self.tree.nodes[ck];
                Entry {
                    name: child
                        .path
                        .file_name()
                        .unwrap_or(child.path.as_str())
                        .to_owned(),
                    path: child.path.clone(),
                    size: child.size,
                    is_dir: child.is_dir,
                }
            })
            .collect();

        entries.sort_by(|a, b| b.size.cmp(&a.size));
        Some((entries, node.size))
    }
}

// ─── ScanState ───────────────────────────────────────────────────────────────

enum ScanState {
    /// Background scan in progress; result arrives on `rx`.
    Scanning(Receiver<CoreResult<ScanTree>>),
    /// Fully scanned and indexed.  All navigation is O(1).
    Ready(CachedScan),
    Error(String),
}

// ─── AnalyzerPage ────────────────────────────────────────────────────────────

pub struct AnalyzerPage {
    #[allow(dead_code)]
    engine: Arc<Engine>,
    /// Root used for the current scan.
    scan_root: Utf8PathBuf,
    /// Directory currently on screen (may be scan_root or any descendant).
    current_path: Utf8PathBuf,
    /// Stack of previously viewed paths — used by h/← back navigation.
    nav_stack: Vec<Utf8PathBuf>,
    scan_state: ScanState,
    /// Currently displayed children, sorted by size descending.
    entries: Vec<Entry>,
    list_state: ListState,
    total_size: u64,
    tick_count: usize,
}

impl AnalyzerPage {
    pub fn new(path: Utf8PathBuf, engine: Arc<Engine>) -> Self {
        let scan_state = Self::launch_scan(path.clone());
        Self {
            engine,
            scan_root: path.clone(),
            current_path: path,
            nav_stack: Vec::new(),
            scan_state,
            entries: Vec::new(),
            list_state: ListState::default(),
            total_size: 0,
            tick_count: 0,
        }
    }

    // ── Scan management ──────────────────────────────────────────────────────

    /// Spawn a recursive background scan.  Non-blocking; result is polled by `tick()`.
    fn launch_scan(root: Utf8PathBuf) -> ScanState {
        let (tx, rx) = mpsc::channel::<CoreResult<ScanTree>>();
        std::thread::spawn(move || {
            let opts = ScanOptions { max_depth: None, min_size: None, follow_symlinks: false };
            // Run inside a lightweight single-thread tokio runtime.  jwalk
            // uses its own rayon pool so actual I/O is parallel.
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio rt");
            let _ = tx.send(rt.block_on(scan_directory(root, opts)));
        });
        ScanState::Scanning(rx)
    }

    /// Non-blocking poll; transitions to `Ready` when the scan completes.
    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);

        let outcome = if let ScanState::Scanning(rx) = &self.scan_state {
            match rx.try_recv() {
                Ok(r) => Some(r),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(wisp_core::CoreError::Io(
                    std::io::Error::new(std::io::ErrorKind::BrokenPipe, "scan thread exited"),
                ))),
            }
        } else {
            None
        };

        if let Some(result) = outcome {
            match result {
                Ok(tree) => {
                    let cached = CachedScan::new(tree);
                    if let Some((entries, total)) = cached.entries_for(&self.current_path) {
                        self.entries = entries;
                        self.total_size = total;
                        if !self.entries.is_empty() {
                            self.list_state.select(Some(0));
                        }
                    }
                    self.scan_state = ScanState::Ready(cached);
                }
                Err(e) => {
                    self.scan_state = ScanState::Error(e.to_string());
                }
            }
        }
    }

    // ── Navigation ───────────────────────────────────────────────────────────

    /// Enter a child directory — O(1) if already cached.
    fn navigate_into(&mut self, new_path: Utf8PathBuf) {
        match &self.scan_state {
            ScanState::Ready(cached) => {
                if let Some((entries, total)) = cached.entries_for(&new_path) {
                    self.nav_stack.push(self.current_path.clone());
                    self.current_path = new_path;
                    self.entries = entries;
                    self.total_size = total;
                    self.list_state
                        .select(if self.entries.is_empty() { None } else { Some(0) });
                    return;
                }
                // Path not in the cached tree — shouldn't happen with full-depth
                // scan, but fall back to a fresh scan (e.g. mount-point appeared).
                self.start_fresh_scan(new_path);
            }
            // Still scanning — ignore navigation until ready
            _ => {}
        }
    }

    /// Go back to the previous directory — O(1) cache lookup.
    /// Returns `Pop` if the nav stack is empty (exit the page).
    fn navigate_back(&mut self) -> PageAction {
        if let Some(prev) = self.nav_stack.pop() {
            // prev is guaranteed to be inside the cached tree
            if let ScanState::Ready(cached) = &self.scan_state {
                if let Some((entries, total)) = cached.entries_for(&prev) {
                    self.current_path = prev;
                    self.entries = entries;
                    self.total_size = total;
                    self.list_state
                        .select(if self.entries.is_empty() { None } else { Some(0) });
                }
            }
            PageAction::None
        } else {
            PageAction::Pop
        }
    }

    /// Force rescan of the current path (user pressed `r`).
    fn start_fresh_scan(&mut self, path: Utf8PathBuf) {
        self.nav_stack.clear();
        self.scan_root = path.clone();
        self.current_path = path.clone();
        self.entries.clear();
        self.total_size = 0;
        self.scan_state = Self::launch_scan(path);
    }

    // ── Rendering ────────────────────────────────────────────────────────────

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(2)])
            .split(area);

        // ── Breadcrumb ────────────────────────────────────────────────────
        let scanning = matches!(self.scan_state, ScanState::Scanning(_));
        let status = if scanning {
            let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let s = spinner_chars[self.tick_count % spinner_chars.len()];
            format!(" {s} scanning…")
        } else {
            format!("  ({})", format_size(self.total_size, DECIMAL))
        };

        let breadcrumb = Paragraph::new(Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(
                self.current_path.as_str().to_owned(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(status, Style::default().fg(Color::Yellow)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Analyze "),
        );
        f.render_widget(breadcrumb, chunks[0]);

        // ── Body ─────────────────────────────────────────────────────────
        match &self.scan_state {
            ScanState::Scanning(_) if self.entries.is_empty() => {
                let msg = Paragraph::new(
                    " Scanning recursively in background — results will appear automatically…",
                )
                .style(Style::default().fg(Color::DarkGray));
                f.render_widget(msg, chunks[1]);
            }
            ScanState::Error(e) => {
                let e = e.clone();
                let msg = Paragraph::new(format!(" Error: {e}"))
                    .style(Style::default().fg(Color::Red));
                f.render_widget(msg, chunks[1]);
            }
            _ => {
                self.render_entries(f, chunks[1]);
            }
        }

        // ── Footer ────────────────────────────────────────────────────────
        let depth = self.nav_stack.len();
        let back_hint = if depth > 0 {
            format!(" h/← back ({depth} levels)  ")
        } else {
            "  h/← exit  ".into()
        };
        let footer = Paragraph::new(Line::from(vec![
            Span::styled(
                " j/↓ k/↑ move  Enter/l enter  ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(back_hint, Style::default().fg(Color::DarkGray)),
            Span::styled(" r rescan  q quit ", Style::default().fg(Color::DarkGray)),
        ]));
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

        let count = self.entries.len();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(format!(" {count} items ")),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        f.render_stateful_widget(list, panes[0], &mut self.list_state);

        // ── Right: proportional bar chart ────────────────────────────────
        let bar_area = panes[1];
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Size distribution ");
        let inner = block.inner(bar_area);
        f.render_widget(block, bar_area);

        if self.total_size == 0 || self.entries.is_empty() || inner.height == 0 {
            return;
        }

        let n = self.entries.len().min(inner.height as usize);
        for (i, entry) in self.entries.iter().take(n).enumerate() {
            let ratio = entry.size as f64 / self.total_size as f64;
            let label = format!(
                "{} {}{}",
                format_size(entry.size, DECIMAL),
                entry.name,
                if entry.is_dir { "/" } else { "" },
            );
            let gauge_rect = Rect {
                x: inner.x,
                y: inner.y + i as u16,
                width: inner.width,
                height: 1,
            };
            let gauge = Gauge::default()
                .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
                .ratio(ratio.min(1.0))
                .label(label);
            f.render_widget(gauge, gauge_rect);
        }
    }

    // ── Event handling ───────────────────────────────────────────────────────

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
                    self.list_state
                        .select(Some(i.checked_sub(1).unwrap_or(self.entries.len() - 1)));
                }
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                if let Some(sel) = self.list_state.selected() {
                    if let Some(entry) = self.entries.get(sel) {
                        if entry.is_dir {
                            let path = entry.path.clone();
                            self.navigate_into(path);
                        }
                    }
                }
            }
            KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left => {
                return self.navigate_back();
            }
            KeyCode::Char('r') => {
                let root = self.scan_root.clone();
                self.start_fresh_scan(root);
            }
            KeyCode::Char('q') | KeyCode::Esc => return PageAction::Pop,
            _ => {}
        }
        PageAction::None
    }
}
