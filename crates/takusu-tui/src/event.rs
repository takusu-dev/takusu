use std::time::Duration;

use crossterm::event::{self, Event as CtEvent, KeyEventKind};
use tokio::sync::mpsc;

use crate::app::Msg;

pub async fn spawn(tx: mpsc::UnboundedSender<Msg>) {
    loop {
        if event::poll(Duration::from_millis(250)).unwrap_or(false) {
            match event::read() {
                Ok(CtEvent::Key(key)) if key.kind == KeyEventKind::Press => {
                    if tx.send(Msg::Key(key)).is_err() {
                        break;
                    }
                }
                Ok(CtEvent::Resize(_, _)) => {
                    let _ = tx.send(Msg::Tick);
                }
                _ => {}
            }
        } else if tx.send(Msg::Tick).is_err() {
            break;
        }
    }
}
