use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::{Bar, BarChart, BarGroup, Row, Scrollbar, ScrollbarOrientation, Table},
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

impl BookPanel {
    pub fn new() -> Self {
        Self {
            is_focused: false,
            ..Self::default()
        }
    }
}

impl Component for BookPanel {
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
            Action::XrplBookOffers(offers) => {
                self.offers = offers.to_vec();
                self.table_state.reset_len(self.offers.len());
                self.has_received_offers = true;
            }
            Action::SelectNext if !self.offers.is_empty() && self.is_focused => {
                self.table_state.select_next(self.offers.len());
            }
            Action::SelectPrev if !self.offers.is_empty() && self.is_focused => {
                self.table_state.select_prev(self.offers.len());
            }
            Action::TxDetailToggle if self.is_focused && !self.offers.is_empty() => {
                if let Some(idx) = self.table_state.selected()
                    && let Some(offer) = self.offers.get(idx)
                {
                    self.detail
                        .open(offer.raw_json.clone(), ArcValue::default());
                }
            }
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
                o.quality.clone(),
                o.price.clone(),
                o.taker_gets.clone(),
                o.taker_pays.clone(),
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
        .header(header)
        .row_highlight_style(theme::selected_row_style(self.is_focused))
        .highlight_symbol("▶ ");

        // leave 1-col gap on the right for the scrollbar
        let [tbl_area, sb_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)]).areas(table_area);

        frame.render_stateful_widget(table, tbl_area, self.table_state.table_mut());
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(theme::dim_style())
                .thumb_style(theme::accent_style()),
            sb_area,
            self.table_state.scroll_mut(),
        );

        // ── BarChart::grouped — quality distribution ──
        render_tx_detail(frame, area, &mut self.detail);

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
            .bar_style(Style::new().fg(theme::ACCENT))
            .value_style(theme::dim_style());
            frame.render_widget(chart, chart_area);
        }

        Ok(())
    }
}
