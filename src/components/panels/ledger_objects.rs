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
            selectable_table::SelectableTableState,
            theme,
            tx_detail::{TxDetailState, render_tx_detail},
            widgets::{render_empty, render_loading, titled_block_with_count},
        },
    },
    xrpl::{
        ArcValue, LedgerObjectRow, is_escrow_type, is_objects_tab_ledger_type, is_pay_channel_type,
    },
};

#[derive(Clone, Copy, Default)]
pub enum LedgerObjectFilter {
    /// Check, Ticket, MPT, DepositPreauth, SignerList, DID (credential / XLS-40), …
    #[default]
    ObjectsTab,
    PayChannelOnly,
    EscrowOnly,
}

impl LedgerObjectFilter {
    fn keep(&self, r: &LedgerObjectRow) -> bool {
        match self {
            LedgerObjectFilter::ObjectsTab => is_objects_tab_ledger_type(&r.ledger_type),
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
            Action::XrplLedgerObjects(all) => {
                self.apply_filter(all);
            }
            Action::SelectNext if !self.rows.is_empty() && self.is_focused => {
                self.table_state.select_next(self.rows.len());
            }
            Action::SelectPrev if !self.rows.is_empty() && self.is_focused => {
                self.table_state.select_prev(self.rows.len());
            }
            Action::TxDetailToggle if self.is_focused && !self.rows.is_empty() => {
                if let Some(idx) = self.table_state.selected()
                    && let Some(row) = self.rows.get(idx)
                {
                    self.detail.open(row.raw_json.clone(), ArcValue::default());
                }
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
                r.ledger_type.clone(),
                r.index.chars().take(20).collect::<String>(),
                r.detail.chars().take(64).collect::<String>(),
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

        render_tx_detail(frame, area, &self.detail);
        Ok(())
    }
}
