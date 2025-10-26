use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    prelude::Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Terminal,
};
use std::{fs, io};

use crate::abi::load_abi;
use crate::commands;
use crate::process::{process_item, BatchOpts};

// shared defaults
mod defaults;
use defaults::Defaults;

/* ───────────────────────── Router & Screen Trait ───────────────────────── */

enum Transition {
    Stay,
    Push(Box<dyn ScreenWidget>),
    Pop,
    Replace(Box<dyn ScreenWidget>),
    Quit,
}

#[async_trait]
trait ScreenWidget {
    fn title(&self) -> &str {
        "Inkan"
    }
    fn draw(&self, f: &mut Frame<'_>, area: Rect, ctx: &AppCtx);
    async fn on_key(&mut self, key: KeyEvent, ctx: &mut AppCtx) -> Result<Transition>;
}

/* ───────────────────────── Shared Context ───────────────────────── */

#[derive(Default)]
struct AppCtx {
    result_text: String,
}

/* ───────────────────────── Text Field Widget ───────────────────────── */

#[derive(Clone, Default)]
struct TextField {
    text: String,
    cursor: usize,
}
impl TextField {
    fn with(text: &str) -> Self {
        Self {
            text: text.into(),
            cursor: text.len(),
        }
    }
    fn insert_char(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }
    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        self.text.remove(self.cursor);
    }
    fn delete(&mut self) {
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
        }
    }
    fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }
    fn move_right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor += 1;
        }
    }
    fn home(&mut self) {
        self.cursor = 0;
    }
    fn end(&mut self) {
        self.cursor = self.text.len();
    }
}

/* ───────────────────────── Render Helpers ───────────────────────── */

fn draw_frame_title(title: &str) -> Block<'_> {
    Block::default().borders(Borders::ALL).title(title)
}

