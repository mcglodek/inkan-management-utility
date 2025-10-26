use anyhow::Result;
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

use crate::app::{AppCtx, ScreenWidget, Transition};
use crate::ui::layout::{three_box_layout, Margins};
use crate::ui::style::{span_key, span_sep, span_text, button_spans};
use crate::ui::common_nav::esc_to_back;
use crate::ui::components::{TextField, field_line_text};
use crate::defaults::Defaults; // ⬅ added

const CURSOR_BLOCK: &str = "█";

#[derive(Default)]
pub struct CreateKeyPairScreen {
    field_index: usize,     // 0..=3 text fields, 4 method toggle, 5 submit, 6 cancel
    nickname: TextField,
    password: TextField,
    confirm: TextField,
    out_dir: TextField,
    format_modern: bool,    // true = Argon2id + XChaCha20-Poly1305, false = OpenPGP AEAD
}

impl CreateKeyPairScreen {
    pub fn new() -> Self {
        let mut s = Self::default();
        s.out_dir = TextField::with(Defaults::CREATE_KEYPAIR_OUT_DIR);
        s.format_modern = true;
        s
    }

    fn is_text(&self) -> bool { self.field_index <= 3 }

    fn tf_mut(&mut self, idx: usize) -> &mut TextField {
        match idx {
            0 => &mut self.nickname,
            1 => &mut self.password,
            2 => &mut self.confirm,
            3 => &mut self.out_dir,
            _ => unreachable!("tf_mut called on non-text field"),
        }
    }

    fn tf_ref(&self, idx: usize) -> &TextField {
        match idx {
            0 => &self.nickname,
            1 => &self.password,
            2 => &self.confirm,
            3 => &self.out_dir,
            _ => unreachable!("tf_ref called on non-text field"),
        }
    }

    fn field_line_password(label: &str, tf: &TextField, selected: bool) -> Line<'static> {
        let label_span = Span::styled(format!("{label}: "), Style::default().fg(Color::Yellow));
        let masked = "•".repeat(tf.text.chars().count());

        if selected {
            let (left, right) = split_at_char(&masked, tf_cursor(tf));
            Line::from(vec![
                label_span,
                Span::raw(left.to_string()),
                Span::styled(CURSOR_BLOCK.to_string(), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(right.to_string()),
            ])
        } else {
            Line::from(vec![label_span, Span::raw(masked)])
        }
    }

    fn encryption_method_line(&self, selected: bool) -> Line<'static> {
        let label_span = Span::styled("Encryption Method: ", Style::default().fg(Color::Yellow));
        let val = if self.format_modern {
            "Argon2id + XChaCha20-Poly1305"
        } else {
            "OpenPGP AEAD"
        };

        let val_style = if selected {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        Line::from(vec![label_span, Span::styled(val.to_string(), val_style)])
    }

    fn buttons_line(submit_selected: bool, cancel_selected: bool) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.extend(button_spans("Submit", submit_selected));
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
            "You can choose between Argon2id + XChaCha20-Poly1305 and OpenPGP AEAD encryption.",
        ];

        let top_inner_width = size.width.saturating_sub(2*2 + 2 + 2*3) as usize;
        let header_lines = wrap(header_text, top_inner_width).len() as u16;

        let mut exp_lines = 0usize;
        for p in explanation_paras { exp_lines += wrap(p, top_inner_width).len(); }
        let explanation_lines = exp_lines as u16 + (explanation_paras.len().saturating_sub(1) as u16);

        let top_needed = 2 + 2 + header_lines + 1 + explanation_lines;
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

        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from("")); // empty line above first field
        lines.push(field_line_text("Key Pair Name", self.tf_ref(0), self.field_index == 0));
        lines.push(Self::field_line_password("Password For Output File", self.tf_ref(1), self.field_index == 1));
        lines.push(Self::field_line_password("Confirm Password", self.tf_ref(2), self.field_index == 2));
        lines.push(field_line_text("Output Directory", self.tf_ref(3), self.field_index == 3));
        lines.push(self.encryption_method_line(self.field_index == 4));
        lines.push(Line::from(""));
        lines.push(Self::buttons_line(self.field_index == 5, self.field_index == 6));

        let middle_para = Paragraph::new(lines);
        f.render_widget(middle_para, regions.middle_inner);

        // FOOTER
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
            return Ok(t);
        }

        if let KeyCode::Char('q') = k.code {
            if k.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(Transition::Push(Box::new(crate::screens::ConfirmQuitScreen::new())));
            }
        }

        match k.code {
            KeyCode::Up => {
                if self.field_index == 0 { self.field_index = 6; } else { self.field_index -= 1; }
            }
            KeyCode::Down | KeyCode::Tab => {
                self.field_index = (self.field_index + 1) % 7;
            }

            KeyCode::Enter if self.field_index == 5 => return Ok(Transition::Stay),
            KeyCode::Enter if self.field_index == 6 => return Ok(Transition::Pop),

            KeyCode::Char(' ') if self.field_index == 4 => self.format_modern = !self.format_modern,
            KeyCode::Left | KeyCode::Right if self.field_index == 4 => self.format_modern = !self.format_modern,

            KeyCode::Left if self.is_text() => self.tf_mut(self.field_index).move_left(),
            KeyCode::Right if self.is_text() => self.tf_mut(self.field_index).move_right(),
            KeyCode::Home if self.is_text() => self.tf_mut(self.field_index).home(),
            KeyCode::End if self.is_text() => self.tf_mut(self.field_index).end(),
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
