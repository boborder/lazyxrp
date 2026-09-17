use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    widgets::{Cell, Row, Table},
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            selectable_table::{SelectableTableState, render_selectable_table},
            theme,
            tx_detail::{TxDetailState, render_tx_detail},
            widgets::{render_empty, render_loading, titled_block_with_count},
        },
    },
    xrpl::{ArcValue, LedgerObjectRow, is_escrow_type, is_misc_ledger_type, is_pay_channel_type},
};

#[derive(Clone, Copy, Default)]
pub enum LedgerObjectFilter {
    /// Check, Ticket, MPT, DepositPreauth, SignerList, DID (credential / XLS-40), …
    #[default]
    MiscObjects,
    PayChannelOnly,
    EscrowOnly,
}

impl LedgerObjectFilter {
    fn keep(&self, r: &LedgerObjectRow) -> bool {
        match self {
            LedgerObjectFilter::MiscObjects => is_misc_ledger_type(&r.ledger_type),
            LedgerObjectFilter::PayChannelOnly => is_pay_channel_type(&r.ledger_type),
            LedgerObjectFilter::EscrowOnly => is_escrow_type(&r.ledger_type),
        }
    }
}

#[derive(Default)]
pub struct LedgerObjectsPanel {
    pub title: &'static str,
    filter: LedgerObjectFilter,
    rows: Vec<LedgerObjectRow>,
    table_state: SelectableTableState,
    tick: usize,
    received: bool,
    pub is_focused: bool,
    detail: TxDetailState,
}

impl LedgerObjectsPanel {
    pub fn new(title: &'static str, filter: LedgerObjectFilter) -> Self {
        Self {
            title,
            filter,
            is_focused: false,
            ..Self::default()
        }
    }

    fn apply_filter(&mut self, all: &[LedgerObjectRow]) {
        self.rows = all
            .iter()
            .filter(|r| self.filter.keep(r))
            .cloned()
            .collect();
        self.table_state.reset_len(self.rows.len());
        self.received = true;
    }
}

impl Component for LedgerObjectsPanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        let len = self.rows.len();
        let open = matches!(action, Action::TxDetailToggle)
            .then(|| self.table_state.selected_if_focused(self.is_focused, len))
            .flatten()
            .and_then(|idx| self.rows.get(idx))
            .map(|row| (row.raw_json.clone(), ArcValue::default()));
        if self.detail.handle_panel_action(action, open) {
            return Ok(None);
        }
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplLedgerObjects(all) => {
                self.apply_filter(all);
            }
            _ if self
                .table_state
                .handle_row_select(action, self.is_focused, len) => {}
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if !self.received {
            render_loading(
                frame,
                area,
                self.title,
                self.tick,
                "loading ledger objects...",
                self.is_focused,
            );
            return Ok(());
        }
        if self.rows.is_empty() {
            render_empty(
                frame,
                area,
                self.title,
                "(none for this account)",
                self.is_focused,
            );
            return Ok(());
        }
        let block = titled_block_with_count(
            self.title,
            self.table_state.selected(),
            self.rows.len(),
            self.is_focused,
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let header =
            Row::new(vec!["Type", "Object index", "Detail"]).style(theme::header_row_style());
        let rows = self.rows.iter().map(|r| {
            Row::new(vec![
                Cell::from(r.ledger_type.as_str()),
                Cell::from(r.index.chars().take(20).collect::<String>()),
                Cell::from(r.detail.chars().take(64).collect::<String>()),
            ])
        });
        let table = Table::new(
            rows,
            [
                Constraint::Length(14),
                Constraint::Length(22),
                Constraint::Fill(1),
            ],
        )
        .header(header);

        render_selectable_table(frame, inner, table, &mut self.table_state, self.is_focused);

        render_tx_detail(frame, area, &mut self.detail);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xrpl::ArcValue;

    fn row(ledger_type: &str) -> LedgerObjectRow {
        LedgerObjectRow {
            ledger_type: ledger_type.into(),
            index: "IDX".into(),
            detail: "detail".into(),
            raw_json: ArcValue::default(),
        }
    }

    /// TC-151: ledger object filters classify rows without rendering.
    #[test]
    fn ledger_object_filter_keep_classifies_rows() {
        let misc = LedgerObjectFilter::MiscObjects;
        let pay = LedgerObjectFilter::PayChannelOnly;
        let escrow = LedgerObjectFilter::EscrowOnly;

        assert!(misc.keep(&row("Check")));
        assert!(misc.keep(&row("Ticket")));
        assert!(!misc.keep(&row("PayChannel")));
        assert!(!misc.keep(&row("Escrow")));

        assert!(pay.keep(&row("PayChannel")));
        assert!(!pay.keep(&row("Escrow")));
        assert!(!pay.keep(&row("Check")));

        assert!(escrow.keep(&row("Escrow")));
        assert!(!escrow.keep(&row("PayChannel")));
        assert!(!escrow.keep(&row("Ticket")));
    }

    #[test]
    fn apply_filter_keeps_only_matching_rows() {
        let mut panel = LedgerObjectsPanel::new("objects", LedgerObjectFilter::PayChannelOnly);
        let all = vec![
            row("PayChannel"),
            row("Escrow"),
            row("Check"),
            row("PayChannel"),
        ];
        panel.update(&Action::XrplLedgerObjects(all)).unwrap();
        let out = crate::test_support::render_to_string(80, 10, |frame| {
            panel.draw(frame, frame.area()).unwrap()
        });
        assert_eq!(out.matches("PayChannel").count(), 2);
        assert!(!out.contains("Escrow"));
        assert!(!out.contains("Check"));
    }
}