// Style helpers for Help bar
fn span_key(s: &'static str) -> Span<'static> {
    Span::styled(s, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
}
fn span_sep() -> Span<'static> {
    Span::styled("  |  ", Style::default().fg(Color::DarkGray))
}
fn span_text(s: &'static str) -> Span<'static> {
    Span::raw(s)
}

// BIOS-style help bars (include Ctrl+Q)
fn help_menu<'a>() -> Paragraph<'a> {
    let line = Line::from(vec![
        span_key("↑/↓/Tab"), span_text(" Navigate"), span_sep(),
        span_key("Enter"), span_text(" Select"), span_sep(),
        span_key("Ctrl+Q"), span_text(" Quit"),
    ]);
    Paragraph::new(line).block(Block::default().borders(Borders::ALL))
}
fn help_keygen<'a>() -> Paragraph<'a> {
    let line = Line::from(vec![
        span_key("↑/↓/Tab"), span_text(" Move"), span_sep(),
        span_key("Enter"), span_text(" Submit (on [Submit])"), span_sep(),
        span_key("Space/←/→"), span_text(" Toggle"), span_sep(),
        span_key("←/→/Home/End"), span_text(" Cursor"), span_sep(),
        span_key("Backspace/Delete"), span_text(" Edit"), span_sep(),
        span_key("Esc"), span_text(" Back"), span_sep(),
        span_key("Ctrl+Q"), span_text(" Quit"),
    ]);
    Paragraph::new(line).block(Block::default().borders(Borders::ALL))
}
fn help_batch<'a>() -> Paragraph<'a> {
    let line = Line::from(vec![
        span_key("↑/↓/Tab"), span_text(" Move"), span_sep(),
        span_key("Enter"), span_text(" Submit (on [Submit])"), span_sep(),
        span_key("←/→/Home/End"), span_text(" Cursor"), span_sep(),
        span_key("Backspace/Delete"), span_text(" Edit"), span_sep(),
        span_key("Esc"), span_text(" Back"), span_sep(),
        span_key("Ctrl+Q"), span_text(" Quit"),
    ]);
    Paragraph::new(line).block(Block::default().borders(Borders::ALL))
}

// Bash-style block cursor that covers the character (no shifting).
fn field_line_text<'a>(label: &str, field: &TextField, focused: bool) -> Line<'a> {
    let label_s = format!("{label}: ");
    let text = field.text.as_str();
    let cur = field.cursor.min(text.len());
    let label_span = Span::styled(label_s, Style::default().fg(Color::Yellow));

    if !focused {
        return Line::from(vec![label_span, Span::raw(text.to_string())]);
    }

    let (left, rest) = text.split_at(cur);
    let block = |s: &str| {
        Span::styled(
            s.to_string(),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    };

    if let Some(ch) = rest.chars().next() {
        let after = &rest[ch.len_utf8()..];
        Line::from(vec![
            label_span,
            Span::raw(left.to_string()),
            block(&ch.to_string()),
            Span::raw(after.to_string()),
        ])
    } else {
        Line::from(vec![label_span, Span::raw(left.to_string()), block(" ")])
    }
}

fn bool_field_line<'a>(label: &str, val: bool, focused: bool) -> Line<'a> {
    let label = format!("{label}: ");
    let mark = if val { "[x] Yes" } else { "[ ] No " };
    let cursor = if focused { " ▉" } else { "" };
    Line::from(vec![
        Span::styled(label, Style::default().fg(Color::Yellow)),
        Span::raw(mark.to_string()),
        Span::styled(cursor, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ])
}

fn submit_line<'a>(focused: bool, label: &'a str) -> Line<'a> {
    let (lbr, rbr): (&'a str, &'a str) = ("[ ", " ]");
    let inner = if focused {
        Span::styled(label, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    } else {
        Span::raw(label.to_string())
    };
    Line::from(vec![
        Span::styled(lbr, Style::default().fg(Color::DarkGray)),
        inner,
        Span::styled(rbr, Style::default().fg(Color::DarkGray)),
    ])
}

/// Center an absolute-size rectangle within `r`.
fn centered_rect_abs(width: u16, height: u16, r: Rect) -> Rect {
    let w = width.min(r.width.saturating_sub(2));
    let h = height.min(r.height.saturating_sub(2));
    let x = r.x + (r.width.saturating_sub(w)) / 2;
    let y = r.y + (r.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}

/// Old percentage-based helper (used elsewhere).
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}

/* ───────────────────────── Quit Confirmation Popup ───────────────────────── */

struct ConfirmQuitScreen {
    // 0 = Don't Quit (default), 1 = Quit
    selected: usize,
}
impl ConfirmQuitScreen {
    fn new() -> Self {
        Self { selected: 0 }
    }
}
#[async_trait]
impl ScreenWidget for ConfirmQuitScreen {
    fn title(&self) -> &str {
        // Remove any title text so the top border is a clean line.
        ""
    }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, _ctx: &AppCtx) {
        let msg = "Do you really want to quit the Inkan Management Utility?";
        let left_label = "Don't Quit";
        let right_label = "Quit";

        // Build button spans with selection highlighting.
        let buttons_len =
            ("[ ".len() + left_label.len() + " ]".len()) +
            3 + // gap
            ("[ ".len() + right_label.len() + " ]".len());
        let inner_w_needed = std::cmp::max(msg.len(), buttons_len) as u16;

        // Box sizing
        let inner_width = inner_w_needed.max(36); // minimum pleasant width

        // layout = [blank, message, spacer, buttons]
        let inner_height = 4;

        let total_w = inner_width + 4; // 2 cols margin + borders
        let total_h = inner_height + 3; // balanced vertical padding (+ borders)

        let area = centered_rect_abs(total_w, total_h, size);

        // No visible title text (top border stays continuous)
        let block = Block::default()
            .borders(Borders::ALL)
            .title(self.title());

        // Inner content area (1 line vertical padding top/bottom)
        let inner = area.inner(&Margin { horizontal: 2, vertical: 1 });

        // Vertical layout: blank | message | spacer | buttons
        let vchunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // blank line ABOVE the message
                Constraint::Length(1), // message
                Constraint::Length(1), // spacer
                Constraint::Length(1), // buttons
            ])
            .split(inner);

        // Message line (centered)
        let msg_line = Paragraph::new(Line::from(vec![Span::raw(msg)]))
            .alignment(Alignment::Center);

        // Buttons line (centered)
        let left_spans: Vec<Span> = if self.selected == 0 {
            vec![
                Span::styled("[ ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    left_label,
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ]", Style::default().fg(Color::DarkGray)),
            ]
        } else {
            vec![
                Span::styled("[ ", Style::default().fg(Color::DarkGray)),
                Span::raw(left_label.to_string()),
                Span::styled(" ]", Style::default().fg(Color::DarkGray)),
            ]
        };

        let right_spans: Vec<Span> = if self.selected == 1 {
            vec![
                Span::styled("[ ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    right_label,
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ]", Style::default().fg(Color::DarkGray)),
            ]
        } else {
            vec![
                Span::styled("[ ", Style::default().fg(Color::DarkGray)),
                Span::raw(right_label.to_string()),
                Span::styled(" ]", Style::default().fg(Color::DarkGray)),
            ]
        };

        let mut btn_spans = Vec::new();
        btn_spans.extend(left_spans);
        btn_spans.push(Span::raw("   "));
        btn_spans.extend(right_spans);

        let buttons_line = Paragraph::new(Line::from(btn_spans))
            .alignment(Alignment::Center);

        // Render
        f.render_widget(Clear, area);
        f.render_widget(block, area);
        // vchunks[0] is the intentional blank line
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
                if self.selected == 1 {
                    return Ok(Transition::Quit);
                } else {
                    return Ok(Transition::Pop);
                }
            }
            _ => {}
        }
        Ok(Transition::Stay)
    }
}

