use anyhow::{Context, Result};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Frame,
    style::Style,
    text::Line,
    widgets::Paragraph,
};
use std::fs;

use crate::app::{AppCtx, ScreenWidget, Transition};
use crate::ui::components::{TextField, draw_frame_title, field_line_text, submit_line};
use crate::ui::help::help_batch;
use crate::defaults::Defaults;
use crate::abi::load_abi;
use crate::process::{process_item, BatchOpts};
use crate::screens::ResultScreen;

pub struct BatchScreen {
    batch_path: TextField,
    out_path: TextField,
    gas_limit: TextField,
    max_fee_per_gas: TextField,
    max_priority_fee_per_gas: TextField,
    field_index: usize,
}
impl BatchScreen {
    pub fn new() -> Self {
        Self {
            batch_path: TextField::with(Defaults::BATCH_INPUT_PATH),
            out_path: TextField::with(Defaults::BATCH_OUTPUT_PATH),
            gas_limit: TextField::with(Defaults::BATCH_GAS_LIMIT),
            max_fee_per_gas: TextField::with(Defaults::BATCH_MAX_FEE_PER_GAS),
            max_priority_fee_per_gas: TextField::with(Defaults::BATCH_MAX_PRIORITY_FEE_PER_GAS),
            field_index: 0,
        }
    }
    fn is_text(&self) -> bool { self.field_index <= 4 }
    fn tf_mut(&mut self, idx: usize) -> &mut TextField {
        match idx {
            0 => &mut self.batch_path,
            1 => &mut self.out_path,
            2 => &mut self.gas_limit,
            3 => &mut self.max_fee_per_gas,
            4 => &mut self.max_priority_fee_per_gas,
            _ => unreachable!(),
        }
    }
}
impl Default for BatchScreen { fn default() -> Self { Self::new() } }

#[async_trait]
impl ScreenWidget for BatchScreen {
    fn title(&self) -> &str { "Batch" }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, _ctx: &AppCtx) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([Constraint::Length(3), Constraint::Min(11), Constraint::Length(3)].as_ref())
            .split(size);

        let header = Paragraph::new("Sign a JSON array of EIP-1559 calls (offline)")
            .block(draw_frame_title(self.title()));

        let mut lines: Vec<Line> = vec![
            field_line_text("Batch path", &self.batch_path, self.field_index == 0),
            field_line_text("Output path", &self.out_path, self.field_index == 1),
            field_line_text("Gas limit", &self.gas_limit, self.field_index == 2),
            field_line_text("Max fee per gas (wei)", &self.max_fee_per_gas, self.field_index == 3),
            field_line_text("Max priority fee per gas (wei)", &self.max_priority_fee_per_gas, self.field_index == 4),
        ];
        lines.push(Line::from(""));
        lines.push(submit_line(self.field_index == 5, "Submit"));

        let help = help_batch();
        let form = Paragraph::new(lines).block(draw_frame_title("Inputs")).style(Style::default());

        f.render_widget(header, chunks[0]);
        f.render_widget(form, chunks[1]);
        f.render_widget(help, chunks[2]);
    }

    async fn on_key(&mut self, k: KeyEvent, ctx: &mut AppCtx) -> Result<Transition> {
        match k.code {
            KeyCode::Esc => return Ok(Transition::Pop),

            // Navigation
            KeyCode::Up => { if self.field_index == 0 { self.field_index = 5; } else { self.field_index -= 1; } }
            KeyCode::Down | KeyCode::Tab => { self.field_index = (self.field_index + 1) % 6; }

            // Enter ONLY submits when on [Submit]
            KeyCode::Enter if self.field_index == 5 => {
                let batch_path = self.batch_path.text.trim().to_string();
                let out_path = self.out_path.text.trim().to_string();

                let abi = load_abi()?;
                let text = fs::read_to_string(&batch_path).with_context(|| format!("reading {}", batch_path))?;
                let items: Vec<crate::types::Item> = serde_json::from_str(&text).context("parsing batch JSON (array)")?;

                let opts = BatchOpts {
                    gas_limit: self.gas_limit.text.trim().to_string(),
                    max_fee_per_gas: self.max_fee_per_gas.text.trim().to_string(),
                    max_priority_fee_per_gas: self.max_priority_fee_per_gas.text.trim().to_string(),
                };

                let mut out_vec: Vec<crate::types::BatchEntryOut> = Vec::with_capacity(items.len());
                for (i, it) in items.iter().enumerate() {
                    let res = process_item(&abi, &opts, it)
                        .await
                        .with_context(|| format!("processing item #{} ({})", i, it.function_to_call));
                    match res {
                        Ok(entry) => out_vec.push(entry),
                        Err(e) => {
                            ctx.result_text = format!("Error: {e:#}");
                            return Ok(Transition::Push(Box::new(ResultScreen::default())));
                        }
                    }
                }

                fs::write(&out_path, serde_json::to_string_pretty(&out_vec)?)
                    .with_context(|| format!("writing {}", out_path))?;

                ctx.result_text = format!("✓ Wrote {}", out_path);
                return Ok(Transition::Push(Box::new(ResultScreen::default())));
            }

            // Cursor movement
            KeyCode::Left if self.is_text() => self.tf_mut(self.field_index).move_left(),
            KeyCode::Right if self.is_text() => self.tf_mut(self.field_index).move_right(),
            KeyCode::Home if self.is_text() => self.tf_mut(self.field_index).home(),
            KeyCode::End if self.is_text() => self.tf_mut(self.field_index).end(),

            // Editing
            KeyCode::Backspace if self.is_text() => self.tf_mut(self.field_index).backspace(),
            KeyCode::Delete if self.is_text() => self.tf_mut(self.field_index).delete(),
            KeyCode::Char(c) if self.is_text() && !k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.tf_mut(self.field_index).insert_char(c)
            }

            _ => {}
        }
        Ok(Transition::Stay)
    }
}

