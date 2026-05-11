use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation, Table},
};

use crate::{
    components::shared::{fmt, selectable_table::SelectableTableState, theme},
    xrpl::TxRow,
};

/// One table row for [`TxRow`] with per-column colors (hash / type / ledger / result).
pub fn tx_table_row(t: &TxRow) -> Row<'_> {
    let result_style = if t.result == "tesSUCCESS" {
        theme::success_style()
    } else {
        theme::error_style()
    };
    let hash_cell = if t.hash.len() > 16 {
        Cell::from(format!("{}…", &t.hash[..16])).style(theme::secondary_style())
    } else {
        Cell::from(t.hash.as_str()).style(theme::secondary_style())
    };
    Row::new(vec![
        hash_cell,
        Cell::from(t.tx_type.as_str()).style(theme::accent_style()),
        Cell::from(fmt::group_digits_u64(u64::from(t.ledger_index))).style(theme::dim_style()),
        Cell::from(t.result.as_str()).style(result_style),
    ])
}

/// Recent-tx / Tx History table with header, column layout, highlight, and vertical scrollbar.
pub fn render_tx_scroll_table(
    frame: &mut Frame,
    area: Rect,
    txs: &[TxRow],
    table_state: &mut SelectableTableState,
    is_focused: bool,
) {
    let header =
        Row::new(vec!["Hash", "Type", "Ledger", "Result"]).style(theme::header_row_style());
    let rows = txs.iter().map(tx_table_row);
    let table = Table::new(
        rows,
        [
            Constraint::Length(19),
            Constraint::Length(16),
            Constraint::Length(10),
            Constraint::Fill(1),
        ],
    )
    .header(header)
    .column_spacing(1)
    .row_highlight_style(theme::selected_row_style(is_focused))
    .highlight_symbol("▶ ");

    let [tbl_area, sb_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

    frame.render_stateful_widget(table, tbl_area, table_state.table_mut());
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(theme::dim_style())
            .thumb_style(theme::secondary_style()),
        sb_area,
        table_state.scroll_mut(),
    );
}

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
