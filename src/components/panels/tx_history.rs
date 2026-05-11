use ratatui::{Frame, layout::Rect};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            selectable_table::SelectableTableState,
            widgets::{
                render_empty, render_loading, render_tx_scroll_table, titled_block_with_count,
            },
        },
    },
    xrpl::TxRow,
};

#[derive(Default)]
pub struct TxHistoryPanel {
    txs: Vec<TxRow>,
    table_state: SelectableTableState,
    tick: usize,
    received: bool,
    pub is_focused: bool,
}

impl TxHistoryPanel {
    pub fn new() -> Self {
        Self {
            is_focused: false,
            ..Self::default()
        }
    }
}

impl Component for TxHistoryPanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplTxHistory(txs) => {
                self.txs = txs.to_vec();
                self.table_state.reset_len(self.txs.len());
                self.received = true;
            }
            Action::SelectNext if !self.txs.is_empty() && self.is_focused => {
                self.table_state.select_next(self.txs.len());
            }
            Action::SelectPrev if !self.txs.is_empty() && self.is_focused => {
                self.table_state.select_prev(self.txs.len());
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if !self.received {
            render_loading(
                frame,
                area,
                "Tx History",
                self.tick,
                "loading tx history...",
                self.is_focused,
            );
            return Ok(());
        }
        if self.txs.is_empty() {
            render_empty(frame, area, "Tx History", "None", self.is_focused);
            return Ok(());
        }
        let block = titled_block_with_count(
            "Tx History",
            self.table_state.selected(),
            self.txs.len(),
            self.is_focused,
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);

        render_tx_scroll_table(
            frame,
            inner,
            &self.txs,
            &mut self.table_state,
            self.is_focused,
        );
        Ok(())
    }
}
