// src/screens/confirm_ok.rs
use anyhow::Result;
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    prelude::Frame,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{AppCtx, ScreenWidget, Transition};
use crate::ui::layout::centered_rect_abs;
use crate::ui::style;

// Where to go when user presses OK
pub enum AfterOk {
    Pop,                // just close the modal
    PopToMainMenu,      // replace current screen with Main Menu
}

pub struct ConfirmOkScreen {
    lines: Vec<String>,
    after_ok: AfterOk,
}

impl ConfirmOkScreen {
    pub fn new<L: Into<String>>(line: L) -> Self {
        Self { lines: vec![line.into()], after_ok: AfterOk::Pop }
    }
    pub fn with_lines(lines: Vec<String>) -> Self {
        Self { lines, after_ok: AfterOk::Pop }
    }
    pub fn with_after_ok(mut self, after_ok: AfterOk) -> Self {
        self.after_ok = after_ok;
        self
    }
}

#[async_trait]
impl ScreenWidget for ConfirmOkScreen {
    fn title(&self) -> &str { "" }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, _ctx: &AppCtx) {
        let ok_label = "OK";
        let ok_spans = style::button_spans(ok_label, true); // single button, always selected

        // width: max of content and button
        let content_w = self
            .lines
            .iter()
            .map(|s| s.len())
            .max()
            .unwrap_or(0)
            .max(4 + ok_label.len()) as u16; // "< " + label + " >"

        let inner_width  = content_w.max(36);
        let inner_height = (self.lines.len() as u16).max(1) + 2; // lines + spacer + button
        let total_w = inner_width + 4;
        let total_h = inner_height + 3;

        let area  = centered_rect_abs(total_w, total_h, size);
        let inner = area.inner(&Margin { horizontal: 2, vertical: 1 });

        // vertical layout: lines..., spacer, button row
        let mut constraints = Vec::new();
        for _ in &self.lines {
            constraints.push(Constraint::Length(1));
        }
        constraints.push(Constraint::Length(1)); // spacer
        constraints.push(Constraint::Length(1)); // buttons

        let vchunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        f.render_widget(Clear, area);
        f.render_widget(Block::default().borders(Borders::ALL).title(self.title()), area);

        for (i, text) in self.lines.iter().enumerate() {
            let p = Paragraph::new(Line::from(vec![Span::raw(text)])).alignment(Alignment::Center);
            f.render_widget(p, vchunks[i]);
        }

        let buttons_line = Paragraph::new(Line::from(ok_spans)).alignment(Alignment::Center);
        f.render_widget(buttons_line, vchunks[vchunks.len() - 1]);
    }

    async fn on_key(&mut self, k: KeyEvent, _ctx: &mut AppCtx) -> Result<Transition> {
        match k.code {
            KeyCode::Esc | KeyCode::Enter => {
                Ok(match self.after_ok {
                AfterOk::Pop           => Transition::Pop,   
                AfterOk::PopToMainMenu => Transition::Replace(Box::new(
                    crate::screens::MainMenuScreen::default()
                )),     
                })
            }
            _ => Ok(Transition::Stay),
        }
    }
}
