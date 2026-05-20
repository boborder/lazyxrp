use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState},
};

use super::theme;

#[derive(Default)]
pub struct SelectableTableState {
    table: TableState,
    scroll: ScrollbarState,
}

impl SelectableTableState {
    pub fn selected(&self) -> Option<usize> {
        self.table.selected()
    }

    pub fn table_mut(&mut self) -> &mut TableState {
        &mut self.table
    }

    pub fn scroll_mut(&mut self) -> &mut ScrollbarState {
        &mut self.scroll
    }

    pub fn reset_len(&mut self, len: usize) {
        self.scroll = ScrollbarState::new(len);
        self.clamp(len);
    }

    pub fn select_next(&mut self, len: usize) {
        if len == 0 {
            self.table.select(None);
            self.scroll = self.scroll.position(0);
            return;
        }
        let next = self.table.selected().map_or(0, |i| (i + 1).min(len - 1));
        self.select(Some(next));
    }

    pub fn select_prev(&mut self, len: usize) {
        if len == 0 {
            self.table.select(None);
            self.scroll = self.scroll.position(0);
            return;
        }
        let prev = self.table.selected().map_or(0, |i| i.saturating_sub(1));
        self.select(Some(prev));
    }

    fn clamp(&mut self, len: usize) {
        if len == 0 {
            self.select(None);
            return;
        }
        let selected = self.table.selected().unwrap_or(0).min(len - 1);
        self.select(Some(selected));
    }

    fn select(&mut self, selected: Option<usize>) {
        self.table.select(selected);
        self.scroll = self.scroll.position(selected.unwrap_or(0));
    }
}

/// Table body + vertical scrollbar (1-col). Selection chrome is applied here.
///
/// Caller builds header / rows / constraints (and optional `column_spacing`).
/// Adjacent UI (charts, summaries, detail overlays) stays outside this helper.
/// Selectable-table scrollbar thumb is always [`theme::accent_style`].
pub fn render_selectable_table(
    frame: &mut Frame,
    area: Rect,
    table: Table<'_>,
    table_state: &mut SelectableTableState,
    is_focused: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let table = table
        .row_highlight_style(theme::selected_row_style(is_focused))
        .highlight_symbol("▶ ");

    let [tbl_area, sb_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

    frame.render_stateful_widget(table, tbl_area, table_state.table_mut());
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(theme::dim_style())
            .thumb_style(theme::accent_style()),
        sb_area,
        table_state.scroll_mut(),
    );
}