/* ───────────────────────── Main Menu Screen ───────────────────────── */

#[derive(Default)]
struct MainMenuScreen {
    menu_index: usize,
}
#[derive(Copy, Clone, Debug)]
enum MenuItem {
    Keygen,
    BatchSign,
    Quit,
}
impl MenuItem {
    fn all() -> Vec<MenuItem> {
        vec![MenuItem::Keygen, MenuItem::BatchSign, MenuItem::Quit]
    }
    fn label(&self) -> &'static str {
        match self {
            MenuItem::Keygen => "Generate Keys",
            MenuItem::BatchSign => "Batch Sign Transactions",
            MenuItem::Quit => "Quit",
        }
    }
}

#[async_trait]
impl ScreenWidget for MainMenuScreen {
    fn title(&self) -> &str {
        ""
    }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, _ctx: &AppCtx) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Min(5),
                    Constraint::Length(3),
                ]
                .as_ref(),
            )
            .split(size);

let header = Paragraph::new("Inkan Management Utility — Main Menu")
    .alignment(Alignment::Center)
    .block(draw_frame_title(self.title()));

        let items = MenuItem::all();
        let list_items: Vec<ListItem> = items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                let selected = i == self.menu_index;
                let prefix = if selected { "▶ " } else { "  " };
                let line = Line::from(vec![
                    Span::styled(prefix, Style::default().fg(Color::Cyan)),
                    Span::raw(it.label()),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(list_items)
            .block(Block::default().borders(Borders::ALL))
            .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

        let footer = help_menu();

        f.render_widget(header, chunks[0]);
        f.render_widget(list, chunks[1]);
        f.render_widget(footer, chunks[2]);
    }

    async fn on_key(&mut self, k: KeyEvent, _ctx: &mut AppCtx) -> Result<Transition> {
        match k.code {
            // Optional: keep bare 'q'; Ctrl+Q is the global quit
            KeyCode::Char('q') => return Ok(Transition::Quit),
            KeyCode::Up => {
                if self.menu_index == 0 {
                    self.menu_index = MenuItem::all().len() - 1;
                } else {
                    self.menu_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Tab => {
    self.menu_index = (self.menu_index + 1) % MenuItem::all().len();
            }
            KeyCode::Enter => {
                return Ok(match MenuItem::all()[self.menu_index] {
                    MenuItem::Keygen => Transition::Push(Box::new(KeygenScreen::new())),
                    MenuItem::BatchSign => Transition::Push(Box::new(BatchScreen::new())),
                    MenuItem::Quit => Transition::Quit, // direct quit, no confirmation
                })
            }
            _ => {}
        }
        Ok(Transition::Stay)
    }
}

/* ───────────────────────── Keygen Screen ───────────────────────── */

struct KeygenScreen {
    count: TextField,
    save_to_file: bool,
    out_path: TextField,
    field_index: usize,
}
impl KeygenScreen {
    fn new() -> Self {
        Self {
            count: TextField::with(Defaults::KEYGEN_COUNT),
            save_to_file: Defaults::KEYGEN_SAVE_TO_FILE,
            out_path: TextField::with(Defaults::KEYGEN_OUT_PATH),
            field_index: 0,
        }
    }
    fn submit_index(&self) -> usize {
        if self.save_to_file { 3 } else { 2 }
    }
    fn is_text_field(&self, idx: usize) -> bool {
        idx == 0 || (self.save_to_file && idx == 2)
    }
}
impl Default for KeygenScreen {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScreenWidget for KeygenScreen {
    fn title(&self) -> &str {
        "Keygen"
    }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, _ctx: &AppCtx) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Min(8),
                    Constraint::Length(3),
                ]
                .as_ref(),
            )
            .split(size);

        let header = Paragraph::new("Generate Ethereum/Nostr keypairs (offline)")
            .block(draw_frame_title(self.title()));

        let submit_idx = self.submit_index();

        let mut lines: Vec<Line> = vec![
            field_line_text("Count", &self.count, self.field_index == 0),
            bool_field_line("Save to file?", self.save_to_file, self.field_index == 1),
        ];
        if self.save_to_file {
            lines.push(field_line_text(
                "Output path",
                &self.out_path,
                self.field_index == 2,
            ));
        }
        lines.push(Line::from(""));
        lines.push(submit_line(self.field_index == submit_idx, "Submit"));

        let help = help_keygen();

        let form = Paragraph::new(lines)
            .block(draw_frame_title("Inputs"))
            .style(Style::default());

        f.render_widget(header, chunks[0]);
        f.render_widget(form, chunks[1]);
        f.render_widget(help, chunks[2]);
    }

    async fn on_key(&mut self, k: KeyEvent, ctx: &mut AppCtx) -> Result<Transition> {
        let submit_idx = self.submit_index();

        match k.code {
            KeyCode::Esc => return Ok(Transition::Pop),

            // Navigation (Up/Down/Tab only; no Shift+Tab)
            KeyCode::Up => {
                if self.field_index == 0 {
                    self.field_index = submit_idx;
                } else {
                    self.field_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Tab => {
                self.field_index = (self.field_index + 1) % (submit_idx + 1);
            }

            // Enter ONLY submits when on [Submit]
            KeyCode::Enter if self.field_index == submit_idx => {
                let count: u32 = self
                    .count
                    .text
                    .trim()
                    .parse()
                    .map_err(|_| anyhow!("Count must be a positive integer"))?;
                let records = commands::keygen::generate(count)?;
                if self.save_to_file {
                    let p = self.out_path.text.trim();
                    commands::keygen::emit(records, Some(p.into()))
                        .with_context(|| format!("writing {}", p))?;
                    ctx.result_text = format!("✓ Wrote {}", p);
                } else {
                    let json = serde_json::to_string_pretty(&records)?;
                    ctx.result_text = json;
                }
                return Ok(Transition::Push(Box::new(ResultScreen::default())));
            }

            // Checkbox toggle: Space or Left/Right only
            KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right if self.field_index == 1 => {
                self.save_to_file = !self.save_to_file;
            }

            // Text cursor movement
            KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End
                if self.is_text_field(self.field_index) =>
            {
                match self.field_index {
                    0 => match k.code {
                        KeyCode::Left => self.count.move_left(),
                        KeyCode::Right => self.count.move_right(),
                        KeyCode::Home => self.count.home(),
                        KeyCode::End => self.count.end(),
                        _ => {}
                    },
                    2 => match k.code {
                        KeyCode::Left => self.out_path.move_left(),
                        KeyCode::Right => self.out_path.move_right(),
                        KeyCode::Home => self.out_path.home(),
                        KeyCode::End => self.out_path.end(),
                        _ => {}
                    },
                    _ => {}
                }
            }

            // Editing
            KeyCode::Backspace if self.is_text_field(self.field_index) => match self.field_index {
                0 => self.count.backspace(),
                2 => self.out_path.backspace(),
                _ => {}
            },
            KeyCode::Delete if self.is_text_field(self.field_index) => match self.field_index {
                0 => self.count.delete(),
                2 => self.out_path.delete(),
                _ => {}
            },
            KeyCode::Char(c)
                if !k.modifiers.contains(KeyModifiers::CONTROL) && self.is_text_field(self.field_index) =>
            {
                match self.field_index {
                    0 => self.count.insert_char(c),
                    2 => self.out_path.insert_char(c),
                    _ => {}
                }
            }

            _ => {}
        }
        Ok(Transition::Stay)
    }
}

/* ───────────────────────── Batch Screen ───────────────────────── */

struct BatchScreen {
    batch_path: TextField,
    out_path: TextField,
    gas_limit: TextField,
    max_fee_per_gas: TextField,
    max_priority_fee_per_gas: TextField,
    field_index: usize,
}
impl BatchScreen {
    fn new() -> Self {
        Self {
            batch_path: TextField::with(Defaults::BATCH_INPUT_PATH),
            out_path: TextField::with(Defaults::BATCH_OUTPUT_PATH),
            gas_limit: TextField::with(Defaults::BATCH_GAS_LIMIT),
            max_fee_per_gas: TextField::with(Defaults::BATCH_MAX_FEE_PER_GAS),
            max_priority_fee_per_gas: TextField::with(Defaults::BATCH_MAX_PRIORITY_FEE_PER_GAS),
            field_index: 0,
        }
    }
    fn is_text(&self) -> bool {
        self.field_index <= 4
    }
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
impl Default for BatchScreen {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScreenWidget for BatchScreen {
    fn title(&self) -> &str {
        "Batch"
    }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, _ctx: &AppCtx) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Min(11),
                    Constraint::Length(3),
                ]
                .as_ref(),
            )
            .split(size);

        let header =
            Paragraph::new("Sign a JSON array of EIP-1559 calls (offline)").block(draw_frame_title(self.title()));

        let mut lines: Vec<Line> = vec![
            field_line_text("Batch path", &self.batch_path, self.field_index == 0),
            field_line_text("Output path", &self.out_path, self.field_index == 1),
            field_line_text("Gas limit", &self.gas_limit, self.field_index == 2),
            field_line_text(
                "Max fee per gas (wei)",
                &self.max_fee_per_gas,
                self.field_index == 3,
            ),
            field_line_text(
                "Max priority fee per gas (wei)",
                &self.max_priority_fee_per_gas,
                self.field_index == 4,
            ),
        ];

        lines.push(Line::from(""));
        lines.push(submit_line(self.field_index == 5, "Submit"));

        let help = help_batch();

        let form = Paragraph::new(lines)
            .block(draw_frame_title("Inputs"))
            .style(Style::default());

        f.render_widget(header, chunks[0]);
        f.render_widget(form, chunks[1]);
        f.render_widget(help, chunks[2]);
    }

    async fn on_key(&mut self, k: KeyEvent, ctx: &mut AppCtx) -> Result<Transition> {
        match k.code {
            KeyCode::Esc => return Ok(Transition::Pop),

            // Navigation (Up/Down/Tab only; no Shift+Tab)
            KeyCode::Up => {
                if self.field_index == 0 {
                    self.field_index = 5;
                } else {
                    self.field_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Tab => {
                self.field_index = (self.field_index + 1) % 6;
            }

            // Enter ONLY submits when on [Submit]
            KeyCode::Enter if self.field_index == 5 => {
                let batch_path = self.batch_path.text.trim().to_string();
                let out_path = self.out_path.text.trim().to_string();

                let abi = load_abi()?;
                let text =
                    fs::read_to_string(&batch_path).with_context(|| format!("reading {}", batch_path))?;
                let items: Vec<crate::types::Item> =
                    serde_json::from_str(&text).context("parsing batch JSON (array)")?;

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

            // Cursor movement within text fields
            KeyCode::Left if self.is_text() => self.tf_mut(self.field_index).move_left(),
            KeyCode::Right if self.is_text() => self.tf_mut(self.field_index).move_right(),
            KeyCode::Home if self.is_text() => self.tf_mut(self.field_index).home(),
            KeyCode::End if self.is_text() => self.tf_mut(self.field_index).end(),

            // Editing
            KeyCode::Backspace if self.is_text() => self.tf_mut(self.field_index).backspace(),
            KeyCode::Delete if self.is_text() => self.tf_mut(self.field_index).delete(),
            KeyCode::Char(c)
                if self.is_text() && !k.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.tf_mut(self.field_index).insert_char(c)
            }

            _ => {}
        }
        Ok(Transition::Stay)
    }
}

/* ───────────────────────── Result Screen ───────────────────────── */

#[derive(Default)]
struct ResultScreen;

#[async_trait]
impl ScreenWidget for ResultScreen {
    fn title(&self) -> &str {
        "Result"
    }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, ctx: &AppCtx) {
        let area = centered_rect(80, 70, size);
        let block = Block::default().borders(Borders::ALL).title(self.title());
        let text = Paragraph::new(ctx.result_text.as_str()).block(block);
        f.render_widget(Clear, area);
        f.render_widget(text, area);
    }

    async fn on_key(&mut self, k: KeyEvent, ctx: &mut AppCtx) -> Result<Transition> {
        match k.code {
            KeyCode::Esc | KeyCode::Enter => {
                ctx.result_text.clear();
                Ok(Transition::Pop)
            }
            _ => Ok(Transition::Stay),
        }
    }
}

