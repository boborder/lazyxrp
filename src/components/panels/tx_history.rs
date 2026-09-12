use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::Table,
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            selectable_table::{SelectableTableState, render_selectable_table},
            tx_detail::{TxDetailState, render_tx_detail},
            widgets::{render_empty, render_loading, titled_block_with_count, tx_table},
        },
    },
    xrpl::TxRow,
};

#[derive(Default)]
pub struct TxHistoryPanel {
    txs: Vec<TxRow>,
    filtered: Option<Vec<usize>>,
    table_state: SelectableTableState,
    table: Table<'static>,
    tick: usize,
    has_received_history: bool,
    pub is_focused: bool,
    detail: TxDetailState,
    marker: Option<serde_json::Value>,
    has_more: bool,
    loading_more: bool,
    is_filter_mode: bool,
    filter_input: String,
}

impl TxHistoryPanel {
    fn reapply_filter(&mut self) {
        if self.filter_input.is_empty() {
            self.filtered = None;
        } else {
            let f = self.filter_input.to_lowercase();
            self.filtered = Some(
                self.txs
                    .iter()
                    .enumerate()
                    .filter_map(|(index, r)| {
                        (r.tx_type.to_lowercase().contains(&f)
                            || r.hash.to_lowercase().contains(&f))
                        .then_some(index)
                    })
                    .collect(),
            );
        }
        let count = self.row_count();
        self.table_state.reset_len(count);
        self.table = tx_table((0..count).filter_map(|index| self.display_row(index)));
    }

    fn row_count(&self) -> usize {
        self.filtered
            .as_ref()
            .map(|v| v.len())
            .unwrap_or(self.txs.len())
    }

    fn display_row(&self, index: usize) -> Option<&TxRow> {
        self.txs.get(match &self.filtered {
            Some(indices) => *indices.get(index)?,
            None => index,
        })
    }
}

