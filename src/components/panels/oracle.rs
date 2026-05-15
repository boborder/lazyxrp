use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    widgets::{Block, Paragraph, Row, Scrollbar, ScrollbarOrientation, Table},
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{selectable_table::SelectableTableState, theme, widgets::render_loading},
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
}

impl Component for OraclePanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplOraclePrices(prices) => {
                self.prices = prices.clone();
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
        let block = Block::bordered()
            .title(" Oracle Aggregate Prices ")
            .border_style(if self.is_focused {
                theme::accent_style()
            } else {
                theme::dim_style()
            });
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.not_configured {
            let para = Paragraph::new(
                "No oracles configured — add [[xrpl.oracles]] to your config.toml",
            );
            frame.render_widget(para, inner);
            return Ok(());
        }
        if self.prices.is_empty() {
            render_loading(
                frame,
                inner,
                "Oracle",
                self.tick,
                "loading prices...",
                self.is_focused,
            );
            return Ok(());
        }

        let header = Row::new(vec!["Pair", "Mean", "Size", "Std Dev", "Time"])
            .style(theme::header_row_style());
        let rows = self.prices.iter().map(|p| {
            let pair = format!(
                "{}/{}",
                asset_display_name(&p.base_asset),
                asset_display_name(&p.quote_asset)
            );
            let time = if p.time > 0 {
                format!("{}", p.time)
            } else {
                "-".into()
            };
            Row::new(vec![
                pair,
                p.entire_set.mean.clone(),
                p.entire_set.size.to_string(),
                p.entire_set.standard_deviation.clone(),
                time,
            ])
        });
        let table = Table::new(
            rows,
            [
                Constraint::Length(14),
                Constraint::Length(14),
                Constraint::Length(6),
                Constraint::Length(12),
                Constraint::Fill(1),
            ],
        )
        .header(header)
        .row_highlight_style(theme::selected_row_style(self.is_focused))
        .highlight_symbol("▶ ");

        let [tbl_area, sb_area] =
            ratatui::layout::Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)])
                .areas(inner);

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
