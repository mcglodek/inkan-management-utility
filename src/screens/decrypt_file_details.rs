use anyhow::{Context, Result};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use textwrap::wrap;

use std::fs;
use std::path::PathBuf;

use crate::app::{AppCtx, ScreenWidget, Transition};
use crate::ui::layout::{three_box_layout, Margins};
use crate::ui::style::{span_key, span_sep, span_text, button_spans};
use crate::ui::common_nav::esc_to_back;
use crate::ui::components::{TextField, field_line_text};
use crate::screens::{ConfirmOkScreen, AfterOk};
use crate::commands::decrypt_auto::decrypt_auto;
use crate::defaults::Defaults;


pub struct DecryptFileDetailsScreen {
    // indices: 0 password, 1 show pwd toggle, 2 out dir, 3 submit, 4 cancel
    field_index: usize,
    input_path: PathBuf,
    password: TextField,
    out_dir: TextField,
    show_password: bool,
}

impl DecryptFileDetailsScreen {
    pub fn new(input_path: PathBuf) -> Self {
        // Instead of deriving from the input path, always start from the central default.
        let default_out_dir = Defaults::DECRYPT_OUTPUT_DIR.to_string();

        Self {
            field_index: 0,
            input_path,
            password: TextField::with(""),
            out_dir: TextField::with(&default_out_dir),
            show_password: false,
        }
    }


    fn is_text(&self) -> bool { matches!(self.field_index, 0 | 2) }

    fn field_line_password(label: &str, tf: &TextField, selected: bool, show: bool) -> Line<'static> {
        let render = if show { tf.text.clone() } else { "•".repeat(tf.text.chars().count()) };

        let mut tmp = TextField::with(&render);
        let cursor_chars = tf.cursor.min(render.chars().count());
        let cursor_bytes = if cursor_chars == 0 {
            0
        } else {
            render
                .char_indices()
                .nth(cursor_chars)
                .map(|(i, _)| i)
                .unwrap_or_else(|| render.len())
        };
        tmp.cursor = cursor_bytes;

        field_line_text(label, &tmp, selected)
    }

    fn show_password_line(&self, selected: bool) -> Line<'static> {
        let label_span = Span::styled("Show Password: ", Style::default().fg(Color::Yellow));
        let val = if self.show_password { "On" } else { "Off" };
        let val_style = if selected {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        Line::from(vec![label_span, Span::styled(val.to_string(), val_style)])
    }

    fn buttons_line(submit_selected: bool, cancel_selected: bool) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.extend(button_spans("Decrypt File", submit_selected));
        spans.push(Span::raw("   "));
        spans.extend(button_spans("Cancel", cancel_selected));
        Line::from(spans)
    }
}

