//! Cleaner page.
//!
//! Idle → p (dry-run preview) → Planned → r/Enter (execute) → Running →
//! Done (scrollable report card).
//!
//! The Done state shows a structured summary (freed / succeeded / failed /
//! skipped) and a scrollable action log so the user can inspect every
//! individual result, including error messages.

use std::sync::Arc;

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
    Done { freed: u64, succeeded: usize, failed: usize, skipped: usize },
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
            LogKind::Failed  => Style::default().fg(Color::Red),
            LogKind::Skipped => Style::default().fg(Color::DarkGray),
            LogKind::Warning => Style::default().fg(Color::Yellow),
        }
    }

    fn prefix(&self) -> &'static str {
        match self.kind {
            LogKind::Success => "✓ ",
            LogKind::Failed  => "✗ ",
            LogKind::Skipped => "– ",
            LogKind::Warning => "⚠ ",
        }
    }
}

// ─── Page ────────────────────────────────────────────────────────────────────

pub struct CleanerPage {
    engine: Arc<Engine>,
    group: CleanGroup,
    plan_list_state: ListState,
    log_scroll: usize,
    run_state: RunState,
    plan: Option<CleanPlan>,
    log: Vec<LogEntry>,
    confirm_dialog: Option<ConfirmDialog>,
    tick_count: usize,
}

