use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders},
};

#[derive(Clone, Default)]
pub struct TextField {
    pub text: String,
    pub cursor: usize,
}

impl TextField {
    pub fn with(text: &str) -> Self {
        Self { text: text.into(), cursor: text.len() }
    }
    pub fn insert_char(&mut self, c: char) { self.text.insert(self.cursor, c); self.cursor += c.len_utf8(); }
    pub fn backspace(&mut self) { if self.cursor > 0 { self.cursor -= 1; self.text.remove(self.cursor); } }
    pub fn delete(&mut self) { if self.cursor < self.text.len() { self.text.remove(self.cursor); } }
    pub fn move_left(&mut self) { if self.cursor > 0 { self.cursor -= 1; } }
    pub fn move_right(&mut self) { if self.cursor < self.text.len() { self.cursor += 1; } }
    pub fn home(&mut self) { self.cursor = 0; }
    pub fn end(&mut self) { self.cursor = self.text.len(); }
}

pub fn draw_frame_title(title: &str) -> Block<'_> {
    Block::default().borders(Borders::ALL).title(title)
}

pub fn submit_line<'a>(focused: bool, label: &'a str) -> Line<'a> {
    let (lbr, rbr) = (
        Span::styled("[ ", Style::default().fg(Color::DarkGray)),
        Span::styled(" ]", Style::default().fg(Color::DarkGray)),
    );
    let inner = if focused {
        Span::styled(label, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    } else {
        Span::raw(label.to_string())
    };
    Line::from(vec![lbr, inner, rbr])
}

// Bash-style block cursor that covers the char (no shifting)
pub fn field_line_text<'a>(label: &str, field: &TextField, focused: bool) -> Line<'a> {
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
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
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

pub fn bool_field_line<'a>(label: &str, val: bool, focused: bool) -> Line<'a> {
    let label = format!("{label}: ");
    let mark = if val { "[x] Yes" } else { "[ ] No " };
    let cursor = if focused { " ▉" } else { "" };
    Line::from(vec![
        Span::styled(label, Style::default().fg(Color::Yellow)),
        Span::raw(mark.to_string()),
        Span::styled(cursor, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ])
}

