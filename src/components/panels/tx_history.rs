use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            selectable_table::SelectableTableState,
            tx_detail::{TxDetailState, render_tx_detail},
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
    filtered: Option<Vec<TxRow>>,
    table_state: SelectableTableState,
    tick: usize,
    received: bool,
    pub is_focused: bool,
    detail: TxDetailState,
    marker: Option<serde_json::Value>,
    has_more: bool,
    loading_more: bool,
    filter_mode: bool,
    filter_input: String,
}

impl TxHistoryPanel {
    pub fn new() -> Self {
        Self {
            is_focused: false,
            ..Self::default()
        }
    }

    fn reapply_filter(&mut self) {
        if self.filter_input.is_empty() {
            self.filtered = None;
        } else {
            let f = self.filter_input.to_lowercase();
            self.filtered = Some(
                self.txs
                    .iter()
                    .filter(|r| {
                        r.tx_type.to_lowercase().contains(&f) || r.hash.to_lowercase().contains(&f)
                    })
                    .cloned()
                    .collect(),
            );
        }
        let count = self.row_count();
        self.table_state.reset_len(count);
    }

    fn row_count(&self) -> usize {
        self.filtered
            .as_ref()
            .map(|v| v.len())
            .unwrap_or(self.txs.len())
    }

    fn display_rows(&self) -> &[TxRow] {
        self.filtered.as_deref().unwrap_or(&self.txs)
    }
}

impl Component for TxHistoryPanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        // Detail overlay takes precedence when open
        if self.detail.visible {
            match action {
                Action::TxDetailToggle => {
                    self.detail.close();
                    return Ok(None);
                }
                Action::SelectNext | Action::FocusNext => {
                    self.detail.scroll = self.detail.scroll.saturating_add(1);
                    return Ok(None);
                }
                Action::SelectPrev | Action::FocusPrev => {
                    self.detail.scroll = self.detail.scroll.saturating_sub(1);
                    return Ok(None);
                }
                Action::Quit => return Ok(None),
                _ => return Ok(None),
            }
        }

        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplTxHistory(txs, marker) => {
                self.txs = txs.to_vec();
                self.received = true;
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
            Action::SelectNext if self.row_count() > 0 && self.is_focused => {
                self.table_state.select_next(self.row_count());
            }
            Action::SelectPrev if self.row_count() > 0 && self.is_focused => {
                self.table_state.select_prev(self.row_count());
            }
            Action::TxDetailToggle if self.is_focused && self.row_count() > 0 => {
                let rows = self.display_rows();
                if let Some(idx) = self.table_state.selected()
                    && let Some(tx) = rows.get(idx)
                {
                    self.detail.open(tx.tx_json.clone(), tx.meta_json.clone());
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<Option<Action>> {
        if !self.is_focused || self.detail.visible {
            return Ok(None);
        }
        if self.filter_mode {
            match key.code {
                KeyCode::Enter => {
                    self.filter_mode = false;
                }
                KeyCode::Esc => {
                    self.filter_mode = false;
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
            self.filter_mode = true;
            self.filter_input.clear();
            self.reapply_filter();
            return Ok(None);
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
        let row_count = self.row_count();
        if row_count == 0 {
            let msg = if self.filter_input.is_empty() {
                "None"
            } else {
                "no matches"
            };
            render_empty(frame, area, "Tx History", msg, self.is_focused);
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

        let hint = if self.filter_mode {
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

        if let Some(ref filtered) = self.filtered {
            render_tx_scroll_table(
                frame,
                table_area,
                filtered,
                &mut self.table_state,
                self.is_focused,
            );
        } else {
            render_tx_scroll_table(
                frame,
                table_area,
                &self.txs,
                &mut self.table_state,
                self.is_focused,
            );
        }

        use ratatui::text::Line;
        use ratatui::widgets::Paragraph;
        frame.render_widget(
            Paragraph::new(Line::from(ratatui::text::Span::styled(
                hint,
                crate::components::shared::theme::dim_style(),
            ))),
            hint_area,
        );

        render_tx_detail(frame, area, &self.detail);
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
    fn filter_by_tx_type() {
        let mut panel = TxHistoryPanel::new();
        panel.txs = vec![
            dummy_tx_row("aaa", "Payment"),
            dummy_tx_row("bbb", "OfferCreate"),
            dummy_tx_row("ccc", "Payment"),
        ];
        panel.filter_input = "pay".to_string();
        panel.reapply_filter();
        assert_eq!(panel.row_count(), 2);
        assert!(panel.filtered.as_ref().unwrap()[0].tx_type == "Payment");
    }

    #[test]
    fn filter_by_hash_partial() {
        let mut panel = TxHistoryPanel::new();
        panel.txs = vec![
            dummy_tx_row("deadbeef", "Payment"),
            dummy_tx_row("cafebabe", "AccountSet"),
        ];
        panel.filter_input = "cafe".to_string();
        panel.reapply_filter();
        assert_eq!(panel.row_count(), 1);
        assert_eq!(panel.filtered.as_ref().unwrap()[0].hash, "cafebabe");
    }

    #[test]
    fn filter_empty_shows_all() {
        let mut panel = TxHistoryPanel::new();
        panel.txs = vec![
            dummy_tx_row("aaa", "Payment"),
            dummy_tx_row("bbb", "TrustSet"),
        ];
        panel.filter_input = String::new();
        panel.reapply_filter();
        assert_eq!(panel.row_count(), 2);
        assert!(panel.filtered.is_none());
    }
}
