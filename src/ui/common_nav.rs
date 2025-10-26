use crossterm::event::{KeyCode, KeyEvent};
use crate::app::Transition;

/// Return `Transition::Pop` on Esc so every screen gets "Back" for free.
pub fn esc_to_back(k: KeyEvent) -> Option<Transition> {
    if matches!(k.code, KeyCode::Esc) {
        Some(Transition::Pop)
    } else {
        None
    }
}

