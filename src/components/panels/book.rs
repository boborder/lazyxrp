use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Bar, BarChart, BarGroup, Cell, Row, Table},
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
    xrpl::{ArcValue, OfferRow},
};

#[derive(Default)]
pub struct BookPanel {
    offers: Vec<OfferRow>,
    table_state: SelectableTableState,
    tick: usize,
    has_received_offers: bool,
    pub is_focused: bool,
    detail: TxDetailState,
}

impl Component for BookPanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        let len = self.offers.len();
        let open = matches!(action, Action::TxDetailToggle)
            .then(|| self.table_state.selected_if_focused(self.is_focused, len))
            .flatten()
            .and_then(|idx| self.offers.get(idx))
            .map(|offer| (offer.raw_json.clone(), ArcValue::default()));
        if self.detail.handle_panel_action(action, open) {
            return Ok(None);
        }
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplBookOffers(offers) => {
                self.offers = offers.to_vec();
                self.table_state.reset_len(self.offers.len());
                self.has_received_offers = true;
            }
            _ if self
                .table_state
                .handle_row_select(action, self.is_focused, len) => {}
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if !self.has_received_offers {
            render_loading(
                frame,
                area,
                "Book Offers",
                self.tick,
                "loading order book...",
                self.is_focused,
            );
            return Ok(());
        }
        if self.offers.is_empty() {
            render_empty(frame, area, "Book Offers", "(no offers)", self.is_focused);
            return Ok(());
        }
        let block = titled_block_with_count(
            "Book Offers",
            self.table_state.selected(),
            self.offers.len(),
            self.is_focused,
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chart_height = if self.offers.len() >= 2 { 5u16 } else { 0 };
        let [table_area, chart_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(chart_height)]).areas(inner);

        // ── Table with StatefulWidget ──
        let header = Row::new(vec!["Quality", "Price", "TakerGets", "TakerPays"])
            .style(theme::header_row_style());
        let rows = self.offers.iter().map(|o| {
            Row::new(vec![
                Cell::from(o.quality.as_str()),
                Cell::from(o.price.as_str()),
                Cell::from(o.taker_gets.as_str()),
                Cell::from(o.taker_pays.as_str()),
            ])
        });
        let table = Table::new(
            rows,
            [
                Constraint::Length(14),
                Constraint::Length(12),
                Constraint::Fill(1),
                Constraint::Fill(1),
            ],
        )
        .header(header);

        render_selectable_table(
            frame,
            table_area,
            table,
            &mut self.table_state,
            self.is_focused,
        );

        // ── BarChart::grouped — quality distribution ──
        if chart_height > 0 {
            let gets_bars: Vec<Bar<'_>> = self
                .offers
                .iter()
                .take(8)
                .enumerate()
                .map(|(i, o)| {
                    let quality_value = o.quality.parse::<f64>().unwrap_or(0.0);
                    Bar::default()
                        .value((quality_value * 1_000.0) as u64)
                        .label(Line::from(format!("#{}", i + 1)))
                })
                .collect();
            let pays_bars: Vec<Bar<'_>> = self
                .offers
                .iter()
                .take(8)
                .enumerate()
                .map(|(i, o)| {
                    let quality_value = o.quality.parse::<f64>().unwrap_or(0.0);
                    Bar::default()
                        .value((quality_value * 800.0) as u64)
                        .label(Line::from(format!("#{}", i + 1)))
                })
                .collect();
            let chart = BarChart::grouped([
                BarGroup::default()
                    .label(Line::from("Gets×1k"))
                    .bars(&gets_bars),
                BarGroup::default()
                    .label(Line::from("Pays×0.8k"))
                    .bars(&pays_bars),
            ])
            .bar_width(3)
            .bar_gap(0)
            .bar_style(theme::accent_style())
            .value_style(theme::dim_style());
            frame.render_widget(chart, chart_area);
        }

        render_tx_detail(frame, area, &mut self.detail);
        Ok(())
    }
}
