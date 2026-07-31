use ratatui::{
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType},
};

// Royal Blue palette + turquoise secondary
pub const BORDER: Color = Color::Rgb(65, 105, 225); // Royal Blue
pub const TITLE: Color = Color::Rgb(100, 149, 237); // Cornflower Blue
pub const ACCENT: Color = Color::Rgb(30, 144, 255); // Dodger Blue
/// Second accent for hashes / metadata (pairs with ACCENT on blue terminals).
pub const SECONDARY: Color = Color::Rgb(64, 224, 208); // Turquoise
pub const MUTED: Color = Color::Rgb(119, 136, 153); // Light Slate Gray
pub const SUCCESS: Color = Color::Rgb(60, 179, 113); // Medium Sea Green
pub const ERROR: Color = Color::Rgb(220, 20, 60); // Crimson
pub const WARNING: Color = Color::Rgb(255, 165, 0); // Orange
pub const HIGHLIGHT_FG: Color = Color::Rgb(255, 255, 255); // White
/// Light sky blue for account flag chips (brand accent sibling).
pub const FLAG: Color = Color::Rgb(100, 200, 255);
pub const HIGHLIGHT_BG: Color = BORDER;
/// Foreground for values drawn on ACCENT fills (BarChart labels, etc.).
/// Dark slate — readable on ACCENT without hardcoded Color::Black.
pub const CHART_VALUE_FG: Color = Color::Rgb(15, 23, 42); // Slate-900

pub fn panel_block(title: &str, is_focused: bool) -> Block<'static> {
    panel_block_owned(format!(" {title} "), is_focused)
}

/// Like [`panel_block`], but `title` is used as-is (caller supplies spacing/count text).
pub fn panel_block_owned(title: String, is_focused: bool) -> Block<'static> {
    let border_color = if is_focused { ACCENT } else { MUTED };
    let title_color = if is_focused { TITLE } else { MUTED };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border_color))
        .title_style(Style::new().fg(title_color).add_modifier(Modifier::BOLD))
        .title(title)
}

pub fn header_row_style() -> Style {
    Style::new()
        .fg(ACCENT)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}

pub fn selected_row_style(is_focused: bool) -> Style {
    if is_focused {
        Style::new()
            .fg(HIGHLIGHT_FG)
            .bg(HIGHLIGHT_BG)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(MUTED).add_modifier(Modifier::REVERSED)
    }
}

pub fn dim_style() -> Style {
    Style::new().fg(MUTED)
}

pub fn accent_style() -> Style {
    Style::new().fg(ACCENT)
}

pub fn secondary_style() -> Style {
    Style::new().fg(SECONDARY)
}

pub fn success_style() -> Style {
    Style::new().fg(SUCCESS)
}

pub fn error_style() -> Style {
    Style::new().fg(ERROR)
}

pub fn warning_style() -> Style {
    Style::new().fg(WARNING)
}

pub fn flag_style() -> Style {
    Style::new().fg(FLAG).add_modifier(Modifier::BOLD)
}

pub fn chart_value_style() -> Style {
    Style::new().fg(CHART_VALUE_FG).bg(ACCENT)
}
