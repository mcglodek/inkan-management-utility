use anyhow::{anyhow, Context, Result}; // UPDATED
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

// NEW: wire to your existing commands
use crate::commands::keygen;
use crate::commands::key_save::{emit_encrypted_one_modern, emit_encrypted_one_pgp, EncryptedSaveOptions};

const CURSOR_BLOCK: &str = "█";

#[derive(Default)]
pub struct CreateKeyPairScreen {
    field_index: usize,     // 0..=2 text fields, 3 show password, 4 text (out dir), 5 method toggle, 6 submit, 7 cancel
    nickname: TextField,
    password: TextField,
    confirm: TextField,
    out_dir: TextField,
    format_modern: bool,    // true = Argon2id + XChaCha20-Poly1305, false = OpenPGP
    show_password: bool,    // NEW: show/hide password fields
}

impl CreateKeyPairScreen {
    pub fn new() -> Self {
        let mut s = Self::default();
        s.out_dir = TextField::with(Defaults::CREATE_KEYPAIR_OUT_DIR);
        s.format_modern = true;
        s.show_password = false;
        s
    }

    fn is_text(&self) -> bool { matches!(self.field_index, 0 | 1 | 2 | 4) }

    fn tf_mut(&mut self, idx: usize) -> &mut TextField {
        match idx {
            0 => &mut self.nickname,
            1 => &mut self.password,
            2 => &mut self.confirm,
            4 => &mut self.out_dir,   // moved from 3 -> 4
            _ => unreachable!("tf_mut called on non-text field"),
        }
    }

    fn tf_ref(&self, idx: usize) -> &TextField {
        match idx {
            0 => &self.nickname,
            1 => &self.password,
            2 => &self.confirm,
            4 => &self.out_dir,       // moved from 3 -> 4
            _ => unreachable!("tf_ref called on non-text field"),
        }
    }

    // Password field that visually matches field_line_text (yellow label and SAME cursor behavior/color).
    // FIX: convert the cursor from char index -> byte index for the temporary TextField to avoid UTF-8 boundary panics.
    fn field_line_password(label: &str, tf: &TextField, selected: bool, show: bool) -> Line<'static> {
        // Determine the text to render (masked or plain)
        let render = if show {
            tf.text.clone()
        } else {
            "•".repeat(tf.text.chars().count())
        };

        // Build a temporary TextField with the rendered text and a BYTE-INDEX cursor
        let mut tmp = TextField::with(&render);

        // Clamp the original cursor as a CHAR index to the rendered length
        let cursor_chars = tf.cursor.min(render.chars().count());

        // Convert char index -> byte index safely
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

        // Delegate to the shared renderer so the cursor looks/behaves exactly like in "Key Pair Name"
        field_line_text(label, &tmp, selected)
    }

    // Encryption Method line with yellow label and cyan value when focused.
    fn encryption_method_line(&self, selected: bool) -> Line<'static> {
        let label_span = Span::styled("Encryption Method: ", Style::default().fg(Color::Yellow));
        let val = if self.format_modern { "Argon2id + XChaCha20-Poly1305" } else { "OpenPGP" };

        let val_style = if selected {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        Line::from(vec![label_span, Span::styled(val.to_string(), val_style)])
    }

    // NEW: Show Password toggle line (yellow label; cyan + bold value when focused).
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

    // One horizontal line: < Create Key Pair >   < Cancel >
    fn buttons_line(submit_selected: bool, cancel_selected: bool) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.extend(button_spans("Create Key Pair", submit_selected));
        spans.push(Span::raw("   "));
        spans.extend(button_spans("Cancel", cancel_selected));
        Line::from(spans)
    }
}

