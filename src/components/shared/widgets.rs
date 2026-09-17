use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::{Line, Span},
    widgets::{Block, Cell, Paragraph, Row, Table},
};

use crate::{
    components::shared::{fmt, theme},
    xrpl::TxRow,
};

/// One table row for [`TxRow`] with per-column colors (hash / dir / type / ledger / result).
fn tx_table_row(t: &TxRow) -> Row<'static> {
    let result_style = if t.result == "tesSUCCESS" {
        theme::success_style()
    } else {
        theme::error_style()
    };
    let dir_style = match t.direction.as_str() {
        "▼" => theme::error_style(),
        "▲" => theme::success_style(),
        _ => theme::dim_style(),
    };
    let hash_cell = if t.hash.len() > 16 {
        Cell::from(format!("{}…", &t.hash[..16])).style(theme::secondary_style())
    } else {
        Cell::from(t.hash.clone()).style(theme::secondary_style())
    };
    Row::new(vec![
        hash_cell,
        Cell::from(t.direction.clone()).style(dir_style),
        Cell::from(t.tx_type.clone()).style(theme::accent_style()),
        Cell::from(fmt::group_digits_u64(u64::from(t.ledger_index))).style(theme::dim_style()),
        Cell::from(t.result.clone()).style(result_style),
    ])
}

/// Build owned transaction rows once per history/filter change.
pub fn tx_table<'a>(txs: impl IntoIterator<Item = &'a TxRow>) -> Table<'static> {
    let header =
        Row::new(vec!["Hash", "Dir", "Type", "Ledger", "Result"]).style(theme::header_row_style());
    let rows = txs.into_iter().map(tx_table_row);
    Table::new(
        rows,
        [
            Constraint::Length(19),
            Constraint::Length(4),
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Fill(1),
        ],
    )
    .header(header)
    .column_spacing(1)
}

pub fn titled_block(title: &str, is_focused: bool) -> Block<'static> {
    theme::panel_block(title, is_focused)
}

pub fn titled_block_with_count(
    title: &str,
    selected: Option<usize>,
    total: usize,
    is_focused: bool,
) -> Block<'static> {
    if total == 0 {
        return theme::panel_block(title, is_focused);
    }
    let titled = match selected {
        Some(i) => format!(" {title} ({}/{}) ", i + 1, total),
        None => format!(" {title} ({total}) "),
    };
    theme::panel_block_owned(titled, is_focused)
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

/// Centered popup rect (~80% of `area`) that never panics when `area` is smaller than `min_w`/`min_h`.
pub fn centered_popup_rect(area: Rect, min_w: u16, min_h: u16) -> Rect {
    let max_w = area.width.saturating_sub(4).max(1);
    let max_h = area.height.saturating_sub(2).max(1);
    let min_w = min_w.min(max_w);
    let min_h = min_h.min(max_h);
    let popup_w = ((area.width * 4 / 5).max(1)).clamp(min_w, max_w);
    let popup_h = ((area.height * 4 / 5).max(1)).clamp(min_h, max_h);
    let popup_x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let popup_y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    Rect::new(popup_x, popup_y, popup_w, popup_h)
}

#[cfg(test)]
mod popup_tests {
    use super::*;

    #[test]
    fn centered_popup_small_area_no_panic() {
        for area in [
            Rect::new(0, 0, 10, 5),
            Rect::new(0, 0, 1, 1),
            Rect::new(0, 0, 0, 0),
        ] {
            let popup = centered_popup_rect(area, 40, 12);
            assert!(popup.width > 0 && popup.height > 0);
            if area.width > 0 && area.height > 0 {
                assert!(popup.x + popup.width <= area.right());
                assert!(popup.y + popup.height <= area.bottom());
            }
        }
    }
}
