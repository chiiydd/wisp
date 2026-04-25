//! Cleaner page.
//!
//! Shows available cleaners for the selected group, lets the user run them
//! in dry-run or real mode, and streams progress.

use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEventKind};
use humansize::{DECIMAL, format_size};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, List, ListItem, ListState, Paragraph,
};

use wisp_core::types::{CleanPlan, ProgressEvent, RiskLevel};
use wisp_engine::Engine;

use crate::widgets::confirm::ConfirmDialog;

use super::{CleanGroup, PageAction};

#[derive(Debug, Clone, PartialEq)]
enum RunState {
    Idle,
    Building,
    Planned,
    ConfirmDangerous,
    Running,
    Done { freed: u64, failed: usize },
    Error(String),
}

pub struct CleanerPage {
    engine: Arc<Engine>,
    group: CleanGroup,
    list_state: ListState,
    run_state: RunState,
    plan: Option<CleanPlan>,
    progress_lines: Vec<String>,
    confirm_dialog: Option<ConfirmDialog>,
    tick_count: usize,
}

impl CleanerPage {
    pub fn new(group: CleanGroup, engine: Arc<Engine>) -> Self {
        Self {
            engine,
            group,
            list_state: ListState::default(),
            run_state: RunState::Idle,
            plan: None,
            progress_lines: Vec::new(),
            confirm_dialog: None,
            tick_count: 0,
        }
    }

    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);

        if self.run_state == RunState::Building {
            self.run_state = RunState::Building; // prevent re-entry signal; actual build is synchronous below
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .split(area);

        // Header
        let title = format!(" Clean: {} ", self.group.as_target());
        let header = Paragraph::new(Line::from(vec![
            Span::styled(title, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded));
        f.render_widget(header, chunks[0]);

        // Body
        match &self.run_state {
            RunState::Idle => {
                let hint = Paragraph::new(vec![
                    Line::from(" Press  p  to build a plan (dry-run preview)"),
                    Line::from(" Press  r  to run for real"),
                    Line::from(" Press  q / Esc  to go back"),
                ])
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded));
                f.render_widget(hint, chunks[1]);
            }
            RunState::Building => {
                let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let s = spinner_chars[self.tick_count % spinner_chars.len()];
                let msg = Paragraph::new(format!(" {s} Building plan…"))
                    .style(Style::default().fg(Color::Yellow));
                f.render_widget(msg, chunks[1]);
            }
            RunState::Planned => {
                self.render_plan(f, chunks[1]);
            }
            RunState::ConfirmDangerous => {
                self.render_plan(f, chunks[1]);
                if let Some(dlg) = &self.confirm_dialog {
                    dlg.render(f, area);
                }
            }
            RunState::Running => {
                self.render_progress(f, chunks[1]);
            }
            RunState::Done { freed, failed } => {
                let freed = *freed;
                let failed = *failed;
                let msg = Paragraph::new(vec![
                    Line::from(Span::styled(
                        format!(" ✓  Done! Freed {}  |  {} failed", format_size(freed, DECIMAL), failed),
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                    )),
                    Line::from(""),
                    Line::from(" Press q or Esc to go back"),
                ])
                .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded));
                f.render_widget(msg, chunks[1]);
            }
            RunState::Error(e) => {
                let msg = Paragraph::new(format!(" Error: {e}"))
                    .style(Style::default().fg(Color::Red))
                    .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded));
                f.render_widget(msg, chunks[1]);
            }
        }

        // Footer
        let footer_text = match &self.run_state {
            RunState::Idle => " p plan (dry-run)   r run   q back ",
            RunState::Planned => " Enter / r  execute plan   n dry-run again   q back ",
            _ => " q back ",
        };
        let footer = Paragraph::new(footer_text).style(Style::default().fg(Color::DarkGray));
        f.render_widget(footer, chunks[2]);
    }

    fn render_plan(&mut self, f: &mut Frame, area: Rect) {
        let plan = match &self.plan {
            Some(p) => p,
            None => return,
        };

        let items: Vec<ListItem> = plan
            .actions
            .iter()
            .take(200) // cap rendering to avoid frame slowdown
            .map(|a: &wisp_core::types::CleanAction| {
                let (label, size) = match a {
                    wisp_core::types::CleanAction::Delete { path, size, .. } => {
                        (path.as_str().to_owned(), *size)
                    }
                    wisp_core::types::CleanAction::RunExternal { cmd, estimated_size } => {
                        (format!("{} {}", cmd.program, cmd.args.join(" ")), estimated_size.unwrap_or(0))
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

        let summary = format!(
            " {} actions  ≈ {}  risk: {:?} ",
            plan.actions.len(),
            format_size(plan.estimated_size, DECIMAL),
            plan.risk,
        );

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(summary),
            )
            .highlight_style(
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_progress(&mut self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .progress_lines
            .iter()
            .rev()
            .take(area.height as usize)
            .rev()
            .map(|l| ListItem::new(l.as_str()))
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Progress "),
        );
        f.render_widget(list, area);
    }

    pub async fn handle_event(&mut self, evt: &Event) -> PageAction {
        // Forward to confirm dialog if active
        if let Some(dlg) = &mut self.confirm_dialog {
            let r = dlg.handle_event(evt);
            match r {
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
        if k.kind != KeyEventKind::Press {
            return PageAction::None;
        }

        match k.code {
            KeyCode::Char('p') | KeyCode::Char('n') => {
                self.build_plan(true).await;
            }
            KeyCode::Char('r') | KeyCode::Enter => match &self.run_state {
                RunState::Idle | RunState::Error(_) | RunState::Done { .. } => {
                    self.build_and_run().await;
                }
                RunState::Planned => {
                    self.maybe_confirm_and_run().await;
                }
                _ => {}
            },
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Backspace => {
                return PageAction::Pop;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(p) = &self.plan {
                    let i = self.list_state.selected().unwrap_or(0);
                    self.list_state.select(Some((i + 1).min(p.actions.len().saturating_sub(1))));
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.list_state.selected().unwrap_or(0);
                self.list_state.select(Some(i.saturating_sub(1)));
            }
            _ => {}
        }
        PageAction::None
    }

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
            Err(e) => {
                self.run_state = RunState::Error(e.to_string());
            }
        }
    }

    async fn build_and_run(&mut self) {
        self.build_plan(true).await;
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
        self.progress_lines.clear();

        let (tx, mut rx) = tokio::sync::mpsc::channel::<ProgressEvent>(256);
        let mut config = self.engine.config.clone();
        config.dry_run = dry_run;
        let engine = Arc::new(Engine::new(config, Arc::clone(&self.engine.distro)));
        let confirmer = Arc::new(wisp_engine::AutoApproveConfirmer);

        let handle = tokio::spawn(async move {
            engine.execute(plan, confirmer, tx).await
        });

        while let Some(evt) = rx.recv().await {
            match &evt {
                ProgressEvent::ActionFinished { result, .. } => {
                    let line = match result {
                        wisp_core::types::ActionResult::Success { bytes_freed } => {
                            format!("✓  freed {}", format_size(*bytes_freed, DECIMAL))
                        }
                        wisp_core::types::ActionResult::Failed { error } => {
                            format!("✗  {error}")
                        }
                        wisp_core::types::ActionResult::Skipped { reason } => {
                            format!("–  skipped ({reason})")
                        }
                    };
                    self.progress_lines.push(line);
                }
                ProgressEvent::Warning(w) => {
                    self.progress_lines.push(format!("⚠  {w}"));
                }
                _ => {}
            }
        }

        match handle.await {
            Ok(Ok(report)) => {
                self.run_state = RunState::Done {
                    freed: report.bytes_freed,
                    failed: report.failed,
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
}
