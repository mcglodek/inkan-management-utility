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
use crate::ui::style; // centralized style

pub struct ConfirmQuitScreen {
    selected: usize, // 0 = Don't Quit, 1 = Quit
}

impl ConfirmQuitScreen {
    pub fn new() -> Self {
        Self { selected: 0 }
    }
}

#[async_trait]
impl ScreenWidget for ConfirmQuitScreen {
    fn title(&self) -> &str {
        "" // continuous top border
    }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, _ctx: &AppCtx) {
        let msg = "Do you really want to quit the Inkan Management Utility?";
        let left_label = "Don't Quit";
        let right_label = "Quit";

        // Compute width
        let btn_len = |label: &str| 4 + label.len(); // "< " + label + " >"
        let buttons_len = btn_len(left_label) + 3 + btn_len(right_label);

        let inner_w_needed = msg.len().max(buttons_len) as u16;
        let inner_width = inner_w_needed.max(36);
        let inner_height = 4;

        let total_w = inner_width + 4;
        let total_h = inner_height + 3;

        let area = centered_rect_abs(total_w, total_h, size);
        let inner = area.inner(&Margin { horizontal: 2, vertical: 1 });

        let vchunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

        // Message line
        let msg_line = Paragraph::new(Line::from(vec![Span::raw(msg)])).alignment(Alignment::Center);

        // Buttons via shared style.rs
        let left_spans = style::button_spans(left_label, self.selected == 0);
        let right_spans = style::button_spans(right_label, self.selected == 1);

        let mut btn_spans = Vec::new();
        btn_spans.extend(left_spans);
        btn_spans.push(Span::raw("   ")); // just spaces, no vertical line
        btn_spans.extend(right_spans);

        let buttons_line = Paragraph::new(Line::from(btn_spans)).alignment(Alignment::Center);

        f.render_widget(Clear, area);
        f.render_widget(Block::default().borders(Borders::ALL).title(self.title()), area);
        f.render_widget(msg_line, vchunks[1]);
        f.render_widget(buttons_line, vchunks[3]);
    }

    async fn on_key(&mut self, k: KeyEvent, _ctx: &mut AppCtx) -> Result<Transition> {
        match k.code {
            KeyCode::Esc => return Ok(Transition::Pop),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => {
                self.selected = 1 - self.selected;
            }
            KeyCode::Enter => {
                return Ok(if self.selected == 1 {
                    Transition::Quit
                } else {
                    Transition::Pop
                });
            }
            _ => {}
        }
        Ok(Transition::Stay)
    }
}
