use anyhow::Result;
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    prelude::Frame,
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{AppCtx, ScreenWidget, Transition};
use crate::ui::layout::centered_rect;

#[derive(Default)]
pub struct ResultScreen;

#[async_trait]
impl ScreenWidget for ResultScreen {
    fn title(&self) -> &str { "Result" }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, ctx: &AppCtx) {
        let area = centered_rect(80, 70, size);
        let block = Block::default().borders(Borders::ALL).title(self.title());
        let text = Paragraph::new(ctx.result_text.as_str()).block(block);
        f.render_widget(Clear, area);
        f.render_widget(text, area);
    }

    async fn on_key(&mut self, k: KeyEvent, ctx: &mut AppCtx) -> Result<Transition> {
        match k.code {
            KeyCode::Esc | KeyCode::Enter => { ctx.result_text.clear(); Ok(Transition::Pop) }
            _ => Ok(Transition::Stay),
        }
    }
}

