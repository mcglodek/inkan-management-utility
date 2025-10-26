use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};

pub struct ThreeBox {
    pub top: Rect,
    pub middle: Rect,
    pub bottom: Rect,
    pub top_inner: Rect,
    pub middle_inner: Rect,
    pub bottom_inner: Rect,
}

pub struct Margins {
    pub page: u16,         // outer page margin (e.g., 2)
    pub inner_top: u16,    // inner margin for top box (e.g., 3)
    pub inner_middle: u16, // inner margin for middle box (e.g., 3)
    pub inner_bottom: u16, // inner margin for bottom box (e.g., 3)
}

pub fn three_box_layout(
    size: Rect,
    top_needed: u16,
    middle_needed: u16,
    footer_height: u16,
    margins: Margins,
) -> ThreeBox {
    let available_for_top_and_middle =
        size.height.saturating_sub(2 * margins.page).saturating_sub(footer_height);

    let top_min = 5;
    let top_cap = available_for_top_and_middle.saturating_sub(middle_needed);
    let top_height = top_needed.min(top_cap.max(top_min));
    let middle_height = available_for_top_and_middle.saturating_sub(top_height);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(margins.page)
        .constraints([
            Constraint::Length(top_height),
            Constraint::Length(middle_height),
            Constraint::Length(footer_height),
        ])
        .split(size);

    let top_inner = chunks[0].inner(&Margin { horizontal: margins.inner_top, vertical: 1 });
    let middle_inner = chunks[1].inner(&Margin { horizontal: margins.inner_middle, vertical: 1 });
    let bottom_inner = chunks[2].inner(&Margin { horizontal: margins.inner_bottom, vertical: 1 });

    ThreeBox {
        top: chunks[0],
        middle: chunks[1],
        bottom: chunks[2],
        top_inner,
        middle_inner,
        bottom_inner,
    }
}

// Also expose the centering helpers used by other screens.
pub fn centered_rect_abs(width: u16, height: u16, r: Rect) -> Rect {
    let w = width.min(r.width.saturating_sub(2));
    let h = height.min(r.height.saturating_sub(2));
    let x = r.x + (r.width.saturating_sub(w)) / 2;
    let y = r.y + (r.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}

pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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

