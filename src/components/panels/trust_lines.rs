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
    xrpl::{ArcValue, TrustLineRow},
};

#[derive(Default)]
pub struct TrustLinesPanel {
    lines: Vec<TrustLineRow>,
    table_state: SelectableTableState,
    tick: usize,
    received: bool,
    pub is_focused: bool,
    detail: TxDetailState,
}

impl Component for TrustLinesPanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        let len = self.lines.len();
        let open = matches!(action, Action::TxDetailToggle)
            .then(|| self.table_state.selected_if_focused(self.is_focused, len))
            .flatten()
            .and_then(|idx| self.lines.get(idx))
            .map(|line| (line.raw_json.clone(), ArcValue::default()));
        if self.detail.handle_panel_action(action, open) {
            return Ok(None);
        }
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplTrustLines(lines) => {
                self.lines = lines.to_vec();
                self.table_state.reset_len(self.lines.len());
                self.received = true;
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
                "Trust Lines",
                self.tick,
                "loading trust lines...",
                self.is_focused,
            );
            return Ok(());
        }
        if self.lines.is_empty() {
            render_empty(
                frame,
                area,
                "Trust Lines",
                "(no trust lines)",
                self.is_focused,
            );
            return Ok(());
        }
        let block = titled_block_with_count(
            "Trust Lines",
            self.table_state.selected(),
            self.lines.len(),
            self.is_focused,
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let header = Row::new(vec!["Currency", "Issuer", "Balance", "Limit"])
            .style(theme::header_row_style());
        let rows = self.lines.iter().map(|l| {
            let balance_style = if l.balance.starts_with('-') {
                theme::error_style()
            } else {
                theme::success_style()
            };
            Row::new(vec![
                Cell::from(l.currency.as_str()),
                Cell::from(l.account.chars().take(12).collect::<String>()),
                Cell::from(l.balance.as_str()),
                Cell::from(l.limit.as_str()),
            ])
            .style(balance_style)
        });
        let table = Table::new(
            rows,
            [
                Constraint::Length(10),
                Constraint::Length(14),
                Constraint::Length(18),
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

    #[test]
    fn trust_lines_panel_renders_loading_empty_and_rows() {
        let mut panel = TrustLinesPanel::default();
        let loading = crate::test_support::render_to_string(90, 10, |frame| {
            panel.draw(frame, frame.area()).unwrap()
        });
        assert!(loading.contains("loading trust lines"));

        panel.update(&Action::XrplTrustLines(Vec::new())).unwrap();
        let empty = crate::test_support::render_to_string(90, 10, |frame| {
            panel.draw(frame, frame.area()).unwrap()
        });
        assert!(empty.contains("(no trust lines)"));

        panel
            .update(&Action::XrplTrustLines(vec![TrustLineRow {
                currency: "USD".into(),
                account: "rIssuer".into(),
                balance: "-2".into(),
                limit: "100".into(),
                raw_json: ArcValue::default(),
            }]))
            .unwrap();
        let out = crate::test_support::render_to_string(90, 10, |frame| {
            panel.draw(frame, frame.area()).unwrap()
        });
        assert!(out.contains("USD"));
        assert!(out.contains("rIssuer"));
        assert!(out.contains("-2"));
    }
}