#[async_trait]
impl ScreenWidget for DecryptFileDetailsScreen {
    fn title(&self) -> &str { "" }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, _ctx: &AppCtx) {
        let header_text = "Decrypt File";
        let explanation_paras = [
            "Confirm the file and enter the decryption parameters.",
            &format!("Input File Path: {}", self.input_path.display()),
        ];

        // TOP sizing
        let top_inner_width = size.width.saturating_sub(2*2 + 2 + 2*3) as usize;
        let header_lines = wrap(header_text, top_inner_width).len() as u16;

        let mut exp_lines = 0usize;
        for p in explanation_paras { exp_lines += wrap(p, top_inner_width).len(); }
        let explanation_lines = exp_lines as u16 + (explanation_paras.len().saturating_sub(1) as u16);
        let top_needed = 2 + 2 + header_lines + 1 + explanation_lines;

        // Middle rows: spacer + password + show + outdir + spacer + buttons
        let middle_rows: u16 = 5 + 1;
        let middle_needed = 2 + 2 + middle_rows;

        let footer_height = 3;

        let regions = three_box_layout(
            size, top_needed, middle_needed, footer_height,
            Margins { page: 2, inner_top: 3, inner_middle: 3, inner_bottom: 3 }
        );

        // TOP
        f.render_widget(Block::default().borders(Borders::ALL), regions.top);
        let top_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(header_lines.max(1)), Constraint::Length(1), Constraint::Min(1)])
            .split(regions.top_inner);

        let header_para = Paragraph::new(header_text).alignment(Alignment::Center).wrap(Wrap { trim: true });

        let mut expl_lines: Vec<Line> = Vec::new();
        for (i, p) in explanation_paras.iter().enumerate() {
            for seg in wrap(p, top_inner_width) { expl_lines.push(Line::from(seg.to_string())); }
            if i + 1 < explanation_paras.len() { expl_lines.push(Line::from("")); }
        }
        let explanation_para = Paragraph::new(expl_lines).alignment(Alignment::Left).wrap(Wrap { trim: true });

        f.render_widget(header_para, top_chunks[0]);
        f.render_widget(explanation_para, top_chunks[2]);

        // MIDDLE
        f.render_widget(Block::default().borders(Borders::ALL), regions.middle);
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));
        lines.push(Self::field_line_password("Password", &self.password, self.field_index == 0, self.show_password));
        lines.push(self.show_password_line(self.field_index == 1));
        lines.push(field_line_text("Output Directory", &self.out_dir, self.field_index == 2));
        lines.push(Line::from(""));
        lines.push(Self::buttons_line(self.field_index == 3, self.field_index == 4));

        f.render_widget(Paragraph::new(lines), regions.middle_inner);

        // FOOTER legend (keep Toggle for Show Password)
        f.render_widget(Block::default().borders(Borders::ALL), regions.bottom);
        let footer_line = Line::from(vec![
            span_key("↑/↓/Tab"), span_text(" Navigate"), span_sep(),
            span_key("←/→/Space"), span_text(" Toggle"), span_sep(),
            span_key("Enter"),   span_text(" Select"), span_sep(),
            span_key("Esc"),     span_text(" Back"), span_sep(),
            span_key("Ctrl+Q"),  span_text(" Quit"),
        ]);
        f.render_widget(Paragraph::new(footer_line).wrap(Wrap { trim: true }), regions.bottom_inner);
    }

    async fn on_key(&mut self, k: KeyEvent, _ctx: &mut AppCtx) -> Result<Transition> {
        if let Some(t) = esc_to_back(k) { return Ok(t); }

        if let KeyCode::Char('q') = k.code {
            if k.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(Transition::Push(Box::new(crate::screens::ConfirmQuitScreen::new())));
            }
        }

        match k.code {
            // Navigation
            KeyCode::Up => {
                if self.field_index == 0 { self.field_index = 4; } else { self.field_index -= 1; }
            }
            KeyCode::Down | KeyCode::Tab => {
                self.field_index = (self.field_index + 1) % 5;
            }

            // Enter on Decrypt
            KeyCode::Enter if self.field_index == 3 => {
                let pwd = self.password.text.clone();
                if pwd.is_empty() {
                    return Ok(Transition::Push(Box::new(
                        ConfirmOkScreen::new("Error: Password cannot be empty.").with_after_ok(AfterOk::Pop)
                    )));
                }

                let out_dir = self.out_dir.text.trim();
                if out_dir.is_empty() {
                    return Ok(Transition::Push(Box::new(
                        ConfirmOkScreen::new("Error: Output Directory cannot be empty.").with_after_ok(AfterOk::Pop)
                    )));
                }
                let out_dir_path = PathBuf::from(out_dir);
                fs::create_dir_all(&out_dir_path)
                    .with_context(|| format!("creating directory {}", out_dir_path.display()))?;

                // Call the auto-decrypt orchestrator (tries Modern, then OpenPGP)
                let mut password_utf8 = pwd.into_bytes();
                match decrypt_auto(&self.input_path, &mut password_utf8, &out_dir_path) {
                    Ok((method_label, out_path)) => {
                        let lines = vec![
                            format!("Decryption successful ({}).", method_label),
                            "".to_string(),
                            "Wrote decrypted output to:".to_string(),
                            out_path.display().to_string(),
                        ];
                        return Ok(Transition::Push(Box::new(
                            ConfirmOkScreen::with_lines(lines).with_after_ok(AfterOk::PopToMainMenu)
                        )));
                    }
                    Err(e) => {
                        return Ok(Transition::Push(Box::new(
                            ConfirmOkScreen::new(&e.to_string()).with_after_ok(AfterOk::Pop)
                        )));
                    }
                }
            }

            // Enter on Cancel
            KeyCode::Enter if self.field_index == 4 => {
                return Ok(Transition::Pop);
            }

            // Toggle Show Password
            KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right if self.field_index == 1 => {
                self.show_password = !self.show_password;
            }

            // Text cursor/editing
            KeyCode::Left if self.is_text() => {
                if self.field_index == 0 { self.password.move_left(); } else { self.out_dir.move_left(); }
            }
            KeyCode::Right if self.is_text() => {
                if self.field_index == 0 { self.password.move_right(); } else { self.out_dir.move_right(); }
            }
            KeyCode::Home if self.is_text() => {
                if self.field_index == 0 { self.password.home(); } else { self.out_dir.home(); }
            }
            KeyCode::End if self.is_text() => {
                if self.field_index == 0 { self.password.end(); } else { self.out_dir.end(); }
            }
            KeyCode::Backspace if self.is_text() => {
                if self.field_index == 0 { self.password.backspace(); } else { self.out_dir.backspace(); }
            }
            KeyCode::Delete if self.is_text() => {
                if self.field_index == 0 { self.password.delete(); } else { self.out_dir.delete(); }
            }
            KeyCode::Char(c) if self.is_text() && !k.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.field_index == 0 { self.password.insert_char(c); } else { self.out_dir.insert_char(c); }
            }

            _ => {}
        }

        Ok(Transition::Stay)
    }
}
