// src/screens/choose_delegation_info_dir.rs
use anyhow::{Context, Result};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::Frame,
    text::Line,
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
use crate::defaults::Defaults;

// For error popups
use crate::screens::{ConfirmOkScreen, AfterOk};

#[derive(Default)]
pub struct ChooseDelegationInfoDirScreen {
    // indices: 0 = input_dir, 1 = open, 2 = cancel
    field_index: usize,
    input_dir: TextField,
}

impl ChooseDelegationInfoDirScreen {
    pub fn new() -> Self {
        let mut s = Self::default();
        // Match Decrypt behavior but use our Delegation default
        s.input_dir = TextField::with(Defaults::DELEGATION_INPUT_DIR);
        s
    }

    fn is_text(&self) -> bool { self.field_index == 0 }

    fn buttons_line(open_selected: bool, cancel_selected: bool) -> Line<'static> {
        let mut spans: Vec<_> = Vec::new();
        spans.extend(button_spans("Open Directory", open_selected));
        spans.push("   ".into());
        spans.extend(button_spans("Cancel", cancel_selected));
        Line::from(spans)
    }
}

#[async_trait]
impl ScreenWidget for ChooseDelegationInfoDirScreen {
    fn title(&self) -> &str { "" }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, _ctx: &AppCtx) {
        let header_text = "Load Delegation Info – Choose Directory";
        let explanation_paras = [
            "Start by choosing the directory that contains the delegation info file.",
            "Press Enter on “Open Directory” to browse and select a file.",
        ];

        // --- TOP sizing ---
        let top_inner_width = size.width.saturating_sub(2*2 + 2 + 2*3) as usize;
        let header_lines = wrap(header_text, top_inner_width).len() as u16;

        let mut exp_lines = 0usize;
        for p in explanation_paras { exp_lines += wrap(p, top_inner_width).len(); }
        let explanation_lines = exp_lines as u16 + (explanation_paras.len().saturating_sub(1) as u16);

        let top_needed = 2 + 2 + header_lines + 1 + explanation_lines;

        // Middle rows: 1 text field + spacer + two buttons = 1 + 1 + 1
        let middle_rows: u16 = 3 + 1; // with extra spacer line at top
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

        let header_para = Paragraph::new(header_text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        let mut expl_lines: Vec<Line> = Vec::new();
        for (i, p) in explanation_paras.iter().enumerate() {
            for seg in wrap(p, top_inner_width) { expl_lines.push(Line::from(seg.to_string())); }
            if i + 1 < explanation_paras.len() { expl_lines.push(Line::from("")); }
        }
        let explanation_para = Paragraph::new(expl_lines)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true });

        f.render_widget(header_para, top_chunks[0]);
        f.render_widget(explanation_para, top_chunks[2]);

        // MIDDLE
        f.render_widget(Block::default().borders(Borders::ALL), regions.middle);
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));
        lines.push(field_line_text("Input Directory", &self.input_dir, self.field_index == 0));
        lines.push(Line::from("")); // spacer
        lines.push(Self::buttons_line(self.field_index == 1, self.field_index == 2));
        f.render_widget(Paragraph::new(lines), regions.middle_inner);

        // FOOTER legend
        f.render_widget(Block::default().borders(Borders::ALL), regions.bottom);
        let footer_line = Line::from(vec![
            span_key("↑/↓/Tab"), span_text(" Navigate"), span_sep(),
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
            KeyCode::Up => {
                if self.field_index == 0 { self.field_index = 2; } else { self.field_index -= 1; }
            }
            KeyCode::Down | KeyCode::Tab => {
                self.field_index = (self.field_index + 1) % 3;
            }

            // Enter on Open Directory (create if missing, ensure it's a dir)
            KeyCode::Enter if self.field_index == 1 => {
                let dir = self.input_dir.text.trim();
                if dir.is_empty() {
                    return Ok(Transition::Push(Box::new(
                        ConfirmOkScreen::new("Error: Input Directory cannot be empty.")
                            .with_after_ok(AfterOk::Pop)
                    )));
                }
                let dir_path = PathBuf::from(dir);
                fs::create_dir_all(&dir_path)
                    .with_context(|| format!("creating directory {}", dir_path.display()))?;
                let md = fs::metadata(&dir_path)
                    .with_context(|| format!("accessing {}", dir_path.display()))?;
                if !md.is_dir() {
                    return Ok(Transition::Push(Box::new(
                        ConfirmOkScreen::new("Error: Input Directory is not a directory.")
                            .with_after_ok(AfterOk::Pop)
                    )));
                }
                return Ok(Transition::Push(Box::new(
                    crate::screens::SelectDelegationInfoFileScreen::new(dir_path)
                )));
            }

            // Enter on Cancel
            KeyCode::Enter if self.field_index == 2 => {
                return Ok(Transition::Pop);
            }

            // Text editing on directory field
            KeyCode::Left if self.is_text() => self.input_dir.move_left(),
            KeyCode::Right if self.is_text() => self.input_dir.move_right(),
            KeyCode::Home if self.is_text() => self.input_dir.home(),
            KeyCode::End if self.is_text() => self.input_dir.end(),
            KeyCode::Backspace if self.is_text() => self.input_dir.backspace(),
            KeyCode::Delete if self.is_text() => self.input_dir.delete(),
            KeyCode::Char(c) if self.is_text() && !k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input_dir.insert_char(c)
            }

            _ => {}
        }
        Ok(Transition::Stay)
    }
}
