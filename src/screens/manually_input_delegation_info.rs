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
use std::path::{Path, PathBuf};

use crate::app::{AppCtx, ScreenWidget, Transition};
use crate::ui::layout::{three_box_layout, Margins};
use crate::ui::style::{span_key, span_sep, span_text, button_spans};
use crate::ui::common_nav::esc_to_back;
use crate::ui::components::{TextField, field_line_text};
use crate::defaults::Defaults;

// Generic OK-only modal
use crate::screens::{ConfirmOkScreen, AfterOk};

pub struct ManuallyInputDelegationInfoScreen {
    // 0 delegator, 1 delegatee, 2 toggle, 3 nonce, 4 out_dir, 5 submit, 6 back
    field_index: usize,
    delegator_priv: TextField,
    delegatee_priv: TextField,
    require_delegatee_sig_revocation: bool,
    nonce: TextField,
    out_dir: TextField,
}

impl ManuallyInputDelegationInfoScreen {
    pub fn new() -> Self {
        Self {
            field_index: 0,
            delegator_priv: TextField::with(""),
            delegatee_priv: TextField::with(""),
            require_delegatee_sig_revocation: false, // default: no
            nonce: TextField::with(""),
            out_dir: TextField::with(Defaults::CREATE_DELEGATION_OUT_DIR),
        }
    }

    fn is_text(&self) -> bool {
        matches!(self.field_index, 0 | 1 | 3 | 4)
    }

    fn tf_ref(&self, idx: usize) -> &TextField {
        match idx {
            0 => &self.delegator_priv,
            1 => &self.delegatee_priv,
            3 => &self.nonce,
            4 => &self.out_dir,
            _ => unreachable!("tf_ref called on non-text field"),
        }
    }

    fn tf_mut(&mut self, idx: usize) -> &mut TextField {
        match idx {
            0 => &mut self.delegator_priv,
            1 => &mut self.delegatee_priv,
            3 => &mut self.nonce,
            4 => &mut self.out_dir,
            _ => unreachable!("tf_mut called on non-text field"),
        }
    }

    // One horizontal line: < Create Delegation >   < Back >
    fn buttons_line(submit_selected: bool, back_selected: bool) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.extend(button_spans("Create Delegation", submit_selected));
        spans.push(Span::raw("   "));
        spans.extend(button_spans("Back", back_selected));
        Line::from(spans)
    }

    fn build_batch_json(&self) -> Result<String> {
        // Validate text fields
        let pk_x = self.delegator_priv.text.trim();
        let pk_y = self.delegatee_priv.text.trim();
        if pk_x.is_empty() {
            anyhow::bail!("Delegator PrivKey cannot be empty.");
        }
        if pk_y.is_empty() {
            anyhow::bail!("Delegatee PrivKey cannot be empty.");
        }

        // Parse nonce
        let nonce_str = self.nonce.text.trim();
        let nonce: u64 = nonce_str.parse().context("Nonce must be an integer")?;

        // Construct exactly the shape expected by your batch processor
        let obj = serde_json::json!({
            "FUNCTION_TO_CALL": "createDelegationEvent",
            "NONCE": nonce,
            "CHAIN_ID": Defaults::CHAIN_ID,
            "CONTRACT_ADDRESS": Defaults::CONTRACT_ADDRESS,
            "TYPE_A_PRIVKEY_X": pk_x,
            "TYPE_A_PRIVKEY_Y": pk_y,
            "TYPE_A_PUBKEY_Y": "",
            "TYPE_A_UINT_X": 0,
            "TYPE_A_UINT_Y": 0,
            "TYPE_A_BOOLEAN": if self.require_delegatee_sig_revocation { "true" } else { "false" },
        });

        Ok(serde_json::to_string_pretty(&vec![obj])?)
    }

    fn ensure_dir_and_unique_path(base_dir: &Path, base_filename: &str) -> Result<PathBuf> {
        fs::create_dir_all(base_dir)
            .with_context(|| format!("creating directory {}", base_dir.display()))?;

        let mut candidate = base_dir.join(base_filename);
        if !candidate.exists() {
            return Ok(candidate);
        }
        // Append " (1)", " (2)", ...
        let (stem, ext) = split_name_ext(base_filename);
        let mut n: u32 = 1;
        loop {
            let next_name = if ext.is_empty() {
                format!("{stem} ({n})")
            } else {
                format!("{stem} ({n}).{ext}")
            };
            candidate = base_dir.join(next_name);
            if !candidate.exists() {
                return Ok(candidate);
            }
            n += 1;
        }
    }

    fn write_batch_file_to_dir(&self, json_text: &str) -> Result<PathBuf> {
        let out_dir = self.out_dir.text.trim();
        if out_dir.is_empty() {
            anyhow::bail!("Output Directory cannot be empty.");
        }

        let dir = PathBuf::from(out_dir);
        // Use a deterministic base name: "delegation_nonce_<N>.json"
        // (no nickname here; mirrors the unique-name behavior of keypair screen by avoiding overwrite)
        let nonce_str = self.nonce.text.trim();
        let base_filename = format!("delegation_nonce_{}.json", if nonce_str.is_empty() { "unknown" } else { nonce_str });

        let path = Self::ensure_dir_and_unique_path(&dir, &base_filename)?;
        fs::write(&path, json_text)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(path)
    }
}

