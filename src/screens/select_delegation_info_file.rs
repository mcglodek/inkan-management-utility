// src/screens/select_delegation_info_file.rs

use anyhow::{Context, Result};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use textwrap::wrap;

use std::fs;
use std::path::{PathBuf, Path};

use crate::app::{AppCtx, ScreenWidget, Transition, DelegationPrefill};
use crate::ui::layout::{three_box_layout, Margins};
use crate::ui::style::{span_key, span_sep, span_text, button_spans};
use crate::ui::common_nav::esc_to_back;
use crate::util::parse_delegation_env;

pub struct SelectDelegationInfoFileScreen {
    dir: PathBuf,
    entries: Vec<PathBuf>,
    field_index: usize, // 0 = list, 1 = Refresh, 2 = Back
    list_index: usize,
}

impl SelectDelegationInfoFileScreen {
    pub fn new(dir: PathBuf) -> Self {
        let entries = read_files_only(&dir).unwrap_or_default();
        let field_index = if entries.is_empty() { 1 } else { 0 };
        Self { dir, entries, field_index, list_index: 0 }
    }

    fn refresh_list(&mut self) -> Result<()> {
        self.entries = read_files_only(&self.dir).unwrap_or_default();
        if self.entries.is_empty() { self.field_index = 1; self.list_index = 0; }
        else { self.field_index = 0; self.list_index = 0; }
        Ok(())
    }

    fn buttons_line(refresh_selected: bool, back_selected: bool) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.extend(button_spans("Refresh List", refresh_selected));
        spans.push(Span::raw("   "));
        spans.extend(button_spans("Back", back_selected));
        Line::from(spans)
    }
}

fn read_files_only(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for ent in fs::read_dir(dir).with_context(|| format!("listing {}", dir.display()))? {
        let ent = ent?;
        let p = ent.path();
        if p.is_file() { out.push(p); }
    }
    out.sort();
    Ok(out)
}

#[async_trait]
impl ScreenWidget for SelectDelegationInfoFileScreen {
    fn title(&self) -> &str { "" }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, _ctx: &AppCtx) {
        let header_text = "Select Delegation Info File";
        let explanation_paras = [
            &format!("Directory: {}", self.dir.display()),
            "Use ↑/↓ (or Tab) to move focus. Enter to select.",
        ];

        // --- TOP sizing ---
        let top_inner_width = size.width.saturating_sub(2*2 + 2 + 2*3) as usize;
        let header_lines = wrap(header_text, top_inner_width).len() as u16;

        let mut exp_lines = 0usize;
        for p in explanation_paras.iter() { exp_lines += wrap(p, top_inner_width).len(); }
        let explanation_lines = exp_lines as u16 + (explanation_paras.len().saturating_sub(1) as u16);
        let top_needed = 2 + 2 + header_lines + 1 + explanation_lines;

        // Middle: list + spacer + buttons
        let middle_rows: u16 = (self.entries.len() as u16).saturating_add(3);
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

        let mut items: Vec<ListItem> = Vec::new();
        items.push(ListItem::new(Line::from(""))); // spacer on top

        if self.entries.is_empty() {
            items.push(ListItem::new(Line::from("No files found in this directory.")));
        } else {
            for (i, p) in self.entries.iter().enumerate() {
                let selected = self.field_index == 0 && self.list_index == i;
                let prefix = if selected { "▶ " } else { "  " };
                let line = Line::from(vec![
                    Span::styled(prefix, Style::default().fg(Color::Cyan)),
                    Span::raw(p.file_name().unwrap_or_default().to_string_lossy().to_string()),
                ]);
                items.push(ListItem::new(line));
            }
        }

        // Buttons row
        items.push(ListItem::new(Line::from("")));
        items.push(ListItem::new(Self::buttons_line(self.field_index == 1, self.field_index == 2)));

        let list = List::new(items)
            .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        f.render_widget(list, regions.middle_inner);

        // FOOTER legend
        f.render_widget(Block::default().borders(Borders::ALL), regions.bottom);
        let footer_line = Line::from(vec![
            span_key("↑/↓/Tab"), span_text(" Navigate"), span_sep(),
            span_key("Enter"), span_text(" Select"),   span_sep(),
            span_key("Esc"),   span_text(" Back"),     span_sep(),
            span_key("Ctrl+Q"),span_text(" Quit"),
        ]);
        f.render_widget(Paragraph::new(footer_line).wrap(Wrap { trim: true }), regions.bottom_inner);
    }

    async fn on_key(&mut self, k: KeyEvent, ctx: &mut AppCtx) -> Result<Transition> {
        if let Some(t) = esc_to_back(k) { return Ok(t); }

        if let KeyCode::Char('q') = k.code {
            if k.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(Transition::Push(Box::new(crate::screens::ConfirmQuitScreen::new())));
            }
        }

        // Treat Tab exactly like Down
        let key = match k.code { KeyCode::Tab => KeyCode::Down, other => other };

        let has_files = !self.entries.is_empty();

        match key {
            // DOWN cycles: List -> Refresh -> Back -> (top of) List
            KeyCode::Down => {
                if has_files {
                    match self.field_index {
                        0 => {
                            if self.list_index + 1 < self.entries.len() { self.list_index += 1; }
                            else { self.field_index = 1; }
                        }
                        1 => { self.field_index = 2; }
                        2 => { self.field_index = 0; self.list_index = 0; }
                        _ => {}
                    }
                } else {
                    self.field_index = if self.field_index == 1 { 2 } else { 1 };
                }
            }

            // UP cycles reverse
            KeyCode::Up => {
                if has_files {
                    match self.field_index {
                        0 => {
                            if self.list_index > 0 { self.list_index -= 1; }
                            else { self.field_index = 2; }
                        }
                        1 => { self.field_index = 0; self.list_index = self.entries.len().saturating_sub(1); }
                        2 => { self.field_index = 1; }
                        _ => {}
                    }
                } else {
                    self.field_index = if self.field_index == 2 { 1 } else { 2 };
                }
            }

            // Enter on list selection -> read, parse, stash -> PopN(2) back to form
            KeyCode::Enter if self.field_index == 0 => {
                if let Some(sel) = self.entries.get(self.list_index).cloned() {
                    let contents = fs::read_to_string(&sel)
                        .with_context(|| format!("reading {}", sel.display()))?;
                    let map = parse_delegation_env(&contents);

                    // Stash for the Delegation form to apply
                    ctx.pending_delegation_prefill = Some(DelegationPrefill { map });

                    // Jump straight back: Select File -> Choose Dir -> Delegation Form
                    return Ok(Transition::PopN(2));
                }
            }

            // Enter on Refresh
            KeyCode::Enter if self.field_index == 1 => { self.refresh_list()?; }

            // Enter on Back
            KeyCode::Enter if self.field_index == 2 => { return Ok(Transition::Pop); }

            _ => {}
        }

        Ok(Transition::Stay)
    }
}
