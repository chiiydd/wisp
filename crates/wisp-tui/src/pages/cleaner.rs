//! Cleaner page.
//!
//! Idle → p (dry-run preview) → Planned → r/Enter (execute) → Running →
//! Done (enriched report card + scrollable log).
//!
//! The Done state shows:
//!   • Freed size and succeeded/failed/skipped counts.
//!   • Side-by-side breakdown: top directories vs file type categories.
//!   • Scrollable action log with full paths and sizes per entry.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use camino::Utf8Path;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use humansize::{DECIMAL, format_size};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Scrollbar,
    ScrollbarOrientation, ScrollbarState,
};

use wisp_core::types::{ActionResult, CleanAction, CleanPlan, ProgressEvent, RiskLevel};
use wisp_engine::Engine;

use crate::chrome::KeyHint;
use crate::theme::Theme;
use crate::widgets::confirm::ConfirmDialog;

use super::{CleanGroup, PageAction};

// ─── State machine ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum RunState {
    Idle,
    Building,
    Planned,
    ConfirmDangerous,
    Running,
    Done {
        freed: u64,
        succeeded: usize,
        failed: usize,
        skipped: usize,
    },
    Error(String),
}

// ─── Log entry ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct LogEntry {
    kind: LogKind,
    text: String,
}

#[derive(Clone, PartialEq)]
enum LogKind {
    Success,
    Failed,
    Skipped,
    Warning,
}

impl LogEntry {
    fn style(&self) -> Style {
        match self.kind {
            LogKind::Success => Style::default().fg(Color::Green),
            LogKind::Failed => Style::default().fg(Color::Red),
            LogKind::Skipped => Style::default().fg(Color::DarkGray),
            LogKind::Warning => Style::default().fg(Color::Yellow),
        }
    }

    fn prefix(&self) -> &'static str {
        match self.kind {
            LogKind::Success => "✓ ",
            LogKind::Failed => "✗ ",
            LogKind::Skipped => "– ",
            LogKind::Warning => "⚠ ",
        }
    }
}

// ─── DoneSummary ─────────────────────────────────────────────────────────────

/// Aggregated insights collected during execution, shown in the Done card.
struct DoneSummary {
    /// Top directories by bytes freed, (short label, bytes).
    top_dirs: Vec<(String, u64)>,
    /// Top file-type categories by bytes freed, (name, bytes).
    top_cats: Vec<(&'static str, u64)>,
}

/// Group `path` to at most `depth` components and shorten via `$HOME → ~`.
fn dir_group(path: &Utf8Path, depth: usize) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let s = path.as_str();
    let is_abs = s.starts_with('/');
    let parts: Vec<&str> = s.trim_start_matches('/').splitn(depth + 1, '/').collect();
    let taken: Vec<&str> = parts.iter().take(depth).copied().collect();
    let result = if is_abs {
        format!("/{}", taken.join("/"))
    } else {
        taken.join("/")
    };
    if !home.is_empty()
        && let Some(rel) = result.strip_prefix(&home)
    {
        return format!("~{rel}");
    }
    result
}

/// Classify a path into a human-readable category.
fn categorize_path(path: &Utf8Path) -> &'static str {
    let lc = path.as_str().to_lowercase();
    let ext = path.extension().map(str::to_lowercase).unwrap_or_default();
    if lc.contains("/trash") || lc.contains(".trash") {
        return "Trash";
    }
    if lc.contains(".cache") || lc.contains("/cache") {
        return "Cache";
    }
    if lc.contains("/log") || ext == "log" {
        return "Logs";
    }
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "bmp" | "ico" | "tiff" => "Images",
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" | "m4v" => "Videos",
        "mp3" | "flac" | "ogg" | "wav" | "aac" | "opus" | "m4a" => "Audio",
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "zst" => "Archives",
        "tmp" | "temp" | "bak" | "swp" | "old" => "Temp",
        _ => "Other",
    }
}

/// Shorten `path` by replacing `$HOME` with `~`.
fn short_path(path: &Utf8Path) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty()
        && let Some(rel) = path.as_str().strip_prefix(&home)
    {
        return format!("~{rel}");
    }
    path.to_string()
}

/// Truncate a string to `max` chars, preserving the tail with an ellipsis.
fn truncate_tail(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_owned();
    }
    let keep = max.saturating_sub(1);
    let tail: String = chars[chars.len() - keep..].iter().collect();
    format!("…{tail}")
}

// ─── Risk filter (Planned-state view) ────────────────────────────────────────

/// Visibility filter for the plan list. `All` shows everything; the other
/// variants restrict the list to actions of a single risk level so the user
/// can focus on (e.g.) just the Dangerous rows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RiskFilter {
    All,
    Trivial,
    Safe,
    Moderate,
    Dangerous,
}

impl RiskFilter {
    fn cycle(self) -> Self {
        match self {
            RiskFilter::All => RiskFilter::Trivial,
            RiskFilter::Trivial => RiskFilter::Safe,
            RiskFilter::Safe => RiskFilter::Moderate,
            RiskFilter::Moderate => RiskFilter::Dangerous,
            RiskFilter::Dangerous => RiskFilter::All,
        }
    }

