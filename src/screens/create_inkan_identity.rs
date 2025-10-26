use anyhow::Result;
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

use crate::app::{AppCtx, ScreenWidget, Transition};
use crate::ui::layout::{three_box_layout, Margins};
use crate::ui::style::{span_key, span_sep, span_text};

#[derive(Default)]
pub struct CreateInkanIdentityScreen {
    menu_index: usize,
}

impl CreateInkanIdentityScreen {
    pub fn new() -> Self {
        Self::default()
    }
}




#[derive(Copy, Clone, Debug)]
enum MenuItem {
    BackToMain,
    Quit,
}
impl MenuItem {
    fn all() -> Vec<MenuItem> { vec![MenuItem::BackToMain, MenuItem::Quit] }
    fn label(&self) -> &'static str {
        match self {
            MenuItem::BackToMain => "Back to Main Menu",
            MenuItem::Quit => "Quit",
        }
    }
}

#[async_trait]
impl ScreenWidget for CreateInkanIdentityScreen {
    fn title(&self) -> &str { "" }

    fn draw(&self, f: &mut Frame<'_>, size: Rect, _ctx: &AppCtx) {
        let header_text = "Create Inkan Identity";
        let explanation_paras = [
            "This is a placeholder page for creating a new Inkan Identity.",
            "Future steps: key material generation, secure export, and optional packaging.",
        ];

        let top_inner_width = size.width.saturating_sub(2*2 + 2 + 2*3) as usize;
        let header_lines = wrap(header_text, top_inner_width).len() as u16;

        let mut exp_lines = 0usize;
        for p in explanation_paras { exp_lines += wrap(p, top_inner_width).len(); }
        let explanation_lines = exp_lines as u16 + (explanation_paras.len().saturating_sub(1) as u16);

        let top_needed = 2 + 2 + header_lines + 1 + explanation_lines;

        let menu_items = MenuItem::all();
        let middle_needed = 2 + 2 + (menu_items.len() as u16);
        let footer_height = 3;

        let regions = three_box_layout(
            size,
            top_needed,
            middle_needed,
            footer_height,
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
            for seg in wrap(p, top_inner_width) {
                expl_lines.push(Line::from(seg.to_string()));
            }
            if i + 1 < explanation_paras.len() { expl_lines.push(Line::from("")); }
        }
        let explanation_para = Paragraph::new(expl_lines)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true });

        f.render_widget(header_para, top_chunks[0]);
        f.render_widget(explanation_para, top_chunks[2]);

        // MIDDLE
        f.render_widget(Block::default().borders(Borders::ALL), regions.middle);

        let list_items: Vec<ListItem> = menu_items.iter().enumerate().map(|(i, it)| {
            let selected = i == self.menu_index;
            let prefix = if selected { "▶ " } else { "  " };
            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                Span::raw(it.label()),
            ]);
            ListItem::new(line)
        }).collect();

        let list = List::new(list_items)
            .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

        f.render_widget(list, regions.middle_inner);

        // FOOTER
        f.render_widget(Block::default().borders(Borders::ALL), regions.bottom);
        let footer_line = Line::from(vec![
            span_key("↑/↓/Tab"), span_text(" Navigate"), span_sep(),
            span_key("Enter"), span_text(" Select"), span_sep(),
            span_key("Ctrl+Q"), span_text(" Quit"),
        ]);
        let footer_para = Paragraph::new(footer_line).wrap(Wrap { trim: true });
        f.render_widget(footer_para, regions.bottom_inner);
    }

    async fn on_key(&mut self, k: KeyEvent, _ctx: &mut AppCtx) -> Result<Transition> {
        if let KeyCode::Char('q') = k.code {
            if k.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(Transition::Push(Box::new(crate::screens::ConfirmQuitScreen::new())));
            }
        }

        match k.code {
            KeyCode::Up => {
                if self.menu_index == 0 { self.menu_index = MenuItem::all().len() - 1; }
                else { self.menu_index -= 1; }
            }
            KeyCode::Down | KeyCode::Tab => {
                self.menu_index = (self.menu_index + 1) % MenuItem::all().len();
            }
            KeyCode::Enter => {
                return Ok(match MenuItem::all()[self.menu_index] {
                    MenuItem::BackToMain => Transition::Pop,
                    MenuItem::Quit => Transition::Push(Box::new(crate::screens::ConfirmQuitScreen::new())),
                })
            }
            _ => {}
        }
        Ok(Transition::Stay)
    }
}
