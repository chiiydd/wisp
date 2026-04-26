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
//! ## Selection & deletion
//!
//! `Space` toggles a mark on the highlighted entry.  Marks are stored in a
//! `HashSet<Utf8PathBuf>` and survive directory navigation.  `d` opens a
//! Trash-confirmation; `D` opens a typed-yes Permanent-deletion confirm.
//! On success the cache is pruned in-place and ancestor sizes are
//! decremented — no full rescan needed.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use camino::{Utf8Path, Utf8PathBuf};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use humansize::{DECIMAL, format_size};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Points};
use ratatui::widgets::{Block, BorderType, Borders, Gauge, List, ListItem, ListState, Paragraph};
use uuid::Uuid;

use wisp_core::CoreResult;
use wisp_core::scanner::{ScanOptions, scan_directory};
use wisp_core::types::{
    CleanAction, CleanPlan, DeletionVia, Privileges, ProgressEvent, RiskLevel, ScanKey, ScanTree,
};
use wisp_engine::Engine;

use crate::chrome::KeyHint;
use crate::theme::Theme;
use crate::widgets::confirm::{ConfirmDialog, ConfirmResult};

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

    /// Remove `path` (and its entire subtree) from the cache.  Decrements
    /// ancestor sizes by the deleted subtree's cached size.
    ///
    /// Returns the number of bytes removed, or `None` if the path is absent.
    fn remove_subtree(&mut self, path: &Utf8Path) -> Option<u64> {
        let key = *self.index.get(path)?;
        let removed_size = self.tree.nodes[key].size;

        // Detach from parent's children list
        if let Some(parent) = path.parent()
            && let Some(&pk) = self.index.get(parent)
        {
            self.tree.nodes[pk].children.retain(|&k| k != key);
        }

        // Drop the subtree's nodes + index entries
        let mut stack = vec![key];
        while let Some(k) = stack.pop() {
            let (children, p) = match self.tree.nodes.get(k) {
                Some(n) => (n.children.clone(), n.path.clone()),
                None => continue,
            };
            stack.extend(children);
            self.index.remove(&p);
            self.tree.nodes.remove(k);
        }

        // Decrement ancestor sizes
        let mut cur = path.parent();
        while let Some(p) = cur {
            if let Some(&k) = self.index.get(p) {
                let node = &mut self.tree.nodes[k];
                node.size = node.size.saturating_sub(removed_size);
            }
            cur = p.parent();
        }

        Some(removed_size)
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

// ─── Pending deletion ────────────────────────────────────────────────────────

struct PendingDelete {
    /// Top-level paths to delete (already collapsed — no nested entries).
    paths: Vec<(Utf8PathBuf, u64)>,
    via: DeletionVia,
    total: u64,
}

// ─── Visualization mode ──────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum VizMode {
    Bars,
    Sectors,
}

impl VizMode {
    fn next(self) -> Self {
        match self {
            VizMode::Bars => VizMode::Sectors,
            VizMode::Sectors => VizMode::Bars,
        }
    }
    fn label(self) -> &'static str {
        match self {
            VizMode::Bars => "bars",
            VizMode::Sectors => "sectors",
        }
    }
}

/// Distinct hues used to colour adjacent sectors in the pie chart.
const SECTOR_PALETTE: &[Color] = &[
    Color::Cyan,
    Color::Yellow,
    Color::Green,
    Color::Magenta,
    Color::Blue,
    Color::Red,
    Color::LightCyan,
    Color::LightYellow,
];

// ─── AnalyzerPage ────────────────────────────────────────────────────────────

pub struct AnalyzerPage {
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

    /// Right-pane visualization mode (toggled with `v`).
    viz_mode: VizMode,

