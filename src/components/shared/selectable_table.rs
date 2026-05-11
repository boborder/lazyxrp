use ratatui::widgets::{ScrollbarState, TableState};

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
