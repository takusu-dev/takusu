use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, Modal};

pub async fn handle_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.habit_list.next(),
        KeyCode::Char('k') | KeyCode::Up => app.habit_list.prev(),
        KeyCode::Char('d') if app.selected_habit().is_some() => {
            app.modal = Modal::ConfirmDelete;
        }
        _ => {}
    }
}