    fn label(self) -> &'static str {
        match self {
            RiskFilter::All => "all",
            RiskFilter::Trivial => "trivial",
            RiskFilter::Safe => "safe",
            RiskFilter::Moderate => "moderate",
            RiskFilter::Dangerous => "dangerous",
        }
    }

    fn matches(self, r: RiskLevel) -> bool {
        match self {
            RiskFilter::All => true,
            RiskFilter::Trivial => r == RiskLevel::Trivial,
            RiskFilter::Safe => r == RiskLevel::Safe,
            RiskFilter::Moderate => r == RiskLevel::Moderate,
            RiskFilter::Dangerous => r == RiskLevel::Dangerous,
        }
    }
}

/// Risk of action `i` in `plan`. Falls back to `plan.risk` if the parallel
/// `risks` vector is missing or short (older serialized plans).
fn action_risk(plan: &CleanPlan, i: usize) -> RiskLevel {
    plan.risks.get(i).copied().unwrap_or(plan.risk)
}

/// Count `[Trivial, Safe, Moderate, Dangerous]` actions in `plan`.
fn count_risks(plan: &CleanPlan) -> [usize; 4] {
    let mut c = [0usize; 4];
    for i in 0..plan.actions.len() {
        match action_risk(plan, i) {
            RiskLevel::Trivial => c[0] += 1,
            RiskLevel::Safe => c[1] += 1,
            RiskLevel::Moderate => c[2] += 1,
            RiskLevel::Dangerous => c[3] += 1,
        }
    }
    c
}

// ─── Page ────────────────────────────────────────────────────────────────────

pub struct CleanerPage {
    engine: Arc<Engine>,
    group: CleanGroup,
    plan_list_state: ListState,
    /// Selection within the Idle action list (Preview / Run / Back).
    idle_state: ListState,
    log_scroll: usize,
    run_state: RunState,
    plan: Option<CleanPlan>,
    log: Vec<LogEntry>,
    confirm_dialog: Option<ConfirmDialog>,
    tick_count: usize,
    /// Set when transitioning to Done; cleared when the user resets.
    done_summary: Option<DoneSummary>,

    // ── Plan-review state (Planned state only) ──────────────────────────────
    /// Indices into `plan.actions` the user has marked to skip on execute.
    skipped: HashSet<usize>,
    /// Visibility filter; defaults to `All`.
    filter: RiskFilter,
    /// Indices of `plan.actions` currently visible (filtered + ordered).
    /// Recomputed whenever `plan` or `filter` changes.
    visible: Vec<usize>,
}

/// Idle-state action menu.  The action kind is attached so dispatch
/// doesn't depend on row index.
struct IdleItem {
    label: &'static str,
    desc: &'static str,
    action: IdleAction,
}

#[derive(Clone, Copy)]
enum IdleAction {
    Preview,
    Run,
    Back,
}

const IDLE_ACTIONS: &[IdleItem] = &[
    IdleItem {
        label: "Preview",
        desc: "dry-run · nothing is deleted",
        action: IdleAction::Preview,
    },
    IdleItem {
        label: "Run now",
        desc: "build plan & execute immediately",
        action: IdleAction::Run,
    },
    IdleItem {
        label: "Back to menu",
        desc: "return without changes",
        action: IdleAction::Back,
    },
];

/// Maximum number of plan actions rendered in the preview list.  Above this
/// the list shows "+N more" instead.
const PLAN_PREVIEW_CAP: usize = 500;

impl CleanerPage {
    pub fn new(group: CleanGroup, engine: Arc<Engine>) -> Self {
        let mut idle_state = ListState::default();
        idle_state.select(Some(0));
        Self {
            engine,
            group,
            plan_list_state: ListState::default(),
            idle_state,
            log_scroll: 0,
            run_state: RunState::Idle,
            plan: None,
            log: Vec::new(),
            confirm_dialog: None,
            tick_count: 0,
            done_summary: None,
            skipped: HashSet::new(),
            filter: RiskFilter::All,
            visible: Vec::new(),
        }
    }

