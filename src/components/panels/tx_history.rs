use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::{Row, Scrollbar, ScrollbarOrientation, Table},
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            fmt,
            selectable_table::SelectableTableState,
            theme,
            widgets::{render_empty, render_loading, titled_block_with_count},
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

        let header =
            Row::new(vec!["Hash", "Type", "Ledger", "Result"]).style(theme::header_row_style());
        let rows = self.txs.iter().map(|t| {
            let result_style = if t.result == "tesSUCCESS" {
                theme::success_style()
            } else {
                theme::error_style()
            };
            let short_hash = if t.hash.chars().count() > 16 {
                format!("{}…", t.hash.chars().take(16).collect::<String>())
            } else {
                t.hash.clone()
            };
            Row::new(vec![
                short_hash,
                t.tx_type.clone(),
                fmt::group_digits(&t.ledger_index.to_string()),
                t.result.clone(),
            ])
            .style(result_style)
        });
        let table = Table::new(
            rows,
            [
                Constraint::Length(18),
                Constraint::Length(16),
                Constraint::Length(10),
                Constraint::Fill(1),
            ],
        )
        .header(header)
        .row_highlight_style(theme::selected_row_style(self.is_focused))
        .highlight_symbol("▶ ");

        let [tbl_area, sb_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);

        frame.render_stateful_widget(table, tbl_area, self.table_state.table_mut());
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(theme::dim_style())
                .thumb_style(theme::accent_style()),
            sb_area,
            self.table_state.scroll_mut(),
        );
        Ok(())
    }
}
