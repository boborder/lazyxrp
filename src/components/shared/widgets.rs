use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use crate::components::shared::theme;

pub fn titled_block(title: &str, is_focused: bool) -> Block<'_> {
    theme::panel_block(title, is_focused)
}

pub fn titled_block_with_count(
    title: &str,
    selected: Option<usize>,
    total: usize,
    is_focused: bool,
) -> Block<'_> {
    if total == 0 {
        return theme::panel_block(title, is_focused);
    }
    let count = match selected {
        Some(i) => format!(" {title} ({}/{total}) ", i + 1),
        None => format!(" {title} ({total}) "),
    };
    let border_color = if is_focused {
        theme::ACCENT
    } else {
        theme::MUTED
    };
    let title_color = if is_focused {
        theme::TITLE
    } else {
        theme::MUTED
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border_color))
        .title_style(Style::new().fg(title_color).add_modifier(Modifier::BOLD))
        .title(count)
}

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn spinner(tick: usize) -> &'static str {
    SPINNER_FRAMES[tick % SPINNER_FRAMES.len()]
}

pub fn render_loading(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    tick: usize,
    msg: &str,
    is_focused: bool,
) {
    let line = Line::from(vec![
        Span::styled(spinner(tick), theme::accent_style()),
        Span::raw(" "),
        Span::styled(msg, theme::dim_style()),
    ]);
    frame.render_widget(
        Paragraph::new(line).block(titled_block(title, is_focused)),
        area,
    );
}

pub fn render_empty(frame: &mut Frame, area: Rect, title: &str, msg: &str, is_focused: bool) {
    frame.render_widget(
        Paragraph::new(Span::styled(msg, theme::dim_style()))
            .block(titled_block(title, is_focused)),
        area,
    );
}

pub fn render_error(frame: &mut Frame, area: Rect, title: &str, msg: &str, is_focused: bool) {
    let line = Line::from(vec![
        Span::styled("error: ", theme::error_style()),
        Span::raw(msg),
    ]);
    frame.render_widget(
        Paragraph::new(line).block(titled_block(title, is_focused)),
        area,
    );
}