/* ───────────────────────── Entry Point ───────────────────────── */

pub async fn run_menu() -> Result<()> {
    // terminal init
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?; // clean start

    let mut ctx = AppCtx::default();
    let mut stack: Vec<Box<dyn ScreenWidget>> = vec![Box::new(MainMenuScreen::default())];

    loop {
        terminal.draw(|f| {
            let size = f.size();
            if let Some(top) = stack.last() {
                top.draw(f, size, &ctx);
            }
        })?;

        if event::poll(std::time::Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => {
                    // GLOBAL HOTKEY: Ctrl+Q shows confirm quit from anywhere
                    if k.modifiers.contains(KeyModifiers::CONTROL) && matches!(k.code, KeyCode::Char('q' | 'Q')) {
                        stack.push(Box::new(ConfirmQuitScreen::new()));
                        continue;
                    }

                    if let Some(top) = stack.last_mut() {
                        match top.on_key(k, &mut ctx).await? {
                            Transition::Stay => {}
                            Transition::Push(s) => stack.push(s),
                            Transition::Pop => {
                                stack.pop();
                                if stack.is_empty() {
                                    break;
                                }
                            }
                            Transition::Replace(s) => {
                                stack.pop();
                                stack.push(s);
                            }
                            Transition::Quit => break,
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // restore
    disable_raw_mode()?;
    let out = terminal.backend_mut();
    execute!(out, LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}
