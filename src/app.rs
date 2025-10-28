use anyhow::Result;
use async_trait::async_trait;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    prelude::Frame,
    widgets::Clear,
    Terminal,
};
use std::collections::HashMap;
use std::io;

use crate::screens::ConfirmQuitScreen;

pub enum Transition {
    Stay,
    Push(Box<dyn ScreenWidget>),
    Pop,
    Replace(Box<dyn ScreenWidget>),
    Quit,
    // NEW: pop multiple screens at once
    PopN(usize),
}

/// Small handoff bucket used when loading a delegation/revocation info file.
/// The file select screen fills this map; the target input screen
/// (Create Delegation or Create Revocation) reads and applies it once, then clears it.
#[derive(Debug, Clone, Default)]
pub struct DelegationPrefill {
    pub map: HashMap<String, String>,
}

#[derive(Default)]
pub struct AppCtx {
    pub result_text: String,

    /// If set, contains key/value pairs loaded from a delegation info file.
    /// The Delegation Input screen should `take()` and apply these once.
    pub pending_delegation_prefill: Option<DelegationPrefill>,

    /// If set, contains key/value pairs loaded from a revocation info file.
    /// The Revocation Input screen should `take()` and apply these once.
    pub pending_revocation_prefill: Option<DelegationPrefill>,
}

#[async_trait]
pub trait ScreenWidget {
    fn title(&self) -> &str { "Inkan" }
    fn draw(&self, f: &mut Frame<'_>, area: Rect, ctx: &AppCtx);

    /// Called before each draw when this screen is on top.
    /// Use this to apply any pending prefill immediately upon returning.
    fn apply_prefill(&mut self, _ctx: &mut AppCtx) {}

    async fn on_key(&mut self, key: KeyEvent, ctx: &mut AppCtx) -> Result<Transition>;
}

pub async fn run_menu() -> Result<()> {
    // terminal init
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?; // clean start

    let mut ctx = AppCtx::default();
    let mut stack: Vec<Box<dyn ScreenWidget>> = vec![Box::new(crate::screens::MainMenuScreen::default())];

    loop {
        // Allow the top screen to apply any pending prefill before rendering.
        if let Some(top) = stack.last_mut() {
            top.apply_prefill(&mut ctx);
        }

        terminal.draw(|f| {
            let size = f.size();
            if let Some(top) = stack.last() {
                top.draw(f, size, &ctx);
            } else {
                // just in case—clear remaining area
                f.render_widget(Clear, size);
            }
        })?;

        if event::poll(std::time::Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => {
                    // GLOBAL HOTKEY: Ctrl+Q shows confirm quit from anywhere
                    if k.modifiers.contains(KeyModifiers::CONTROL) && matches!(k.code, KeyCode::Char('q' | 'Q')) {
                        stack.push(Box::new(ConfirmQuitScreen::new()));
                        continue;
                    }

                    if let Some(top) = stack.last_mut() {
                        match top.on_key(k, &mut ctx).await? {
                            Transition::Stay => {}
                            Transition::Push(s) => stack.push(s),
                            Transition::Pop => {
                                stack.pop();
                                if stack.is_empty() {
                                    break;
                                }
                            }
                            Transition::Replace(s) => {
                                stack.pop();
                                stack.push(s);
                            }
                            Transition::Quit => break,
                            // pop multiple levels
                            Transition::PopN(n) => {
                                for _ in 0..n {
                                    if stack.pop().is_none() { break; }
                                }
                                if stack.is_empty() {
                                    break;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // restore
    disable_raw_mode()?;
    let out = terminal.backend_mut();
    execute!(out, LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
