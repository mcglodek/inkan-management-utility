use ratatui::{
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::style::{span_key, span_sep, span_text};

pub fn help_menu<'a>() -> Paragraph<'a> {
    let line = Line::from(vec![
        span_key("↑/↓/Tab"), span_text(" Navigate"), span_sep(),
        span_key("Enter"), span_text(" Select"), span_sep(),
        span_key("Ctrl+Q"), span_text(" Quit"),
    ]);
    Paragraph::new(line).block(Block::default().borders(Borders::ALL)).wrap(Wrap { trim: true })
}

pub fn help_keygen<'a>() -> Paragraph<'a> {
    let line = Line::from(vec![
        span_key("↑/↓/Tab"), span_text(" Move"), span_sep(),
        span_key("Enter"), span_text(" Submit (on [Submit])"), span_sep(),
        span_key("Space/←/→"), span_text(" Toggle"), span_sep(),
        span_key("←/→/Home/End"), span_text(" Cursor"), span_sep(),
        span_key("Backspace/Delete"), span_text(" Edit"), span_sep(),
        span_key("Esc"), span_text(" Back"), span_sep(),
        span_key("Ctrl+Q"), span_text(" Quit"),
    ]);
    Paragraph::new(line).block(Block::default().borders(Borders::ALL)).wrap(Wrap { trim: true })
}

pub fn help_batch<'a>() -> Paragraph<'a> {
    let line = Line::from(vec![
        span_key("↑/↓/Tab"), span_text(" Move"), span_sep(),
        span_key("Enter"), span_text(" Submit (on [Submit])"), span_sep(),
        span_key("←/→/Home/End"), span_text(" Cursor"), span_sep(),
        span_key("Backspace/Delete"), span_text(" Edit"), span_sep(),
        span_key("Esc"), span_text(" Back"), span_sep(),
        span_key("Ctrl+Q"), span_text(" Quit"),
    ]);
    Paragraph::new(line).block(Block::default().borders(Borders::ALL)).wrap(Wrap { trim: true })
}

