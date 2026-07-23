use crossterm::event::{KeyCode, KeyEvent};

use crate::app::App;

pub async fn handle_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.schedule_list.next(),
        KeyCode::Char('k') | KeyCode::Up => app.schedule_list.prev(),
        KeyCode::Char('g') => app.do_generate().await,
        KeyCode::Char('r') => app.do_reschedule().await,
        _ => {}
    }
}