impl Component for TxHistoryPanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        let len = self.row_count();
        let open = matches!(action, Action::TxDetailToggle)
            .then(|| self.table_state.selected_if_focused(self.is_focused, len))
            .flatten()
            .and_then(|idx| self.display_row(idx))
            .map(|tx| (tx.tx_json.clone(), tx.meta_json.clone()));
        if self.detail.handle_panel_action(action, open) {
            return Ok(None);
        }

        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplTxHistory(txs, marker) => {
                self.txs = txs.to_vec();
                self.has_received_history = true;
                self.marker = marker.clone();
                self.has_more = marker.is_some();
                self.loading_more = false;
                self.reapply_filter();
            }
            Action::XrplTxHistoryAppend(txs, marker) => {
                self.txs.extend(txs.iter().cloned());
                self.marker = marker.clone();
                self.has_more = marker.is_some();
                self.loading_more = false;
                self.reapply_filter();
            }
            _ if self
                .table_state
                .handle_row_select(action, self.is_focused, len) => {}
            _ => {}
        }
        Ok(None)
    }

    fn on_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<Option<Action>> {
        if !self.is_focused || self.detail.visible {
            return Ok(None);
        }
        if self.is_filter_mode {
            match key.code {
                KeyCode::Enter => {
                    self.is_filter_mode = false;
                }
                KeyCode::Esc => {
                    self.is_filter_mode = false;
                    self.filter_input.clear();
                    self.reapply_filter();
                }
                KeyCode::Char(c) => {
                    self.filter_input.push(c);
                    self.reapply_filter();
                }
                KeyCode::Backspace => {
                    self.filter_input.pop();
                    self.reapply_filter();
                }
                _ => {}
            }
            return Ok(None);
        }
        if key.code == KeyCode::Char('m') && self.has_more && !self.loading_more {
            self.loading_more = true;
            return Ok(Some(Action::RefreshTxHistoryMore(self.marker.clone())));
        }
        if key.code == KeyCode::Char('f') {
            self.is_filter_mode = true;
            self.filter_input.clear();
            self.reapply_filter();
            return Ok(None);
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if !self.has_received_history {
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
        let row_count = self.row_count();
        if row_count == 0 {
            let empty_history_label = if self.filter_input.is_empty() {
                "None"
            } else {
                "no matches"
            };
            render_empty(
                frame,
                area,
                "Tx History",
                empty_history_label,
                self.is_focused,
            );
            return Ok(());
        }
        let title = if self.filter_input.is_empty() {
            "Tx History".to_string()
        } else {
            format!("Tx History [filter: {}]", self.filter_input)
        };
        let block = titled_block_with_count(
            &title,
            self.table_state.selected(),
            row_count,
            self.is_focused,
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let hint = if self.is_filter_mode {
            format!("Filter: {}_", self.filter_input)
        } else if self.loading_more {
            "loading more…".to_string()
        } else if self.has_more {
            "f: filter · m: more".to_string()
        } else {
            "f: filter".to_string()
        };

        let [table_area, hint_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);

        self.table = render_selectable_table(
            frame,
            table_area,
            std::mem::take(&mut self.table),
            &mut self.table_state,
            self.is_focused,
        );

        use ratatui::text::Line;
        use ratatui::widgets::Paragraph;
        frame.render_widget(
            Paragraph::new(Line::from(ratatui::text::Span::styled(
                hint,
                crate::components::shared::theme::dim_style(),
            ))),
            hint_area,
        );

        render_tx_detail(frame, area, &mut self.detail);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xrpl::ArcValue;

    fn dummy_tx_row(hash: &str, tx_type: &str) -> TxRow {
        TxRow {
            hash: hash.to_string(),
            tx_type: tx_type.to_string(),
            ledger_index: 1,
            result: "tesSUCCESS".to_string(),
            direction: "·".to_string(),
            tx_json: ArcValue::new(serde_json::json!({"hash": hash, "TransactionType": tx_type})),
            meta_json: ArcValue::new(serde_json::json!({})),
        }
    }

    #[test]
    /// TC-077
    fn filter_by_tx_type() {
        let mut panel = TxHistoryPanel {
            txs: vec![
                dummy_tx_row("aaa", "Payment"),
                dummy_tx_row("bbb", "OfferCreate"),
                dummy_tx_row("ccc", "Payment"),
            ],
            filter_input: "pay".to_string(),
            ..Default::default()
        };
        panel.reapply_filter();
        assert_eq!(panel.row_count(), 2);
        assert_eq!(panel.display_row(0).unwrap().tx_type, "Payment");
    }

    #[test]
    /// TC-078
    fn filter_by_hash_partial() {
        let mut panel = TxHistoryPanel {
            txs: vec![
                dummy_tx_row("deadbeef", "Payment"),
                dummy_tx_row("cafebabe", "AccountSet"),
            ],
            filter_input: "cafe".to_string(),
            ..Default::default()
        };
        panel.reapply_filter();
        assert_eq!(panel.row_count(), 1);
        assert_eq!(panel.display_row(0).unwrap().hash, "cafebabe");
    }

    #[test]
    /// TC-079
    fn filter_empty_shows_all() {
        let mut panel = TxHistoryPanel {
            txs: vec![
                dummy_tx_row("aaa", "Payment"),
                dummy_tx_row("bbb", "TrustSet"),
            ],
            ..Default::default()
        };
        panel.reapply_filter();
        assert_eq!(panel.row_count(), 2);
        assert!(panel.filtered.is_none());
    }

    #[test]
    fn cached_rows_follow_filter_append_and_selected_detail() {
        let mut panel = TxHistoryPanel {
            is_focused: true,
            ..Default::default()
        };
        panel
            .update(&Action::XrplTxHistory(
                vec![
                    dummy_tx_row("first", "Payment"),
                    dummy_tx_row("excluded", "OfferCreate"),
                ],
                None,
            ))
            .unwrap();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 20)).unwrap();
        panel
            .on_key_event(KeyEvent::from(KeyCode::Char('f')))
            .unwrap();
        for c in "pay".chars() {
            panel
                .on_key_event(KeyEvent::from(KeyCode::Char(c)))
                .unwrap();
        }
        panel.on_key_event(KeyEvent::from(KeyCode::Enter)).unwrap();
        panel
            .update(&Action::XrplTxHistoryAppend(
                vec![dummy_tx_row("second", "Payment")],
                None,
            ))
            .unwrap();
        panel.update(&Action::SelectNext).unwrap();
        terminal
            .draw(|frame| panel.draw(frame, frame.area()).unwrap())
            .unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("first"));
        assert!(rendered.contains("second"));
        assert!(!rendered.contains("excluded"));
        panel.update(&Action::TxDetailToggle).unwrap();
        assert_eq!(panel.detail.tx_json.0["hash"], "second");
        panel.update(&Action::TxDetailToggle).unwrap();
        panel
            .update(&Action::XrplTxHistory(
                vec![dummy_tx_row("replacement", "Payment")],
                None,
            ))
            .unwrap();
        terminal
            .draw(|frame| panel.draw(frame, frame.area()).unwrap())
            .unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("replacement"));
        assert!(!rendered.contains("first"));
        assert!(!rendered.contains("second"));
    }
}
