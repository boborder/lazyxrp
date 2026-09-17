use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState},
};

use crate::action::Action;

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

    /// Selected row when the panel is focused and has at least one row.
    pub fn selected_if_focused(&self, is_focused: bool, len: usize) -> Option<usize> {
        (is_focused && len > 0).then(|| self.selected()).flatten()
    }

    /// Handle `SelectNext` / `SelectPrev` for a focused non-empty table.
    pub fn handle_row_select(&mut self, action: &Action, is_focused: bool, len: usize) -> bool {
        match action {
            Action::SelectNext if is_focused && len > 0 => {
                self.select_next(len);
                true
            }
            Action::SelectPrev if is_focused && len > 0 => {
                self.select_prev(len);
                true
            }
            _ => false,
        }
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
/// Returns the table so callers may retain its owned rows between redraws.
pub fn render_selectable_table<'a>(
    frame: &mut Frame,
    area: Rect,
    table: Table<'a>,
    table_state: &mut SelectableTableState,
    is_focused: bool,
) -> Table<'a> {
    if area.width == 0 || area.height == 0 {
        return table;
    }

    let table = table
        .row_highlight_style(theme::selected_row_style(is_focused))
        .highlight_symbol("▶ ");

    let [tbl_area, sb_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

    frame.render_stateful_widget(&table, tbl_area, table_state.table_mut());
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(theme::dim_style())
            .thumb_style(theme::accent_style()),
        sb_area,
        table_state.scroll_mut(),
    );
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC-133: selection bounds — select_next stops at the last row, reset_len
    /// clamps a stale selection to the new length, and len=0 clears selection.
    #[test]
    fn selectable_table_state_bounds() {
        let mut st = SelectableTableState::default();

        // select_next from no selection enters row 0, then stops at the end.
        for _ in 0..10 {
            st.select_next(5);
        }
        assert_eq!(st.selected(), Some(4));
        st.select_prev(5);
        assert_eq!(st.selected(), Some(3));

        // Shrinking reset_len clamps the stale selection.
        st.reset_len(2);
        assert_eq!(st.selected(), Some(1));

        // Empty reset_len and navigation on an empty table deselect cleanly.
        st.reset_len(0);
        assert_eq!(st.selected(), None);
        st.select_next(0);
        st.select_prev(0);
        assert_eq!(st.selected(), None);
    }
}
