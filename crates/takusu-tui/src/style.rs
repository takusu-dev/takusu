use ratatui::style::{Color, Modifier, Style};

pub const HEADER_FG: Color = Color::Cyan;
pub const TAB_ACTIVE: Style = Style::new()
    .fg(Color::Black)
    .bg(Color::Cyan)
    .add_modifier(Modifier::BOLD);
pub const TAB_INACTIVE: Style = Style::new().fg(Color::DarkGray);

pub fn selected_style() -> Style {
    Style::new()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
}

pub fn status_color(status: &str) -> Color {
    match status {
        "pending" => Color::Yellow,
        "scheduled" => Color::Green,
        "in_progress" => Color::Rgb(180, 140, 0),
        "completed" => Color::DarkGray,
        "skipped" => Color::Rgb(100, 100, 100),
        _ => Color::White,
    }
}
