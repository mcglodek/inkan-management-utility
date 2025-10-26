use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Frame,
    style::Style,
    text::Line,
    widgets::{Paragraph},
};

use crate::app::{AppCtx, ScreenWidget, Transition};
use crate::ui::components::{TextField, draw_frame_title, field_line_text, bool_field_line, submit_line};
use crate::ui::help::help_keygen;
use crate::defaults::Defaults;
use crate::{commands, screens::ResultScreen};

pub struct KeygenScreen {
    count: TextField,
    save_to_file: bool,
    out_path: TextField,
    field_index: usize,
}
impl KeygenScreen {
    pub fn new() -> Self {
        Self {
            count: TextField::with(Defaults::KEYGEN_COUNT),
            save_to_file: Defaults::KEYGEN_SAVE_TO_FILE,
            out_path: TextField::with(Defaults::KEYGEN_OUT_PATH),
            field_index: 0,
        }
    }
}
impl Default for KeygenScreen { fn default() -> Self { Self::new() } }

#[async_trait]
impl ScreenWidget for KeygenScreen {
    fn title(&self) -> &str { "Keygen" }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, _ctx: &AppCtx) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([Constraint::Length(3), Constraint::Min(8), Constraint::Length(3)].as_ref())
            .split(size);

        let header = Paragraph::new("Generate Ethereum/Nostr keypairs (offline)")
            .block(draw_frame_title(self.title()));

        let submit_idx = if self.save_to_file { 3 } else { 2 };

        let mut lines: Vec<Line> = vec![
            field_line_text("Count", &self.count, self.field_index == 0),
            bool_field_line("Save to file?", self.save_to_file, self.field_index == 1),
        ];
        if self.save_to_file {
            lines.push(field_line_text("Output path", &self.out_path, self.field_index == 2));
        }
        lines.push(Line::from(""));
        lines.push(submit_line(self.field_index == submit_idx, "Submit"));

        let help = help_keygen();

        let form = Paragraph::new(lines).block(draw_frame_title("Inputs")).style(Style::default());

        f.render_widget(header, chunks[0]);
        f.render_widget(form, chunks[1]);
        f.render_widget(help, chunks[2]);
    }

    async fn on_key(&mut self, k: KeyEvent, ctx: &mut AppCtx) -> Result<Transition> {
        let submit_idx = if self.save_to_file { 3 } else { 2 };

        match k.code {
            KeyCode::Esc => return Ok(Transition::Pop),

            // Navigation (Up/Down/Tab only)
            KeyCode::Up => { if self.field_index == 0 { self.field_index = submit_idx; } else { self.field_index -= 1; } }
            KeyCode::Down | KeyCode::Tab => { self.field_index = (self.field_index + 1) % (submit_idx + 1); }

            // Enter ONLY submits when on [Submit]
            KeyCode::Enter if self.field_index == submit_idx => {
                let count: u32 = self.count.text.trim().parse().map_err(|_| anyhow!("Count must be a positive integer"))?;
                let records = commands::keygen::generate(count)?;
                if self.save_to_file {
                    let p = self.out_path.text.trim();
                    commands::keygen::emit(records, Some(p.into())).with_context(|| format!("writing {}", p))?;
                    ctx.result_text = format!("✓ Wrote {}", p);
                } else {
                    let json = serde_json::to_string_pretty(&records)?;
                    ctx.result_text = json;
                }
                return Ok(Transition::Push(Box::new(ResultScreen::default())));
            }

            // Checkbox toggle
            KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right if self.field_index == 1 => {
                self.save_to_file = !self.save_to_file;
            }

            // Cursor movement
            KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End if (self.field_index == 0) || (self.save_to_file && self.field_index == 2) => {
                match self.field_index {
                    0 => match k.code { KeyCode::Left => self.count.move_left(), KeyCode::Right => self.count.move_right(), KeyCode::Home => self.count.home(), KeyCode::End => self.count.end(), _ => {} },
                    2 => match k.code { KeyCode::Left => self.out_path.move_left(), KeyCode::Right => self.out_path.move_right(), KeyCode::Home => self.out_path.home(), KeyCode::End => self.out_path.end(), _ => {} },
                    _ => {}
                }
            }

            // Editing
            KeyCode::Backspace if (self.field_index == 0) || (self.save_to_file && self.field_index == 2) => {
                if self.field_index == 0 { self.count.backspace(); } else { self.out_path.backspace(); }
            }
            KeyCode::Delete if (self.field_index == 0) || (self.save_to_file && self.field_index == 2) => {
                if self.field_index == 0 { self.count.delete(); } else { self.out_path.delete(); }
            }
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) && ((self.field_index == 0) || (self.save_to_file && self.field_index == 2)) => {
                if self.field_index == 0 { self.count.insert_char(c); } else { self.out_path.insert_char(c); }
            }

            _ => {}
        }
        Ok(Transition::Stay)
    }
}

