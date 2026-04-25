//! Application state and main event loop.

use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::Backend;

use wisp_core::CoreResult;
use wisp_engine::Engine;

use crate::pages::{Page, PageAction};

pub struct App {
    pub engine: Arc<Engine>,
    pub page_stack: Vec<Page>,
    pub should_quit: bool,
}

impl App {
    pub fn new(engine: Arc<Engine>) -> Self {
        Self {
            engine,
            page_stack: vec![Page::home()],
            should_quit: false,
        }
    }

    pub async fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> CoreResult<()> {
        loop {
            terminal.draw(|f: &mut ratatui::Frame| {
                if let Some(page) = self.page_stack.last_mut() {
                    page.render(f, f.area());
                }
            }).map_err(|e| wisp_core::CoreError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

            if !event::poll(Duration::from_millis(100))
                .map_err(|e| wisp_core::CoreError::Io(e))?
            {
                // Allow pages to tick (e.g. progress animations)
                if let Some(page) = self.page_stack.last_mut() {
                    page.tick();
                }
                continue;
            }

            let evt = event::read().map_err(wisp_core::CoreError::Io)?;

            // Global quit bindings
            if let Event::Key(k) = &evt {
                if k.kind == KeyEventKind::Press {
                    match (k.code, k.modifiers) {
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                            self.should_quit = true;
                        }
                        (KeyCode::Char('q'), KeyModifiers::NONE)
                        | (KeyCode::Esc, _)
                            if self.page_stack.len() == 1 =>
                        {
                            self.should_quit = true;
                        }
                        _ => {}
                    }
                }
            }

            if self.should_quit {
                break;
            }

            // Delegate event to current page
            let action = if let Some(page) = self.page_stack.last_mut() {
                page.handle_event(&evt).await
            } else {
                PageAction::None
            };

            self.apply_action(action).await;

            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    async fn apply_action(&mut self, action: PageAction) {
        match action {
            PageAction::None => {}
            PageAction::Quit => self.should_quit = true,
            PageAction::Push(page) => self.page_stack.push(page),
            PageAction::Pop => {
                if self.page_stack.len() > 1 {
                    self.page_stack.pop();
                } else {
                    self.should_quit = true;
                }
            }
            PageAction::Replace(page) => {
                self.page_stack.pop();
                self.page_stack.push(page);
            }
            PageAction::PushAnalyzer(path) => {
                let page = Page::analyzer(path, Arc::clone(&self.engine));
                self.page_stack.push(page);
            }
            PageAction::PushCleaner(group) => {
                let page = Page::cleaner(group, Arc::clone(&self.engine));
                self.page_stack.push(page);
            }
            PageAction::PushHistory => {
                let page = Page::history();
                self.page_stack.push(page);
            }
        }
    }
}