    /// Rebuild `visible` from the current `plan` and `filter`. Also clamps
    /// `plan_list_state` so it stays inside the new visible range.
    fn refresh_visible(&mut self) {
        self.visible.clear();
        if let Some(p) = &self.plan {
            for (i, _) in p.actions.iter().enumerate() {
                if self.filter.matches(action_risk(p, i)) {
                    self.visible.push(i);
                }
            }
        }
        let cur = self.plan_list_state.selected().unwrap_or(0);
        if self.visible.is_empty() {
            self.plan_list_state.select(None);
        } else {
            self.plan_list_state
                .select(Some(cur.min(self.visible.len() - 1)));
        }
    }

    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
    }

    // ── Rendering ────────────────────────────────────────────────────────────

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        match &self.run_state {
            RunState::Idle => self.render_idle(f, area),
            RunState::Building => {
                let sp = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let s = sp[self.tick_count % sp.len()];
                f.render_widget(
                    Paragraph::new(format!(" {s} Building plan…"))
                        .style(Style::default().fg(Theme::WARNING)),
                    area,
                );
            }
            RunState::Planned | RunState::ConfirmDangerous => {
                self.render_plan(f, area);
                if matches!(self.run_state, RunState::ConfirmDangerous)
                    && let Some(dlg) = &self.confirm_dialog
                {
                    dlg.render(f, area);
                }
            }
            RunState::Running => self.render_log(f, area, true),
            RunState::Done {
                freed,
                succeeded,
                failed,
                skipped,
            } => {
                let (freed, succeeded, failed, skipped) = (*freed, *succeeded, *failed, *skipped);
                self.render_done(f, area, freed, succeeded, failed, skipped);
            }
            RunState::Error(e) => {
                let e = e.clone();
                f.render_widget(
                    Paragraph::new(format!(" Error: {e}"))
                        .style(Style::default().fg(Theme::DANGER))
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_type(BorderType::Rounded)
                                .border_style(Style::default().fg(Theme::DANGER)),
                        ),
                    area,
                );
            }
        }
    }

    fn render_idle(&mut self, f: &mut Frame, area: Rect) {
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);

        // ── Left: what will be cleaned ────────────────────────────────────
        let cleaners: &[(&str, &str)] = match &self.group {
            CleanGroup::User => &[
                ("Trash can", "~/.local/share/Trash"),
                ("Browser cache", "Chromium · Firefox · Brave"),
                ("Thumbnail cache", "~/.cache/thumbnails"),
                ("LinuxQQ caches", "~/.config/QQ shell + logs"),
                ("Flatpak runtimes", "unused versions"),
            ],
            CleanGroup::System => &[
                ("Pacman cache", "/var/cache/pacman/pkg"),
                ("Systemd journal", "/var/log/journal"),
                ("Orphan packages", "pacman -Qtdq"),
                ("Temporary files", "/tmp"),
            ],
            CleanGroup::Dev => &[
                ("Cargo registry", "~/.cargo/registry"),
                ("npm cache", "~/.npm"),
                ("pip cache", "~/.cache/pip"),
                ("Go module cache", "~/go/pkg/mod"),
                ("Docker", "dangling images & build cache"),
            ],
            CleanGroup::LinuxQq => &[
                ("LinuxQQ caches", "Electron shell · logs · partials (Safe)"),
                (
                    "LinuxQQ chat media",
                    "Pic · Video · Ptt · File · Emoji · avatar (Dangerous)",
                ),
            ],
            CleanGroup::All => &[("All cleaners", "@user + @system + @dev")],
        };

        let group_label = self.group.label();

        let cleaner_items: Vec<ListItem> = cleaners
            .iter()
            .map(|(name, detail)| {
                ListItem::new(Line::from(vec![
                    Span::styled("  ● ", Style::default().fg(Theme::ACCENT)),
                    Span::styled(format!("{name:<20}"), Style::default()),
                    Span::styled(*detail, Style::default().fg(Theme::MUTED)),
                ]))
            })
            .collect();

        f.render_widget(
            List::new(cleaner_items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Theme::ACCENT_DIM))
                    .title(Span::styled(
                        format!(" {group_label} cleaners "),
                        Style::default()
                            .fg(Theme::ACCENT)
                            .add_modifier(Modifier::BOLD),
                    )),
            ),
            panes[0],
        );

        // ── Right: selectable action list ─────────────────────────────────
        let action_items: Vec<ListItem> = IDLE_ACTIONS
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

        let action_list = List::new(action_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Theme::ACCENT_DIM))
                    .title(Span::styled(
                        " Actions ",
                        Style::default()
                            .fg(Theme::ACCENT)
                            .add_modifier(Modifier::BOLD),
                    )),
            )
            .highlight_style(Theme::selection())
            .highlight_symbol("▶ ");

        f.render_stateful_widget(action_list, panes[1], &mut self.idle_state);
    }

    fn render_plan(&mut self, f: &mut Frame, area: Rect) {
        let plan = match &self.plan {
            Some(p) => p,
            None => return,
        };

        // ── Per-row build (use the filtered visible[] view) ──────────────────
        let total = plan.actions.len();
        let counts = count_risks(plan);
        let kept_size: u64 = (0..total)
            .filter(|i| !self.skipped.contains(i))
            .map(|i| match &plan.actions[i] {
                CleanAction::Delete { size, .. } => *size,
                CleanAction::RunExternal { estimated_size, .. } => estimated_size.unwrap_or(0),
            })
            .sum();

        let visible: Vec<usize> = self
            .visible
            .iter()
            .copied()
            .take(PLAN_PREVIEW_CAP)
            .collect();

        let items: Vec<ListItem> = visible
            .iter()
            .map(|&i| {
                let a = &plan.actions[i];
                let risk = action_risk(plan, i);
                let (label, size) = match a {
                    CleanAction::Delete { path, size, .. } => (path.as_str().to_owned(), *size),
                    CleanAction::RunExternal {
                        cmd,
                        estimated_size,
                    } => (
                        format!("{} {}", cmd.program, cmd.args.join(" ")),
                        estimated_size.unwrap_or(0),
                    ),
                };
                let skipped = self.skipped.contains(&i);
                let risk_tag = match risk {
                    RiskLevel::Trivial => "·",
                    RiskLevel::Safe => " ",
                    RiskLevel::Moderate => "⚠",
                    RiskLevel::Dangerous => "⚠",
                };
                let (text_style, size_style) = if skipped {
                    let s = Style::default()
                        .fg(Theme::FG_DIM)
                        .add_modifier(Modifier::CROSSED_OUT);
                    (s, s)
                } else {
                    let row_color = row_color(risk);
                    (
                        Style::default().fg(row_color),
                        Style::default().fg(Theme::WARNING),
                    )
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        if skipped {
                            "✗ ".to_owned()
                        } else {
                            format!("{risk_tag} ")
                        },
                        Style::default().fg(risk_color(risk)),
                    ),
                    Span::styled(format!("{:>10}  ", format_size(size, DECIMAL)), size_style),
                    Span::styled(label, text_style),
                ]))
            })
            .collect();

        // ── Title: counts + filter + skip ────────────────────────────────────
        let mut title = format!(" Plan · {total} ");
        if counts[2] > 0 {
            title.push_str(&format!("· {} mod ", counts[2]));
        }
        if counts[3] > 0 {
            title.push_str(&format!("· {} dang ", counts[3]));
        }
        if !self.skipped.is_empty() {
            title.push_str(&format!(
                "· skip {}/{total} · keep {} ",
                self.skipped.len(),
                format_size(kept_size, DECIMAL)
            ));
        }
        if self.filter != RiskFilter::All {
            title.push_str(&format!("· filter:{} ", self.filter.label()));
        }
        if self.visible.len() > PLAN_PREVIEW_CAP {
            title.push_str(&format!(
                "· +{} more ",
                self.visible.len() - PLAN_PREVIEW_CAP
            ));
        }

        // Border colour reflects the *kept* risk level, not the raw plan risk —
        // skipping all Dangerous should turn the frame yellow / green, not red.
        let kept_risk = (0..total)
            .filter(|i| !self.skipped.contains(i))
            .map(|i| action_risk(plan, i))
            .max()
            .unwrap_or(RiskLevel::Trivial);
        let border_c = risk_color(kept_risk);

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(border_c))
                    .title(Span::styled(
                        title,
                        Style::default().fg(border_c).add_modifier(Modifier::BOLD),
                    )),
            )
            .highlight_style(Theme::selection())
            .highlight_symbol("▶ ");
        f.render_stateful_widget(list, area, &mut self.plan_list_state);

        // Empty-filter hint.
        if self.visible.is_empty() && total > 0 {
            let inner = area.inner(ratatui::layout::Margin {
                horizontal: 2,
                vertical: 2,
            });
            f.render_widget(
                Paragraph::new(format!(
                    "No actions match filter '{}' — press 'a' to clear.",
                    self.filter.label()
                ))
                .style(Style::default().fg(Theme::FG_DIM)),
                inner,
            );
        }
    }

    fn render_log(&self, f: &mut Frame, area: Rect, live: bool) {
        let title = if live { " Progress " } else { " Action log " };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Theme::ACCENT_DIM))
            .title(Span::styled(
                title,
                Style::default()
                    .fg(Theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        f.render_widget(block, area);

        if inner.height == 0 {
            return;
        }
        let vis = inner.height as usize;
        let total = self.log.len();

        let start = if live {
            total.saturating_sub(vis)
        } else {
            self.log_scroll.min(total.saturating_sub(1))
        };

        let items: Vec<ListItem> = self.log[start..]
            .iter()
            .take(vis)
            .map(|e| {
                ListItem::new(Line::from(vec![
                    Span::styled(e.prefix(), e.style()),
                    Span::styled(e.text.clone(), e.style()),
                ]))
            })
            .collect();

        f.render_widget(List::new(items), inner);

        if total > vis {
            let mut sb_state = ScrollbarState::new(total).position(start);
            f.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                area,
                &mut sb_state,
            );
        }
    }

    fn render_done(
        &mut self,
        f: &mut Frame,
        area: Rect,
        freed: u64,
        succeeded: usize,
        failed: usize,
        skipped: usize,
    ) {
        let has_breakdown = self
            .done_summary
            .as_ref()
            .map(|s| !s.top_dirs.is_empty() || !s.top_cats.is_empty())
            .unwrap_or(false);

        let breakdown_rows = if has_breakdown {
            self.done_summary
                .as_ref()
                .map(|s| s.top_dirs.len().max(s.top_cats.len()).min(5))
                .unwrap_or(0)
        } else {
            0
        };

        // border(2) + blank(1) + freed(1) + counts(1) + [blank(1) + header(1) + rows]
        let summary_h = 5u16
            + if has_breakdown {
                2 + breakdown_rows as u16
            } else {
                0
            };
        let summary_h = summary_h.min(area.height.saturating_sub(4));
        let log_h = area.height.saturating_sub(summary_h).max(3);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(summary_h), Constraint::Length(log_h)])
            .split(area);

        // ── Summary card ─────────────────────────────────────────────────
        let total = succeeded + failed + skipped;
        let success_pct = (succeeded * 100).checked_div(total).unwrap_or(0);
        let card_border_color = if failed == 0 {
            Theme::SUCCESS
        } else {
            Theme::WARNING
        };

        let mut card_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Freed  ", Style::default().fg(Theme::MUTED)),
                Span::styled(
                    format_size(freed, DECIMAL),
                    Style::default()
                        .fg(Theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("✓ {succeeded} succeeded"),
                    Style::default().fg(Theme::SUCCESS),
                ),
                Span::raw("   "),
                Span::styled(
                    format!("✗ {failed} failed"),
                    if failed > 0 {
                        Style::default()
                            .fg(Theme::DANGER)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Theme::MUTED)
                    },
                ),
                Span::raw("   "),
                Span::styled(
                    format!("– {skipped} skipped"),
                    Style::default().fg(Theme::MUTED),
                ),
                Span::raw(format!("   ({success_pct}% ok)")),
            ]),
        ];

        if has_breakdown {
            card_lines.push(Line::from(""));
            card_lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<34}", "Directories"),
                    Style::default()
                        .fg(Theme::MUTED)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "File types",
                    Style::default()
                        .fg(Theme::MUTED)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));

            let dirs = self
                .done_summary
                .as_ref()
                .map(|s| &s.top_dirs[..])
                .unwrap_or(&[]);
            let cats = self
                .done_summary
                .as_ref()
                .map(|s| &s.top_cats[..])
                .unwrap_or(&[]);

            for i in 0..breakdown_rows {
                let dir_col = dirs
                    .get(i)
                    .map(|(label, bytes)| {
                        format!(
                            "{:>10}  {}",
                            format_size(*bytes, DECIMAL),
                            truncate_tail(label, 20)
                        )
                    })
                    .unwrap_or_default();

                let cat_col = cats
                    .get(i)
                    .map(|(name, bytes)| {
                        format!("{:<10}  {:>10}", name, format_size(*bytes, DECIMAL))
                    })
                    .unwrap_or_default();

                card_lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{dir_col:<34}"), Style::default().fg(Theme::ACCENT)),
                    Span::styled(cat_col, Style::default().fg(Theme::WARNING)),
                ]));
            }
        }

        f.render_widget(
            Paragraph::new(card_lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(card_border_color))
                    .title(Span::styled(
                        " ✓ Summary ",
                        Style::default()
                            .fg(card_border_color)
                            .add_modifier(Modifier::BOLD),
                    )),
            ),
            chunks[0],
        );

        // ── Scrollable action log ────────────────────────────────────────
        self.render_log(f, chunks[1], false);
    }

    // ── Event handling ───────────────────────────────────────────────────────

    pub async fn handle_event(&mut self, evt: &Event) -> PageAction {
        if let Some(dlg) = &mut self.confirm_dialog {
            match dlg.handle_event(evt) {
                crate::widgets::confirm::ConfirmResult::Confirmed => {
                    self.confirm_dialog = None;
                    self.execute_plan(false).await;
                    return PageAction::None;
                }
                crate::widgets::confirm::ConfirmResult::Cancelled => {
                    self.confirm_dialog = None;
                    self.run_state = RunState::Planned;
                    return PageAction::None;
                }
                crate::widgets::confirm::ConfirmResult::Pending => return PageAction::None,
            }
        }

        let Event::Key(k) = evt else {
            return PageAction::None;
        };
        if k.kind != KeyEventKind::Press {
            return PageAction::None;
        }

        if matches!(self.run_state, RunState::Done { .. }) {
            match k.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.log_scroll = (self.log_scroll + 1).min(self.log.len().saturating_sub(1));
                    return PageAction::None;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.log_scroll = self.log_scroll.saturating_sub(1);
                    return PageAction::None;
                }
                KeyCode::Char('r') => {
                    self.run_state = RunState::Idle;
                    self.log.clear();
                    self.log_scroll = 0;
                    self.done_summary = None;
                    return PageAction::None;
                }
                KeyCode::Char('q') | KeyCode::Esc | KeyCode::Backspace => {
                    return PageAction::Pop;
                }
                _ => return PageAction::None,
            }
        }

        // Idle / Error: selection-based action menu (no p/r shortcuts).
        if matches!(self.run_state, RunState::Idle | RunState::Error(_)) {
            return self.handle_idle(k.code).await;
        }

        // Planned / Building / Running: keyboard shortcuts still apply.
        match k.code {
            KeyCode::Char('p') => {
                self.build_plan(true).await;
            }
            KeyCode::Enter | KeyCode::Char('r') => {
                if matches!(self.run_state, RunState::Planned) {
                    self.maybe_confirm_and_run().await;
                }
            }
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Backspace => return PageAction::Pop,
            KeyCode::Down | KeyCode::Char('j') if !self.visible.is_empty() => {
                let i = self.plan_list_state.selected().unwrap_or(0);
                self.plan_list_state
                    .select(Some((i + 1).min(self.visible.len() - 1)));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.plan_list_state.selected().unwrap_or(0);
                self.plan_list_state.select(Some(i.saturating_sub(1)));
            }
            // Toggle skip on the highlighted action.
            KeyCode::Char(' ' | 's') => {
                if let Some(sel) = self.plan_list_state.selected()
                    && let Some(&idx) = self.visible.get(sel)
                    && !self.skipped.insert(idx)
                {
                    self.skipped.remove(&idx);
                }
            }
            // Bulk-skip every Dangerous action — the "one-key cancel" the
            // user asked for.
            KeyCode::Char('D') => self.skip_all_at_risk(RiskLevel::Dangerous),
            // Bulk-skip Moderate too.
            KeyCode::Char('M') => self.skip_all_at_risk(RiskLevel::Moderate),
            // Clear all skips — useful after bulk-skipping by mistake.
            KeyCode::Char('U') => {
                self.skipped.clear();
            }
            // Cycle filter: All → Trivial → Safe → Moderate → Dangerous → All
            KeyCode::Char('f') => {
                self.filter = self.filter.cycle();
                self.plan_list_state.select(Some(0));
                self.refresh_visible();
            }
            // Reset filter to All.
            KeyCode::Char('a') if self.filter != RiskFilter::All => {
                self.filter = RiskFilter::All;
                self.plan_list_state.select(Some(0));
                self.refresh_visible();
            }
            _ => {}
        }
        PageAction::None
    }

    /// Mark every action whose risk equals `level` as skipped.
    fn skip_all_at_risk(&mut self, level: RiskLevel) {
        let Some(plan) = &self.plan else { return };
        for i in 0..plan.actions.len() {
            if action_risk(plan, i) == level {
                self.skipped.insert(i);
            }
        }
    }

    /// Handle Idle (and Error) state events: navigate the action list and activate.
    async fn handle_idle(&mut self, code: KeyCode) -> PageAction {
        let n = IDLE_ACTIONS.len();
        match code {
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.idle_state.selected().unwrap_or(0);
                self.idle_state.select(Some((i + 1) % n));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.idle_state.selected().unwrap_or(0);
                self.idle_state
                    .select(Some(if i == 0 { n - 1 } else { i - 1 }));
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                return self.activate_idle().await;
            }
            KeyCode::Char('q')
            | KeyCode::Esc
            | KeyCode::Backspace
            | KeyCode::Char('h')
            | KeyCode::Left => {
                return PageAction::Pop;
            }
            _ => {}
        }
        PageAction::None
    }

    /// Run the currently-selected Idle action.
    async fn activate_idle(&mut self) -> PageAction {
        let action = IDLE_ACTIONS
            .get(self.idle_state.selected().unwrap_or(0))
            .map(|item| item.action)
            .unwrap_or(IdleAction::Back);
        match action {
            IdleAction::Preview => {
                self.build_plan(true).await;
            }
            IdleAction::Run => {
                self.build_and_run().await;
            }
            IdleAction::Back => return PageAction::Pop,
        }
        PageAction::None
    }

    // ── Execution helpers ────────────────────────────────────────────────────

    async fn build_plan(&mut self, dry_run: bool) {
        self.run_state = RunState::Building;
        let targets = self.group.as_targets();
        let mut config = self.engine.config.clone();
        config.dry_run = dry_run;
        let engine = Engine::new(config, Arc::clone(&self.engine.distro));
        match engine.build_plan(targets).await {
            Ok(plan) => {
                self.plan = Some(plan);
                self.skipped.clear();
                self.filter = RiskFilter::All;
                self.plan_list_state.select(Some(0));
                self.refresh_visible();
                self.run_state = RunState::Planned;
            }
            Err(e) => {
                self.run_state = RunState::Error(e.to_string());
            }
        }
    }

    async fn build_and_run(&mut self) {
        self.build_plan(false).await;
        if self.run_state == RunState::Planned {
            self.maybe_confirm_and_run().await;
        }
    }

    async fn maybe_confirm_and_run(&mut self) {
        // Risk after applying the user's skip set. Skipping every Dangerous
        // action drops the prompt, so the user gets exactly the friction they
        // chose.
        let kept_risk = self
            .plan
            .as_ref()
            .map(|p| {
                (0..p.actions.len())
                    .filter(|i| !self.skipped.contains(i))
                    .map(|i| action_risk(p, i))
                    .max()
                    .unwrap_or(RiskLevel::Trivial)
            })
            .unwrap_or(RiskLevel::Trivial);
        if kept_risk >= RiskLevel::Dangerous {
            self.confirm_dialog = Some(ConfirmDialog::new(
                "This plan contains DANGEROUS actions. Type 'yes' to confirm.".into(),
                true,
            ));
            self.run_state = RunState::ConfirmDangerous;
        } else {
            self.execute_plan(false).await;
        }
    }

    async fn execute_plan(&mut self, dry_run: bool) {
        let plan = match self.plan.take() {
            Some(p) => p,
            None => return,
        };

        // Drop skipped actions before submitting to the engine, and recompute
        // size + risk on the survivors.
        let plan = if self.skipped.is_empty() {
            plan
        } else {
            let CleanPlan {
                id,
                actions,
                risks,
                required_privileges,
                ..
            } = plan;
            let mut new_actions = Vec::with_capacity(actions.len() - self.skipped.len());
            let mut new_risks = Vec::with_capacity(new_actions.capacity());
            for (i, a) in actions.into_iter().enumerate() {
                if !self.skipped.contains(&i) {
                    let r = risks.get(i).copied().unwrap_or(RiskLevel::Safe);
                    new_actions.push(a);
                    new_risks.push(r);
                }
            }
            let estimated_size = new_actions
                .iter()
                .map(|a| match a {
                    CleanAction::Delete { size, .. } => *size,
                    CleanAction::RunExternal { estimated_size, .. } => estimated_size.unwrap_or(0),
                })
                .sum();
            let max_risk = new_risks
                .iter()
                .copied()
                .max()
                .unwrap_or(RiskLevel::Trivial);
            CleanPlan {
                id,
                actions: new_actions,
                risks: new_risks,
                estimated_size,
                required_privileges,
                risk: max_risk,
            }
        };
        self.skipped.clear();

        self.run_state = RunState::Running;
        self.log.clear();
        self.log_scroll = 0;
        self.done_summary = None;

        // Keep a local copy of actions so we can look up paths by ActionId.
        let plan_actions: Vec<CleanAction> = plan.actions.clone();

        let (tx, mut rx) = tokio::sync::mpsc::channel::<ProgressEvent>(512);
        let mut config = self.engine.config.clone();
        config.dry_run = dry_run;
        let engine = Arc::new(Engine::new(config, Arc::clone(&self.engine.distro)));
        let confirmer = Arc::new(wisp_engine::AutoApproveConfirmer);

        let handle = tokio::spawn(async move { engine.execute(plan, confirmer, tx).await });

        // Accumulators for DoneSummary
        let mut dir_map: HashMap<String, u64> = HashMap::new();
        let mut cat_map: HashMap<&'static str, u64> = HashMap::new();

        while let Some(evt) = rx.recv().await {
            match evt {
                ProgressEvent::ActionFinished { id, result } => {
                    let action = plan_actions.get(id.0 as usize);

                    let path_label = match action {
                        Some(CleanAction::Delete { path, .. }) => short_path(path),
                        Some(CleanAction::RunExternal { cmd, .. }) => {
                            format!("{} {}", cmd.program, cmd.args.join(" "))
                        }
                        _ => String::new(),
                    };

                    // Collect stats for successful deletions.
                    if let ActionResult::Success { bytes_freed } = &result
                        && let Some(CleanAction::Delete { path, .. }) = action
                    {
                        *dir_map.entry(dir_group(path, 4)).or_default() += bytes_freed;
                        *cat_map.entry(categorize_path(path)).or_default() += bytes_freed;
                    }

                    let entry = match result {
                        ActionResult::Success { bytes_freed } => LogEntry {
                            kind: LogKind::Success,
                            text: format!(
                                "{:>10}  {path_label}",
                                format_size(bytes_freed, DECIMAL)
                            ),
                        },
                        ActionResult::Failed { error } => LogEntry {
                            kind: LogKind::Failed,
                            text: if path_label.is_empty() {
                                error
                            } else {
                                format!("{path_label}  — {error}")
                            },
                        },
                        ActionResult::Skipped { reason } => LogEntry {
                            kind: LogKind::Skipped,
                            text: if path_label.is_empty() {
                                format!("({reason})")
                            } else {
                                format!("{path_label}  ({reason})")
                            },
                        },
                    };
                    self.log.push(entry);
                }
                ProgressEvent::Warning(w) => {
                    self.log.push(LogEntry {
                        kind: LogKind::Warning,
                        text: w,
                    });
                }
                _ => {}
            }
        }

        match handle.await {
            Ok(Ok(report)) => {
                let first_fail = self.log.iter().position(|e| e.kind == LogKind::Failed);
                self.log_scroll = first_fail.unwrap_or(0);

                // Build top directories
                let mut top_dirs: Vec<(String, u64)> = dir_map.into_iter().collect();
                top_dirs.sort_by_key(|d| std::cmp::Reverse(d.1));
                top_dirs.truncate(5);

                // Build top categories
                let mut top_cats: Vec<(&'static str, u64)> = cat_map.into_iter().collect();
                top_cats.sort_by_key(|c| std::cmp::Reverse(c.1));
                top_cats.truncate(5);
                // Drop "Other" if it is not the only category
                if top_cats.len() > 1 {
                    top_cats.retain(|(name, _)| *name != "Other");
                    top_cats.truncate(5);
                }

                self.done_summary = Some(DoneSummary { top_dirs, top_cats });

                self.run_state = RunState::Done {
                    freed: report.bytes_freed,
                    succeeded: report.succeeded,
                    failed: report.failed,
                    skipped: report.skipped,
                };
            }
            Ok(Err(e)) => {
                self.run_state = RunState::Error(e.to_string());
            }
            Err(e) => {
                self.run_state = RunState::Error(e.to_string());
            }
        }
    }

    // ── Chrome contract ──────────────────────────────────────────────────────

    pub fn mode(&self) -> (String, Color) {
        match &self.run_state {
            RunState::Idle => ("READY".into(), Theme::MODE_NORMAL),
            RunState::Building => ("PLANNING".into(), Theme::MODE_BUSY),
            RunState::Planned => ("REVIEW".into(), Theme::MODE_DETAIL),
            RunState::ConfirmDangerous => ("CONFIRM".into(), Theme::MODE_DANGER),
            RunState::Running => ("RUNNING".into(), Theme::MODE_BUSY),
            RunState::Done { failed, .. } if *failed > 0 => ("DONE".into(), Theme::MODE_BUSY),
            RunState::Done { .. } => ("DONE".into(), Theme::MODE_DONE),
            RunState::Error(_) => ("ERROR".into(), Theme::MODE_DANGER),
        }
    }

    pub fn context(&self) -> Vec<Span<'static>> {
        let mut spans: Vec<Span<'static>> = vec![Span::styled(
            format!("group {}", self.group.label()),
            Style::default()
                .fg(Theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )];

        match &self.run_state {
            RunState::Planned => {
                if let Some(p) = &self.plan {
                    let counts = count_risks(p);
                    let kept = p.actions.len() - self.skipped.len();
                    let kept_size: u64 = (0..p.actions.len())
                        .filter(|i| !self.skipped.contains(i))
                        .map(|i| match &p.actions[i] {
                            CleanAction::Delete { size, .. } => *size,
                            CleanAction::RunExternal { estimated_size, .. } => {
                                estimated_size.unwrap_or(0)
                            }
                        })
                        .sum();
                    let kept_risk = (0..p.actions.len())
                        .filter(|i| !self.skipped.contains(i))
                        .map(|i| action_risk(p, i))
                        .max()
                        .unwrap_or(RiskLevel::Trivial);

                    spans.push(Span::styled(
                        format!("  ·  {kept}/{} actions", p.actions.len()),
                        Style::default().fg(Theme::FG_DIM),
                    ));
                    spans.push(Span::styled(
                        format!("  ·  ≈ {}", format_size(kept_size, DECIMAL)),
                        Style::default().fg(Theme::WARNING),
                    ));
                    if counts[2] > 0 {
                        spans.push(Span::styled(
                            format!("  ·  ⚠ {} mod", counts[2]),
                            Style::default().fg(Theme::WARNING),
                        ));
                    }
                    if counts[3] > 0 {
                        spans.push(Span::styled(
                            format!("  ·  ⚠ {} dang", counts[3]),
                            Style::default().fg(Theme::DANGER),
                        ));
                    }
                    spans.push(Span::styled(
                        format!("  ·  risk: {kept_risk:?}"),
                        Style::default().fg(risk_color(kept_risk)),
                    ));
                    if self.filter != RiskFilter::All {
                        spans.push(Span::styled(
                            format!("  ·  filter:{}", self.filter.label()),
                            Style::default().fg(Theme::ACCENT),
                        ));
                    }
                }
            }
            RunState::Running => {
                let sp = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let s = sp[self.tick_count % sp.len()];
                spans.push(Span::styled(
                    format!("  ·  {s} {} done", self.log.len()),
                    Style::default().fg(Theme::WARNING),
                ));
            }
            RunState::Done {
                freed,
                succeeded,
                failed,
                ..
            } => {
                spans.push(Span::styled(
                    format!("  ·  freed {}", format_size(*freed, DECIMAL)),
                    Style::default()
                        .fg(Theme::SUCCESS)
                        .add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled(
                    format!("  ·  ✓ {succeeded}"),
                    Style::default().fg(Theme::SUCCESS),
                ));
                if *failed > 0 {
                    spans.push(Span::styled(
                        format!("  ·  ✗ {failed}"),
                        Style::default().fg(Theme::DANGER),
                    ));
                }
            }
            RunState::Building => {
                let sp = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let s = sp[self.tick_count % sp.len()];
                spans.push(Span::styled(
                    format!("  ·  {s} building plan…"),
                    Style::default().fg(Theme::WARNING),
                ));
            }
            _ => {}
        }
        spans
    }

    pub fn hints(&self) -> Vec<KeyHint> {
        match &self.run_state {
            RunState::Idle | RunState::Error(_) => vec![
                KeyHint::new("j/k", "move"),
                KeyHint::new("⏎", "select"),
                KeyHint::new("q", "back"),
            ],
            RunState::Building => vec![KeyHint::new("…", "planning")],
            RunState::Planned => vec![
                KeyHint::new("j/k", "move"),
                KeyHint::new("␣", "skip"),
                KeyHint::new("D", "−dang"),
                KeyHint::new("M", "−mod"),
                KeyHint::new("U", "undo"),
                KeyHint::new("f", "filter"),
                KeyHint::new("⏎", "run"),
                KeyHint::new("p", "rebuild"),
                KeyHint::new("q", "back"),
            ],
            RunState::ConfirmDangerous => vec![
                KeyHint::new("yes", "confirm"),
                KeyHint::new("Esc", "cancel"),
            ],
            RunState::Running => vec![KeyHint::new("…", "running")],
            RunState::Done { .. } => vec![
                KeyHint::new("j/k", "scroll"),
                KeyHint::new("r", "again"),
                KeyHint::new("q", "back"),
            ],
        }
    }
}

fn risk_color(r: RiskLevel) -> Color {
    match r {
        RiskLevel::Trivial => Theme::SUCCESS,
        RiskLevel::Safe => Theme::ACCENT,
        RiskLevel::Moderate => Theme::WARNING,
        RiskLevel::Dangerous => Theme::DANGER,
    }
}

/// Per-row label colour in the plan list. Slightly less saturated than
/// `risk_color` so a long list of Trivial / Safe rows isn't visually loud,
/// while Moderate and Dangerous still pop.
fn row_color(r: RiskLevel) -> Color {
    match r {
        RiskLevel::Trivial => Theme::FG_DIM,
        RiskLevel::Safe => Color::Reset,
        RiskLevel::Moderate => Theme::WARNING,
        RiskLevel::Dangerous => Theme::DANGER,
    }
}
