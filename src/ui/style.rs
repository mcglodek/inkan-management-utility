// style.rs
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::borrow::Cow;

pub fn span_key(s: &'static str) -> Span<'static> {
    Span::styled(s, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
}
pub fn span_sep() -> Span<'static> {
    Span::styled("  |  ", Style::default().fg(Color::DarkGray))
}
pub fn span_text(s: &'static str) -> Span<'static> {
    Span::raw(s)
}

/* ---------- New: button helpers (Blue brackets, Red for selected, Yellow for idle) ---------- */

const ACCENT_BRACKET: Color = Color::Blue;    // your chosen accent color
const SELECTED_TEXT: Color = Color::Red;      // “selected” color
const IDLE_TEXT: Color = Color::Blue;       // non-selected, bright but distinct

/// Core painter: "< " + LABEL + " >"
pub fn button_spans<S: Into<Cow<'static, str>>>(label: S, selected: bool) -> Vec<Span<'static>> {
    let label = label.into();
    vec![
        Span::styled("< ", Style::default().fg(ACCENT_BRACKET).add_modifier(Modifier::BOLD)),
        Span::styled(
            label,
            Style::default()
                .fg(if selected { SELECTED_TEXT } else { IDLE_TEXT })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" >", Style::default().fg(ACCENT_BRACKET).add_modifier(Modifier::BOLD)),
    ]
}

/// Same look, but visually “disabled”
pub fn button_spans_disabled<S: Into<Cow<'static, str>>>(label: S) -> Vec<Span<'static>> {
    let label = label.into();
    vec![
        Span::styled("< ", Style::default().fg(Color::DarkGray)),
        Span::styled(label, Style::default().fg(Color::Gray)),
        Span::styled(" >", Style::default().fg(Color::DarkGray)),
    ]
}

/// Convenience: a single Line you can pass to Paragraph/List/etc.
pub fn button_line<S: Into<Cow<'static, str>>>(label: S, selected: bool) -> Line<'static> {
    Line::from(button_spans(label, selected))
}