impl Default for ManuallyInputDelegationInfoScreen {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl ScreenWidget for ManuallyInputDelegationInfoScreen {
    fn title(&self) -> &str { "" }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, _ctx: &AppCtx) {
        let header_text = "Create Delegation — Manual Input";
        let explanation_paras = [
            "Enter the fields below. The app will generate a single-entry batch JSON",
            "for createDelegationEvent and save it into the chosen output directory.",
        ];

        // === TOP BOX ===
        let top_inner_width = size.width.saturating_sub(2*2 + 2 + 2*3) as usize;
        let header_lines = wrap(header_text, top_inner_width).len() as u16;

        let mut exp_lines = 0usize;
        for p in explanation_paras { exp_lines += wrap(p, top_inner_width).len(); }
        let explanation_lines = exp_lines as u16 + (explanation_paras.len().saturating_sub(1) as u16);

        let top_needed = 2 + 2 + header_lines + 1 + explanation_lines;

        // Middle: 7 focusable positions (0..=6) plus spacer
        let middle_rows: u16 = 7 + 1;
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
            .constraints([
                Constraint::Length(header_lines.max(1)),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
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

        // === MIDDLE BOX ===
        f.render_widget(Block::default().borders(Borders::ALL), regions.middle);

        let toggle_val = if self.require_delegatee_sig_revocation { "yes" } else { "no" }; // no brackets
        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from("")); // spacer above first field
        lines.push(field_line_text("Delegator PrivKey", self.tf_ref(0), self.field_index == 0));
        lines.push(field_line_text("Delegatee PrivKey", self.tf_ref(1), self.field_index == 1));

        // toggle line at index 2 (styled like other selects)
        let label_span = Span::styled("Require Delegatee Signature For Revocation?  ", Style::default().fg(Color::Yellow));
        let val_style = if self.field_index == 2 {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(vec![label_span, Span::styled(toggle_val.to_string(), val_style)]));

        lines.push(field_line_text("Transaction Nonce", self.tf_ref(3), self.field_index == 3));
        lines.push(field_line_text("Output Directory", self.tf_ref(4), self.field_index == 4));
        lines.push(Line::from("")); // spacer
        lines.push(Self::buttons_line(self.field_index == 5, self.field_index == 6));

        let middle_para = Paragraph::new(lines);
        f.render_widget(middle_para, regions.middle_inner);

        // === BOTTOM BOX (legend) ===
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
        if let Some(t) = esc_to_back(k) {
            return Ok(t); // Esc -> Back
        }

        if let KeyCode::Char('q') = k.code {
            if k.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(Transition::Push(Box::new(crate::screens::ConfirmQuitScreen::new())));
            }
        }

        match k.code {
            // Navigation
            KeyCode::Up => {
                if self.field_index == 0 { self.field_index = 6; } else { self.field_index -= 1; }
            }
            KeyCode::Down | KeyCode::Tab => {
                self.field_index = (self.field_index + 1) % 7;
            }

            // Toggle boolean (index 2)
            KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right if self.field_index == 2 => {
                self.require_delegatee_sig_revocation = !self.require_delegatee_sig_revocation;
            }

            // Enter on buttons
            KeyCode::Enter if self.field_index == 5 => {
                // Build JSON and write to selected Output Directory
                match self.build_batch_json() {
                    Ok(text) => {
                        match self.write_batch_file_to_dir(&text) {
                            Ok(path) => {
                                let lines = vec![
                                    "Saved delegation JSON for createDelegationEvent:".to_string(),
                                    "".to_string(),
                                    path.display().to_string(),
                                ];
                                return Ok(Transition::Push(Box::new(
                                    ConfirmOkScreen::with_lines(lines).with_after_ok(AfterOk::Pop)
                                )));
                            }
                            Err(e) => {
                                return Ok(Transition::Push(Box::new(
                                    ConfirmOkScreen::new(&format!("Error writing file: {e:#}"))
                                        .with_after_ok(AfterOk::Pop)
                                )));
                            }
                        }
                    }
                    Err(e) => {
                        return Ok(Transition::Push(Box::new(
                            ConfirmOkScreen::new(&format!("Error: {e:#}"))
                                .with_after_ok(AfterOk::Pop)
                        )));
                    }
                }
            }
            KeyCode::Enter if self.field_index == 6 => {
                return Ok(Transition::Pop); // Back
            }

            // Cursor movement in text fields
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

/* ---------- helpers ---------- */

fn split_name_ext(filename: &str) -> (&str, &str) {
    match filename.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => (stem, ext),
        _ => (filename, ""),
    }
}