    /// Items marked for deletion (absolute paths).  Survives navigation.
    marked: HashSet<Utf8PathBuf>,
    /// Open confirmation dialog (Trash or Permanent).
    confirm: Option<ConfirmDialog>,
    /// Captured at confirm-open time so the dialog yes/no acts on a fixed set.
    pending: Option<PendingDelete>,
    /// True while a delete batch is executing — input is locked.
    deleting: bool,
    /// Last delete result line (e.g. "freed 4.2 GB", "2 failed").
    last_result: Option<String>,
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
            viz_mode: VizMode::Bars,
            marked: HashSet::new(),
            confirm: None,
            pending: None,
            deleting: false,
            last_result: None,
        }
    }

    // ── Scan management ──────────────────────────────────────────────────────

    fn launch_scan(root: Utf8PathBuf) -> ScanState {
        let (tx, rx) = mpsc::channel::<CoreResult<ScanTree>>();
        std::thread::spawn(move || {
            let opts = ScanOptions {
                max_depth: None,
                min_size: None,
                follow_symlinks: false,
            };
            #[allow(clippy::expect_used)]
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio rt");
            let _ = tx.send(rt.block_on(scan_directory(root, opts)));
        });
        ScanState::Scanning(rx)
    }

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

    fn navigate_into(&mut self, new_path: Utf8PathBuf) {
        if let ScanState::Ready(cached) = &self.scan_state {
            if let Some((entries, total)) = cached.entries_for(&new_path) {
                self.nav_stack.push(self.current_path.clone());
                self.current_path = new_path;
                self.entries = entries;
                self.total_size = total;
                self.list_state.select(if self.entries.is_empty() {
                    None
                } else {
                    Some(0)
                });
                return;
            }
            self.start_fresh_scan(new_path);
        }
    }

    fn navigate_back(&mut self) -> PageAction {
        if let Some(prev) = self.nav_stack.pop() {
            if let ScanState::Ready(cached) = &self.scan_state
                && let Some((entries, total)) = cached.entries_for(&prev)
            {
                self.current_path = prev;
                self.entries = entries;
                self.total_size = total;
                self.list_state.select(if self.entries.is_empty() {
                    None
                } else {
                    Some(0)
                });
            }
            PageAction::None
        } else {
            PageAction::Pop
        }
    }

    fn start_fresh_scan(&mut self, path: Utf8PathBuf) {
        self.nav_stack.clear();
        self.scan_root = path.clone();
        self.current_path = path.clone();
        self.entries.clear();
        self.total_size = 0;
        self.marked.clear();
        self.last_result = None;
        self.scan_state = Self::launch_scan(path);
    }

    /// Refresh the displayed entries from the cache for the current path.
    fn refresh_view(&mut self) {
        if let ScanState::Ready(cached) = &self.scan_state
            && let Some((entries, total)) = cached.entries_for(&self.current_path)
        {
            self.entries = entries;
            self.total_size = total;
            let cur = self.list_state.selected().unwrap_or(0);
            self.list_state.select(if self.entries.is_empty() {
                None
            } else {
                Some(cur.min(self.entries.len() - 1))
            });
        }
    }

    // ── Mark management ──────────────────────────────────────────────────────

    fn toggle_mark(&mut self, path: Utf8PathBuf) {
        if !self.marked.remove(&path) {
            self.marked.insert(path);
        }
    }

    fn clear_marks(&mut self) {
        self.marked.clear();
    }

    /// Total bytes covered by current marks (using cached sizes).
    fn marked_total(&self) -> u64 {
        let cached = match &self.scan_state {
            ScanState::Ready(c) => c,
            _ => return 0,
        };
        // De-dup nested marks before summing so /a + /a/b doesn't double-count.
        let mut paths: Vec<&Utf8PathBuf> = self.marked.iter().collect();
        paths.sort();
        let mut total: u64 = 0;
        let mut last: Option<&Utf8PathBuf> = None;
        for p in paths {
            if let Some(parent) = last
                && p.starts_with(parent)
            {
                continue;
            }
            if let Some(&k) = cached.index.get(p) {
                total = total.saturating_add(cached.tree.nodes[k].size);
            }
            last = Some(p);
        }
        total
    }

    // ── Rendering ────────────────────────────────────────────────────────────

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        // Body fills the area; chrome (path, hints, mode) is rendered by `app`.
        match &self.scan_state {
            ScanState::Scanning(_) if self.entries.is_empty() => {
                f.render_widget(
                    Paragraph::new(
                        " Scanning recursively in background — results will appear automatically…",
                    )
                    .style(Style::default().fg(Theme::MUTED)),
                    area,
                );
            }
            ScanState::Error(e) => {
                let e = e.clone();
                f.render_widget(
                    Paragraph::new(format!(" Error: {e}"))
                        .style(Style::default().fg(Theme::DANGER)),
                    area,
                );
            }
            _ => {
                self.render_entries(f, area);
            }
        }

        // Modal confirm dialog floats above everything else.
        if let Some(dlg) = &self.confirm {
            dlg.render(f, area);
        }
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
                let marked = self.marked.contains(&e.path);
                let mark_glyph = if marked { "● " } else { "  " };
                let mark_style = Style::default()
                    .fg(Theme::MARK)
                    .add_modifier(Modifier::BOLD);
                let size_str = format!("{:>10}", format_size(e.size, DECIMAL));
                let suffix = if e.is_dir { "/" } else { "" };
                ListItem::new(Line::from(vec![
                    Span::styled(mark_glyph, mark_style),
                    Span::styled(size_str, Style::default().fg(Theme::WARNING)),
                    Span::raw("  "),
                    Span::styled(
                        format!("{}{suffix}", e.name),
                        if e.is_dir {
                            Style::default().fg(Theme::ACCENT)
                        } else {
                            Style::default()
                        },
                    ),
                ]))
            })
            .collect();

        let count = self.entries.len();
        let title = if self.marked.is_empty() {
            format!(" {count} items ")
        } else {
            format!(" {count} items   ▣ {} marked ", self.marked.len())
        };
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Theme::ACCENT_DIM))
                    .title(Span::styled(
                        title,
                        Style::default()
                            .fg(Theme::ACCENT)
                            .add_modifier(Modifier::BOLD),
                    )),
            )
            .highlight_style(Theme::selection())
            .highlight_symbol("▶ ");

        f.render_stateful_widget(list, panes[0], &mut self.list_state);

        // ── Right: visualization (bars or sectors) ───────────────────────
        let viz_area = panes[1];
        let title = match self.viz_mode {
            VizMode::Bars => " Size distribution ",
            VizMode::Sectors => " Polar sectors ",
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Theme::ACCENT_DIM))
            .title(Span::styled(
                title.to_owned(),
                Style::default()
                    .fg(Theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(viz_area);
        f.render_widget(block, viz_area);

        if self.total_size == 0 || self.entries.is_empty() || inner.height == 0 {
            return;
        }

        match self.viz_mode {
            VizMode::Bars => self.render_bars(f, inner),
            VizMode::Sectors => self.render_sectors(f, inner),
        }
    }

    fn render_bars(&self, f: &mut Frame, inner: Rect) {
        let selected = self.list_state.selected();
        let n = self.entries.len().min(inner.height as usize);
        for (i, entry) in self.entries.iter().take(n).enumerate() {
            let ratio = entry.size as f64 / self.total_size as f64;
            let is_sel = selected == Some(i);
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
            let bar_color = if self.marked.contains(&entry.path) {
                Theme::MARK
            } else if is_sel {
                Theme::ACCENT
            } else {
                Theme::ACCENT_DIM
            };
            let gauge = Gauge::default()
                .gauge_style(Style::default().fg(bar_color).bg(Theme::MUTED))
                .ratio(ratio.min(1.0))
                .label(label);
            f.render_widget(gauge, gauge_rect);
        }
    }

    /// Render the entries as a polar donut chart using ratatui's `Canvas`.
    ///
    /// Visual cues for clarity:
    ///   • Donut shape (inner hole) — leaves room for a center label.
    ///   • Selected slice is **exploded** outward along its bisector and
    ///     drawn with a slightly larger outer radius + bright fill.
    ///   • Dark radial dividers separate adjacent non-selected slices so
    ///     individual wedges read distinctly even with similar palette
    ///     colors.
    ///   • Center text shows the selected entry's name, size, and %.
    fn render_sectors(&self, f: &mut Frame, inner: Rect) {
        struct Wedge {
            frac: f64,
            base_color: Color,
            marked: bool,
            is_sel: bool,
        }

        let selected = self.list_state.selected();
        let total = self.total_size as f64;

        let wedges: Vec<Wedge> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| Wedge {
                frac: e.size as f64 / total,
                base_color: SECTOR_PALETTE[i % SECTOR_PALETTE.len()],
                marked: self.marked.contains(&e.path),
                is_sel: selected == Some(i),
            })
            .filter(|w| w.frac > 0.0005)
            .collect();

        // Pre-compute centre-label strings.  Done here so the closure only
        // captures small owned values.
        let center: Option<(String, String, String)> =
            selected.and_then(|i| self.entries.get(i)).map(|e| {
                let suffix = if e.is_dir { "/" } else { "" };
                let pct = (e.size as f64 / total * 100.0).round() as u32;
                (
                    format!("{}{suffix}", e.name),
                    format_size(e.size, DECIMAL),
                    format!("{pct}%"),
                )
            });

        // Approximate horizontal-centre helper: terminal cell ≈ 2 / canvas-width
        // canvas units wide, so a string of length N centres at x = -N / W.
        let canvas_w_chars = inner.width.max(1) as f64;
        let center_x = move |s: &str| -(s.chars().count() as f64) / canvas_w_chars;

        // Avoid moving `wedges` into the closure as a whole, since we also
        // need it to render dividers afterwards in the same paint pass.
        let canvas = Canvas::default()
            .marker(Marker::Braille)
            .x_bounds([-1.0, 1.0])
            .y_bounds([-1.0, 1.0])
            .paint(move |ctx| {
                const INNER_R: f64 = 0.42;
                const OUTER_R_DEFAULT: f64 = 0.86;
                const OUTER_R_SELECTED: f64 = 0.96;
                const EXPLODE: f64 = 0.08;

                let mut start = -std::f64::consts::FRAC_PI_2;
                let mut boundaries: Vec<f64> = Vec::with_capacity(wedges.len() + 1);
                let mut sel_neighbour: Vec<bool> = Vec::with_capacity(wedges.len() + 1);

                // 1. Fills
                for (i, w) in wedges.iter().enumerate() {
                    boundaries.push(start);
                    let prev_is_sel = if i == 0 {
                        wedges.last().map(|w| w.is_sel).unwrap_or(false)
                    } else {
                        wedges[i - 1].is_sel
                    };
                    sel_neighbour.push(w.is_sel || prev_is_sel);

                    let end = start + w.frac * std::f64::consts::TAU;
                    let mid = (start + end) * 0.5;
                    let (mcos, msin) = (mid.cos(), mid.sin());

                    let (ox, oy) = if w.is_sel {
                        (EXPLODE * mcos, EXPLODE * msin)
                    } else {
                        (0.0, 0.0)
                    };
                    let outer_r = if w.is_sel {
                        OUTER_R_SELECTED
                    } else {
                        OUTER_R_DEFAULT
                    };

                    let color = if w.is_sel {
                        Color::White
                    } else if w.marked {
                        Color::Magenta
                    } else {
                        w.base_color
                    };

                    let n_radial = 14usize;
                    let n_angular = ((w.frac * 280.0) as usize).max(3);
                    let mut pts: Vec<(f64, f64)> =
                        Vec::with_capacity((n_radial + 1) * (n_angular + 1));
                    for ai in 0..=n_angular {
                        let a = start + (end - start) * (ai as f64) / (n_angular as f64);
                        let (sa, ca) = (a.sin(), a.cos());
                        for ri in 0..=n_radial {
                            let r = INNER_R + (outer_r - INNER_R) * (ri as f64) / (n_radial as f64);
                            pts.push((ox + r * ca, oy + r * sa));
                        }
                    }
                    ctx.draw(&Points {
                        coords: &pts,
                        color,
                    });
                    start = end;
                }

                // 2. Radial dividers — only between adjacent non-selected wedges.
                //    Drawn in black so they show as gaps against any palette colour.
                if wedges.len() > 1 {
                    let n = wedges.len();
                    for i in 0..n {
                        // Boundary at index i sits between wedge[i-1] and wedge[i].
                        if sel_neighbour[i] {
                            continue;
                        }
                        let a = boundaries[i];
                        let (sa, ca) = (a.sin(), a.cos());
                        let n_pts = 28usize;
                        let mut line_pts: Vec<(f64, f64)> = Vec::with_capacity(n_pts + 1);
                        for ri in 0..=n_pts {
                            let r = INNER_R
                                + (OUTER_R_DEFAULT - INNER_R) * (ri as f64) / (n_pts as f64);
                            line_pts.push((r * ca, r * sa));
                        }
                        ctx.draw(&Points {
                            coords: &line_pts,
                            color: Color::Black,
                        });
                    }
                }

                // 3. Centre label
                if let Some((name, size, pct)) = &center {
                    ctx.print(
                        center_x(name),
                        0.12,
                        Line::from(Span::styled(
                            name.clone(),
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        )),
                    );
                    ctx.print(
                        center_x(size),
                        0.00,
                        Line::from(Span::styled(
                            size.clone(),
                            Style::default().fg(Color::Yellow),
                        )),
                    );
                    ctx.print(
                        center_x(pct),
                        -0.12,
                        Line::from(Span::styled(
                            pct.clone(),
                            Style::default().fg(Color::DarkGray),
                        )),
                    );
                }
            });
        f.render_widget(canvas, inner);
    }

    // ── Event handling ───────────────────────────────────────────────────────

    pub async fn handle_event(&mut self, evt: &Event) -> PageAction {
        // 1. Confirm dialog intercepts everything when open.
        if let Some(dlg) = &mut self.confirm {
            match dlg.handle_event(evt) {
                ConfirmResult::Confirmed => {
                    self.confirm = None;
                    self.execute_pending().await;
                    return PageAction::None;
                }
                ConfirmResult::Cancelled => {
                    self.confirm = None;
                    self.pending = None;
                    return PageAction::None;
                }
                ConfirmResult::Pending => return PageAction::None,
            }
        }

        // 2. Lock input while a delete batch is running.
        if self.deleting {
            return PageAction::None;
        }

        let Event::Key(k) = evt else {
            return PageAction::None;
        };
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
                if let Some(sel) = self.list_state.selected()
                    && let Some(entry) = self.entries.get(sel)
                    && entry.is_dir
                {
                    let path = entry.path.clone();
                    self.navigate_into(path);
                }
            }
            KeyCode::Char(' ') => {
                if let Some(sel) = self.list_state.selected()
                    && let Some(entry) = self.entries.get(sel)
                {
                    let path = entry.path.clone();
                    self.toggle_mark(path);
                    self.last_result = None;
                }
            }
            KeyCode::Char('c') => {
                self.clear_marks();
            }
            KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left => {
                return self.navigate_back();
            }
            KeyCode::Char('d') => {
                self.start_delete(DeletionVia::Trash);
            }
            KeyCode::Char('D') => {
                self.start_delete(DeletionVia::Direct);
            }
            KeyCode::Char('r') => {
                let root = self.scan_root.clone();
                self.start_fresh_scan(root);
            }
            KeyCode::Char('v') => {
                self.viz_mode = self.viz_mode.next();
            }
            KeyCode::Esc => {
                if !self.marked.is_empty() {
                    self.clear_marks();
                } else {
                    return PageAction::Pop;
                }
            }
            KeyCode::Char('q') => return PageAction::Pop,
            _ => {}
        }
        PageAction::None
    }

    // ── Deletion ─────────────────────────────────────────────────────────────

    /// Build the pending deletion set and open the appropriate confirm dialog.
    fn start_delete(&mut self, via: DeletionVia) {
        let cached = match &self.scan_state {
            ScanState::Ready(c) => c,
            _ => return,
        };

        // Collect target paths: marked items, or fall back to current selection.
        let mut paths: Vec<Utf8PathBuf> = if !self.marked.is_empty() {
            self.marked.iter().cloned().collect()
        } else if let Some(sel) = self.list_state.selected() {
            self.entries
                .get(sel)
                .map(|e| vec![e.path.clone()])
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        if paths.is_empty() {
            return;
        }

        // Drop nested entries: keep only the topmost ancestor.
        paths.sort();
        let mut collapsed: Vec<(Utf8PathBuf, u64)> = Vec::new();
        let mut last: Option<Utf8PathBuf> = None;
        for p in paths {
            if let Some(parent) = &last
                && p.starts_with(parent)
            {
                continue;
            }
            let size = cached
                .index
                .get(&p)
                .map(|&k| cached.tree.nodes[k].size)
                .unwrap_or(0);
            last = Some(p.clone());
            collapsed.push((p, size));
        }

        let total: u64 = collapsed.iter().map(|(_, s)| *s).sum();
        let count = collapsed.len();

        let msg = match via {
            DeletionVia::Trash => format!(
                "Move {count} item{} to Trash ({})",
                if count == 1 { "" } else { "s" },
                format_size(total, DECIMAL),
            ),
            DeletionVia::Direct => format!(
                "PERMANENTLY delete {count} item{} ({})\nThis cannot be undone.",
                if count == 1 { "" } else { "s" },
                format_size(total, DECIMAL),
            ),
        };

        self.confirm = Some(ConfirmDialog::new(msg, matches!(via, DeletionVia::Direct)));
        self.pending = Some(PendingDelete {
            paths: collapsed,
            via,
            total,
        });
    }

    /// Execute the pending delete plan via the engine, then prune the cache.
    async fn execute_pending(&mut self) {
        let pending = match self.pending.take() {
            Some(p) => p,
            None => return,
        };

        self.deleting = true;
        self.last_result = None;

        let actions: Vec<CleanAction> = pending
            .paths
            .iter()
            .map(|(p, sz)| CleanAction::Delete {
                path: p.clone(),
                size: *sz,
                via: pending.via.clone(),
            })
            .collect();

        let plan = CleanPlan {
            id: Uuid::new_v4(),
            actions,
            estimated_size: pending.total,
            required_privileges: Privileges {
                requires_root: false,
            },
            risk: match pending.via {
                DeletionVia::Trash => RiskLevel::Safe,
                DeletionVia::Direct => RiskLevel::Moderate,
            },
        };

        let (tx, mut rx) = tokio::sync::mpsc::channel::<ProgressEvent>(256);
        let mut config = self.engine.config.clone();
        // Direct deletion needs prefer_trash=false so the engine doesn't override it back to Trash.
        config.prefer_trash = matches!(pending.via, DeletionVia::Trash);
        config.dry_run = false;
        let engine = Arc::new(Engine::new(config, Arc::clone(&self.engine.distro)));
        let confirmer = Arc::new(wisp_engine::AutoApproveConfirmer);

        let handle = tokio::spawn(async move { engine.execute(plan, confirmer, tx).await });

        // Track which paths succeeded so we can prune the cache accurately.
        let mut succeeded_idx: Vec<usize> = Vec::new();
        let mut failed_idx: Vec<usize> = Vec::new();
        while let Some(evt) = rx.recv().await {
            if let ProgressEvent::ActionFinished { id, result } = evt {
                let idx = id.0 as usize;
                match result {
                    wisp_core::types::ActionResult::Success { .. } => succeeded_idx.push(idx),
                    wisp_core::types::ActionResult::Failed { .. } => failed_idx.push(idx),
                    wisp_core::types::ActionResult::Skipped { .. } => failed_idx.push(idx),
                }
            }
        }

        let report = handle.await.ok().and_then(|r| r.ok());

        // Prune cache for successful paths and clear their marks.
        if let ScanState::Ready(cached) = &mut self.scan_state {
            for &i in &succeeded_idx {
                if let Some((p, _)) = pending.paths.get(i) {
                    cached.remove_subtree(p);
                    self.marked.remove(p);
                }
            }
        }

        // Failed marks are kept so the user can retry.
        let result_text = match report {
            Some(r) => {
                if r.failed == 0 && r.skipped == 0 {
                    format!(
                        "✓ freed {} ({} item{})",
                        format_size(r.bytes_freed, DECIMAL),
                        r.succeeded,
                        if r.succeeded == 1 { "" } else { "s" },
                    )
                } else {
                    format!(
                        "freed {} · {} ok · {} failed · {} skipped",
                        format_size(r.bytes_freed, DECIMAL),
                        r.succeeded,
                        r.failed,
                        r.skipped,
                    )
                }
            }
            None => "delete failed".to_owned(),
        };
        self.last_result = Some(result_text);

        self.deleting = false;
        self.refresh_view();
    }

    // ── Chrome contract ──────────────────────────────────────────────────────

    pub fn mode(&self) -> (String, Color) {
        if self.confirm.is_some() {
            return ("CONFIRM".into(), Theme::MODE_DANGER);
        }
        if self.deleting {
            return ("DELETING".into(), Theme::MODE_BUSY);
        }
        if matches!(self.scan_state, ScanState::Scanning(_)) {
            return ("SCANNING".into(), Theme::MODE_BUSY);
        }
        if matches!(self.scan_state, ScanState::Error(_)) {
            return ("ERROR".into(), Theme::MODE_DANGER);
        }
        if !self.marked.is_empty() {
            return ("VISUAL".into(), Theme::MODE_DETAIL);
        }
        ("ANALYZE".into(), Theme::MODE_NORMAL)
    }

    pub fn context(&self) -> Vec<Span<'static>> {
        let mut spans: Vec<Span<'static>> = Vec::new();

        // Path
        spans.push(Span::styled(
            short_path(&self.current_path),
            Style::default()
                .fg(Theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ));

        // Spinner / total size
        let busy = matches!(self.scan_state, ScanState::Scanning(_)) || self.deleting;
        if busy {
            let sp = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let s = sp[self.tick_count % sp.len()];
            let label = if self.deleting {
                "deleting…"
            } else {
                "scanning…"
            };
            spans.push(Span::styled(
                format!("  {s} {label}"),
                Style::default().fg(Theme::WARNING),
            ));
        } else if self.total_size > 0 {
            spans.push(Span::styled(
                format!("  · {}", format_size(self.total_size, DECIMAL)),
                Style::default().fg(Theme::FG_DIM),
            ));
        }

        // Marked count
        if !self.marked.is_empty() {
            let total = self.marked_total();
            spans.push(Span::styled(
                format!(
                    "  ·  ▣ {} ({})",
                    self.marked.len(),
                    format_size(total, DECIMAL)
                ),
                Style::default()
                    .fg(Theme::MARK)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        // Last result (after a delete)
        if self.marked.is_empty()
            && let Some(msg) = &self.last_result
        {
            spans.push(Span::styled(
                format!("  ·  {}", msg),
                Style::default().fg(Theme::SUCCESS),
            ));
        }

        // Nav depth indicator
        let depth = self.nav_stack.len();
        if depth > 0 {
            spans.push(Span::styled(
                format!("  ·  depth {depth}"),
                Style::default().fg(Theme::MUTED),
            ));
        }

        // Viz mode badge
        spans.push(Span::styled(
            format!("  ·  viz: {}", self.viz_mode.label()),
            Style::default().fg(Theme::MUTED),
        ));

        spans
    }

    pub fn hints(&self) -> Vec<KeyHint> {
        // Confirm dialog overrides everything.
        if self.confirm.is_some() {
            // Distinguish typed vs y/n by checking whether input is needed
            // — but we don't have direct access; show both keys as relevant.
            return vec![
                KeyHint::new("y/⏎", "confirm"),
                KeyHint::new("Esc", "cancel"),
            ];
        }
        if self.deleting {
            return vec![KeyHint::new("…", "deleting")];
        }
        if matches!(self.scan_state, ScanState::Scanning(_)) && self.entries.is_empty() {
            return vec![KeyHint::new("q", "back")];
        }

        let mut h = vec![
            KeyHint::new("j/k", "move"),
            KeyHint::new("⏎", "open"),
            KeyHint::new("⎵", "mark"),
        ];
        if !self.marked.is_empty() {
            h.push(KeyHint::new("d", "trash"));
            h.push(KeyHint::new("D", "delete!"));
            h.push(KeyHint::new("c", "clear"));
        } else {
            h.push(KeyHint::new("d", "trash"));
        }
        h.push(KeyHint::new("h", "back"));
        h.push(KeyHint::new("v", "viz"));
        h.push(KeyHint::new("r", "rescan"));
        h.push(KeyHint::new("q", "quit"));
        h
    }
}

/// Display a path with `$HOME → ~`.
fn short_path(p: &Utf8Path) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty()
        && let Some(rel) = p.as_str().strip_prefix(&home)
    {
        return format!("~{rel}");
    }
    p.to_string()
}
