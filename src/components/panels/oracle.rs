use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    widgets::{Paragraph, Row, Table},
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
    pub fn new() -> Self {
        Self {
            is_focused: true,
            ..Self::default()
        }
    }

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

        let rows: Vec<Row> = self
            .prices
            .iter()
            .map(|p| {
                let pair = format!(
                    "{}/{}",
                    asset_display_name(&p.base_asset),
                    asset_display_name(&p.quote_asset)
                );
                Row::new(vec![
                    pair,
                    p.entire_set.mean.clone(),
                    p.entire_set.size.to_string(),
                    p.entire_set.standard_deviation.clone(),
                    if p.time > 0 {
                        p.time.to_string()
                    } else {
                        "-".into()
                    },
                ])
            })
            .collect();
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
                self.prices.sort_by(|a, b| {
                    let ak = format!(
                        "{}/{}",
                        asset_display_name(&a.base_asset),
                        asset_display_name(&a.quote_asset)
                    );
                    let bk = format!(
                        "{}/{}",
                        asset_display_name(&b.base_asset),
                        asset_display_name(&b.quote_asset)
                    );
                    ak.cmp(&bk)
                });
                self.table_state.reset_len(self.prices.len());
                self.not_configured = false;
            }
            Action::XrplOracleNotConfigured => {
                self.not_configured = true;
            }
            Action::SelectNext if !self.prices.is_empty() && self.is_focused => {
                self.table_state.select_next(self.prices.len());
            }
            Action::SelectPrev if !self.prices.is_empty() && self.is_focused => {
                self.table_state.select_prev(self.prices.len());
            }
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
