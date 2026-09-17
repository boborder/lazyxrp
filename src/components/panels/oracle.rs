use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    widgets::{Cell, Paragraph, Row, Table},
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            selectable_table::{SelectableTableState, render_selectable_table},
            theme,
            widgets::{render_loading, titled_block_with_count},
        },
    },
    xrpl::{AggregatePrice, asset_display_name},
};

#[derive(Default)]
pub struct OraclePanel {
    prices: Vec<AggregatePrice>,
    table_state: SelectableTableState,
    tick: usize,
    pub is_focused: bool,
    not_configured: bool,
}

impl OraclePanel {
    pub(crate) fn render_content(&mut self, frame: &mut Frame, area: Rect) {
        let has_xrpl = !self.prices.is_empty();

        if self.not_configured && !has_xrpl {
            let para = Paragraph::new("No XRPL oracles — set [[xrpl.oracles]] in config");
            frame.render_widget(para, area);
            return;
        }
        if !has_xrpl {
            render_loading(
                frame,
                area,
                "Oracle",
                self.tick,
                "loading prices...",
                self.is_focused,
            );
            return;
        }

        let rows = self.prices.iter().map(|p| {
            let pair = format!(
                "{}/{}",
                asset_display_name(&p.base_asset),
                asset_display_name(&p.quote_asset)
            );
            Row::new(vec![
                Cell::from(pair),
                Cell::from(p.entire_set.mean.as_str()),
                Cell::from(p.entire_set.size.to_string()),
                Cell::from(p.entire_set.standard_deviation.as_str()),
                Cell::from(if p.time > 0 {
                    p.time.to_string()
                } else {
                    "-".to_owned()
                }),
            ])
        });
        let table = Table::new(
            rows,
            [
                Constraint::Length(14),
                Constraint::Length(12),
                Constraint::Length(6),
                Constraint::Length(10),
                Constraint::Fill(1),
            ],
        )
        .header(
            Row::new(vec!["Pair", "Mean", "Size", "Std Dev", "Time"])
                .style(theme::header_row_style()),
        );
        render_selectable_table(frame, area, table, &mut self.table_state, self.is_focused);
    }
}
impl Component for OraclePanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplOraclePrices(prices) => {
                self.prices = prices.clone();
                self.prices.sort_by_cached_key(|p| {
                    format!(
                        "{}/{}",
                        asset_display_name(&p.base_asset),
                        asset_display_name(&p.quote_asset)
                    )
                });
                self.table_state.reset_len(self.prices.len());
                self.not_configured = false;
            }
            Action::XrplOracleNotConfigured => {
                self.not_configured = true;
            }
            _a if self.table_state.handle_row_select(
                action,
                self.is_focused,
                self.prices.len(),
            ) => {}
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if self.prices.is_empty() {
            // Keep loading / not-configured messaging inside content (owns its own block).
            self.render_content(frame, area);
            return Ok(());
        }
        let block = titled_block_with_count(
            "Oracle",
            self.table_state.selected(),
            self.prices.len(),
            self.is_focused,
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);
        self.render_content(frame, inner);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xrpl::PriceStats;

    #[test]
    fn oracle_panel_renders_not_configured_and_price_rows() {
        let mut panel = OraclePanel::default();
        panel.update(&Action::XrplOracleNotConfigured).unwrap();
        let out = crate::test_support::render_to_string(90, 8, |frame| {
            panel.draw(frame, frame.area()).unwrap()
        });
        assert!(out.contains("No XRPL oracles"));

        panel
            .update(&Action::XrplOraclePrices(vec![AggregatePrice {
                base_asset: "XRP".into(),
                quote_asset: "USD".into(),
                entire_set: PriceStats {
                    mean: "0.52".into(),
                    size: 3,
                    standard_deviation: "0.01".into(),
                },
                trimmed_set: None,
                time: 1_700_000_000,
            }]))
            .unwrap();
        let out = crate::test_support::render_to_string(90, 8, |frame| {
            panel.draw(frame, frame.area()).unwrap()
        });
        assert!(out.contains("XRP/USD"));
        assert!(out.contains("0.52"));
    }
}