impl CleanerPage {
    pub fn new(group: CleanGroup, engine: Arc<Engine>) -> Self {
        Self {
            engine,
            group,
            plan_list_state: ListState::default(),
            log_scroll: 0,
            run_state: RunState::Idle,
            plan: None,
            log: Vec::new(),
            confirm_dialog: None,
            tick_count: 0,
        }
    }

    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
    }

    // ── Rendering ────────────────────────────────────────────────────────────

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(2)])
            .split(area);

        // ── Header ────────────────────────────────────────────────────────
        let state_label = match &self.run_state {
            RunState::Idle             => Span::styled(" idle ", Style::default().fg(Color::DarkGray)),
            RunState::Building         => {
                let sp = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];
                Span::styled(format!(" {} building… ", sp[self.tick_count % sp.len()]),
                    Style::default().fg(Color::Yellow))
            }
            RunState::Planned          => Span::styled(" plan ready ", Style::default().fg(Color::Cyan)),
            RunState::ConfirmDangerous => Span::styled(" confirm ", Style::default().fg(Color::Red)),
            RunState::Running          => {
                let sp = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];
                Span::styled(format!(" {} running… ", sp[self.tick_count % sp.len()]),
                    Style::default().fg(Color::Yellow))
            }
            RunState::Done { .. }      => Span::styled(" done ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            RunState::Error(_)         => Span::styled(" error ", Style::default().fg(Color::Red)),
        };

        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" Clean: {}  ", self.group.as_target()),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            state_label,
        ]))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded));
        f.render_widget(header, chunks[0]);

        // ── Body ─────────────────────────────────────────────────────────
        match &self.run_state {
            RunState::Idle => self.render_idle(f, chunks[1]),
            RunState::Building => {
                let sp = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];
                let s = sp[self.tick_count % sp.len()];
                f.render_widget(
                    Paragraph::new(format!(" {s} Building plan…"))
                        .style(Style::default().fg(Color::Yellow)),
                    chunks[1],
                );
            }
            RunState::Planned | RunState::ConfirmDangerous => {
                self.render_plan(f, chunks[1]);
                if matches!(self.run_state, RunState::ConfirmDangerous) {
                    if let Some(dlg) = &self.confirm_dialog {
                        dlg.render(f, area);
                    }
                }
            }
            RunState::Running => self.render_log(f, chunks[1], true),
            RunState::Done { freed, succeeded, failed, skipped } => {
                let (freed, succeeded, failed, skipped) = (*freed, *succeeded, *failed, *skipped);
                self.render_done(f, chunks[1], freed, succeeded, failed, skipped);
            }
            RunState::Error(e) => {
                let e = e.clone();
                f.render_widget(
                    Paragraph::new(format!(" Error: {e}"))
                        .style(Style::default().fg(Color::Red))
                        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)),
                    chunks[1],
                );
            }
        }

        // ── Footer ────────────────────────────────────────────────────────
        let footer = match &self.run_state {
            RunState::Idle    => " p  dry-run preview   r  run now   q  back ",
            RunState::Planned => " r / Enter  execute   p  rebuild plan   q  back ",
            RunState::Done { .. } => " j/↓ k/↑  scroll log   r  run again   q  back ",
            RunState::Running => " (running…) ",
            _                 => " q  back ",
        };
        f.render_widget(
            Paragraph::new(footer).style(Style::default().fg(Color::DarkGray)),
            chunks[2],
        );
    }

    fn render_idle(&self, f: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("p", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled("  Build a dry-run plan (preview what will be deleted)",
                    Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("r", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled("  Build plan and execute immediately",
                    Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("q", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
                Span::styled("  Back to main menu",
                    Style::default().fg(Color::DarkGray)),
            ]),
        ];
        f.render_widget(
            Paragraph::new(lines)
                .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
                    .title(" Actions ")),
            area,
        );
    }

    fn render_plan(&mut self, f: &mut Frame, area: Rect) {
        let plan = match &self.plan {
            Some(p) => p,
            None => return,
        };

        let risk_color = match plan.risk {
            RiskLevel::Trivial  => Color::Green,
            RiskLevel::Safe     => Color::Cyan,
            RiskLevel::Moderate => Color::Yellow,
            RiskLevel::Dangerous => Color::Red,
        };

        let items: Vec<ListItem> = plan
            .actions
            .iter()
            .take(500)
            .map(|a: &CleanAction| {
                let (label, size) = match a {
                    CleanAction::Delete { path, size, .. } => (path.as_str().to_owned(), *size),
                    CleanAction::RunExternal { cmd, estimated_size } => {
                        (format!("{} {}", cmd.program, cmd.args.join(" ")),
                         estimated_size.unwrap_or(0))
                    }
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:>10}  ", format_size(size, DECIMAL)),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw(label),
                ]))
            })
            .collect();

        let suffix = if plan.actions.len() > 500 {
            format!(" … +{} more ", plan.actions.len() - 500)
        } else {
            String::new()
        };

        let title = format!(
            " {} actions  ≈ {}  risk: {:?}{suffix} ",
            plan.actions.len(),
            format_size(plan.estimated_size, DECIMAL),
            plan.risk,
        );

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
                .title(Span::styled(title, Style::default().fg(risk_color))))
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD))
            .highlight_symbol("▶ ");
        f.render_stateful_widget(list, area, &mut self.plan_list_state);
    }

    fn render_log(&self, f: &mut Frame, area: Rect, live: bool) {
        let title = if live { " Progress " } else { " Action log " };
        let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
            .title(title);
        let inner = block.inner(area);
        f.render_widget(block, area);

        if inner.height == 0 { return; }
        let vis = inner.height as usize;
        let total = self.log.len();

        // For live mode pin to the latest lines; in Done mode respect log_scroll
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

        // Scrollbar
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
        &mut self, f: &mut Frame, area: Rect,
        freed: u64, succeeded: usize, failed: usize, skipped: usize,
    ) {
        // Split into summary card (top) + log (bottom)
        let log_height = area.height.saturating_sub(7).max(3);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(7), Constraint::Length(log_height)])
            .split(area);

        // ── Summary card ─────────────────────────────────────────────────
        let total = succeeded + failed + skipped;
        let success_pct = if total > 0 { succeeded * 100 / total } else { 0 };

        let card_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    format!("Freed    {}", format_size(freed, DECIMAL)),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(format!("✓ {succeeded:>4} succeeded"),
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw("   "),
                Span::styled(format!("✗ {failed:>4} failed"),
                    if failed > 0 {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }),
                Span::raw("   "),
                Span::styled(format!("– {skipped:>4} skipped"),
                    Style::default().fg(Color::DarkGray)),
                Span::raw(format!("   ({success_pct}% ok)")),
            ]),
            Line::from(""),
        ];

        let card_border_color = if failed == 0 { Color::Green } else { Color::Yellow };
        f.render_widget(
            Paragraph::new(card_lines)
                .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(card_border_color))
                    .title(Span::styled(" Summary ", Style::default().fg(card_border_color)))),
            chunks[0],
        );

        // ── Scrollable action log ────────────────────────────────────────
        self.render_log(f, chunks[1], false);
    }

    // ── Event handling ───────────────────────────────────────────────────────

    pub async fn handle_event(&mut self, evt: &Event) -> PageAction {
        // Confirm dialog intercepts all keys when visible
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

        let Event::Key(k) = evt else { return PageAction::None };
        if k.kind != KeyEventKind::Press { return PageAction::None; }

        // In Done state j/k scroll the log
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
                    return PageAction::None;
                }
                KeyCode::Char('q') | KeyCode::Esc | KeyCode::Backspace => {
                    return PageAction::Pop;
                }
                _ => return PageAction::None,
            }
        }

        match k.code {
            KeyCode::Char('p') => { self.build_plan(true).await; }
            KeyCode::Char('r') | KeyCode::Enter => match &self.run_state {
                RunState::Idle | RunState::Error(_) => { self.build_and_run().await; }
                RunState::Planned => { self.maybe_confirm_and_run().await; }
                _ => {}
            },
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Backspace => return PageAction::Pop,
            // Plan list scroll
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(p) = &self.plan {
                    let i = self.plan_list_state.selected().unwrap_or(0);
                    self.plan_list_state.select(Some((i + 1).min(p.actions.len().saturating_sub(1))));
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.plan_list_state.selected().unwrap_or(0);
                self.plan_list_state.select(Some(i.saturating_sub(1)));
            }
            _ => {}
        }
        PageAction::None
    }

    // ── Execution helpers ────────────────────────────────────────────────────

    async fn build_plan(&mut self, dry_run: bool) {
        self.run_state = RunState::Building;
        let target = self.group.as_target();
        let mut config = self.engine.config.clone();
        config.dry_run = dry_run;
        let engine = Engine::new(config, Arc::clone(&self.engine.distro));
        match engine.build_plan(&[target]).await {
            Ok(plan) => {
                self.plan = Some(plan);
                self.run_state = RunState::Planned;
            }
            Err(e) => { self.run_state = RunState::Error(e.to_string()); }
        }
    }

    async fn build_and_run(&mut self) {
        self.build_plan(false).await;
        if self.run_state == RunState::Planned {
            self.maybe_confirm_and_run().await;
        }
    }

    async fn maybe_confirm_and_run(&mut self) {
        let risk = self.plan.as_ref().map(|p| p.risk).unwrap_or(RiskLevel::Trivial);
        if risk >= RiskLevel::Dangerous {
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

        self.run_state = RunState::Running;
        self.log.clear();
        self.log_scroll = 0;

        let (tx, mut rx) = tokio::sync::mpsc::channel::<ProgressEvent>(512);
        let mut config = self.engine.config.clone();
        config.dry_run = dry_run;
        let engine = Arc::new(Engine::new(config, Arc::clone(&self.engine.distro)));
        let confirmer = Arc::new(wisp_engine::AutoApproveConfirmer);

        let handle = tokio::spawn(async move {
            engine.execute(plan, confirmer, tx).await
        });

        while let Some(evt) = rx.recv().await {
            match evt {
                ProgressEvent::ActionFinished { result, .. } => {
                    let entry = match result {
                        ActionResult::Success { bytes_freed } => LogEntry {
                            kind: LogKind::Success,
                            text: format!("freed {}", format_size(bytes_freed, DECIMAL)),
                        },
                        ActionResult::Failed { error } => LogEntry {
                            kind: LogKind::Failed,
                            text: error,
                        },
                        ActionResult::Skipped { reason } => LogEntry {
                            kind: LogKind::Skipped,
                            text: format!("skipped: {reason}"),
                        },
                    };
                    self.log.push(entry);
                }
                ProgressEvent::Warning(w) => {
                    self.log.push(LogEntry { kind: LogKind::Warning, text: w });
                }
                _ => {}
            }
        }

        match handle.await {
            Ok(Ok(report)) => {
                // Pin log scroll to end of failures for immediate visibility
                let first_fail = self.log.iter().position(|e| e.kind == LogKind::Failed);
                self.log_scroll = first_fail.unwrap_or(0);

                self.run_state = RunState::Done {
                    freed:     report.bytes_freed,
                    succeeded: report.succeeded,
                    failed:    report.failed,
                    skipped:   report.skipped,
                };
            }
            Ok(Err(e)) => { self.run_state = RunState::Error(e.to_string()); }
            Err(e)     => { self.run_state = RunState::Error(e.to_string()); }
        }
    }
}