#[async_trait]
impl ScreenWidget for CreateKeyPairScreen {
    fn title(&self) -> &str { "" }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, _ctx: &AppCtx) {
        let header_text = "Create Key Pair";
        let explanation_paras = [
            "Generate a new offline Inkan key pair and save it as an encrypted file.",
            "Fill in the fields below. Password must be entered twice. Choose the output directory.",
            "You can choose between Argon2id + XChaCha20-Poly1305 and OpenPGP encryption.",
        ];

        // === TOP BOX ===
        let top_inner_width = size.width.saturating_sub(2*2 + 2 + 2*3) as usize;
        let header_lines = wrap(header_text, top_inner_width).len() as u16;

        let mut exp_lines = 0usize;
        for p in explanation_paras { exp_lines += wrap(p, top_inner_width).len(); }
        let explanation_lines = exp_lines as u16 + (explanation_paras.len().saturating_sub(1) as u16);

        let top_needed = 2 + 2 + header_lines + 1 + explanation_lines;

        // Middle: now we have 8 focusable positions (0..=7) plus spacer
        let middle_rows: u16 = 8 + 1;
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

        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from("")); // empty line above first field
        lines.push(field_line_text("Key Pair Name", self.tf_ref(0), self.field_index == 0));
        lines.push(Self::field_line_password("Password For Output File", self.tf_ref(1), self.field_index == 1, self.show_password));
        lines.push(Self::field_line_password("Confirm Password", self.tf_ref(2), self.field_index == 2, self.show_password));
        lines.push(self.show_password_line(self.field_index == 3)); // directly under Confirm Password
        lines.push(field_line_text("Output Directory", self.tf_ref(4), self.field_index == 4)); // Output Dir at index 4
        lines.push(self.encryption_method_line(self.field_index == 5));
        lines.push(Line::from("")); // spacer
        lines.push(Self::buttons_line(self.field_index == 6, self.field_index == 7));

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

    async fn on_key(&mut self, k: KeyEvent, ctx: &mut AppCtx) -> Result<Transition> {
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
                if self.field_index == 0 { self.field_index = 7; } else { self.field_index -= 1; }
            }
            KeyCode::Down | KeyCode::Tab => {
                self.field_index = (self.field_index + 1) % 8;
            }

            // Enter on buttons
            KeyCode::Enter if self.field_index == 6 => {
                // === SUBMIT: create + encrypt + save ===
                let nickname = self.nickname.text.trim();
                if nickname.is_empty() {
                    ctx.result_text = "Error: Key Pair Name cannot be empty.".to_string();
                    return Ok(Transition::Push(Box::new(crate::screens::ResultScreen::default())));
                }

                let pwd = self.password.text.clone();
                let confirm = self.confirm.text.clone();
                if pwd != confirm {
                    ctx.result_text = "Error: Password and Confirm Password do not match.".to_string();
                    return Ok(Transition::Push(Box::new(crate::screens::ResultScreen::default())));
                }
                if pwd.is_empty() {
                    ctx.result_text = "Error: Password cannot be empty.".to_string();
                    return Ok(Transition::Push(Box::new(crate::screens::ResultScreen::default())));
                }

                let out_dir = self.out_dir.text.trim();
                if out_dir.is_empty() {
                    ctx.result_text = "Error: Output Directory cannot be empty.".to_string();
                    return Ok(Transition::Push(Box::new(crate::screens::ResultScreen::default())));
                }

                // Ensure directory exists
                let out_dir_path = PathBuf::from(out_dir);
                fs::create_dir_all(&out_dir_path)
                    .with_context(|| format!("creating directory {}", out_dir_path.display()))?;

                // Generate exactly one KeyRecord
                let rec = {
                    let v = keygen::generate(1).with_context(|| "generating keypair")?;
                    v.into_iter().next().ok_or_else(|| anyhow!("internal: expected one key"))?
                };

                // Build file path: "<out_dir>/<sanitized-nickname>.<ext>"
                let ext = if self.format_modern { "inkan" } else { "pgp" };
                let filename = format!("{}.{}", sanitize_filename(nickname), ext);
                let file_path = out_dir_path.join(filename);

                // Password bytes (will be zeroized by savers)
                let mut password_utf8 = pwd.into_bytes();

                // Encrypt & save
                if self.format_modern {
                    // Tune Argon2 here if desired (dev vs prod presets)
                    let opts = EncryptedSaveOptions {
                        out_path: file_path.to_str().ok_or_else(|| anyhow!("invalid output path"))?,
                        nickname,
                        password_utf8: &mut password_utf8,
                        argon_t_cost: 3,
                        argon_m_cost_kib: 262_144, // 256 MiB
                        argon_p_cost: 1,
                        add_noise_prefix: true,
                    };
                    emit_encrypted_one_modern(&rec, opts)
                        .with_context(|| format!("writing {}", file_path.display()))?;
                } else {
                    emit_encrypted_one_pgp(&rec, file_path.to_str().ok_or_else(|| anyhow!("invalid output path"))?, nickname, &mut password_utf8)
                        .with_context(|| format!("writing {}", file_path.display()))?;
                }

                ctx.result_text = format!("✓ Created and saved encrypted key file:\n{}", file_path.display());
                return Ok(Transition::Push(Box::new(crate::screens::ResultScreen::default())));
            }
            KeyCode::Enter if self.field_index == 7 => {
                return Ok(Transition::Pop);
            }

            // Toggle Encryption Method (index 5)
            KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right if self.field_index == 5 => {
                self.format_modern = !self.format_modern;
            }

            // Toggle Show Password (index 3)
            KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right if self.field_index == 3 => {
                self.show_password = !self.show_password;
            }

            // Cursor movement within text fields (same as batch.rs)
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

fn tf_cursor(tf: &TextField) -> usize { tf.cursor }

fn split_at_char(s: &str, idx: usize) -> (&str, &str) {
    if idx == 0 { return ("", s); }
    let count = s.chars().count();
    if idx >= count { return (s, ""); }
    let split = s.char_indices().nth(idx).map(|(i, _)| i).unwrap_or_else(|| s.len());
    (&s[..split], &s[split..])
}

// Simple filesystem-safe name (keeps ASCII letters, numbers, '-', '_', '.')
fn sanitize_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('_');
        }
        // drop everything else
    }
    if out.is_empty() { "keypair".to_string() } else { out }
}
