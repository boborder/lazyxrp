#![allow(dead_code)]
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    widgets::{Block, Paragraph, Row, Table},
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{theme, widgets::render_loading},
    },
    xrpl::FlareFeedPrice,
};

#[derive(Default)]
pub struct FlareFtsoPanel {
    prices: Vec<FlareFeedPrice>,
    tick: usize,
    pub is_focused: bool,
}

impl FlareFtsoPanel {
    pub fn new() -> Self {
        Self {
            is_focused: false,
            ..Self::default()
        }
    }

    pub(crate) fn render_content(&mut self, frame: &mut Frame, area: Rect) {
        if self.prices.is_empty() {
            render_loading(
                frame,
                area,
                "FTSO",
                self.tick,
                "loading feeds...",
                self.is_focused,
            );
            frame.render_widget(
                Paragraph::new("set FLARE_FEEDS to customize pairs"),
                Rect {
                    x: area.x,
                    y: area.y.saturating_add(area.height.saturating_sub(1)),
                    width: area.width,
                    height: 1,
                },
            );
            return;
        }

        let rows = self.prices.iter().map(|fp| {
            Row::new(vec![
                fp.pair.clone(),
                fp.price.clone(),
                fp.timestamp.to_string(),
                fp.source.clone(),
            ])
        });
        let table = Table::new(
            rows,
            [
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Fill(1),
            ],
        )
        .header(Row::new(vec!["Pair", "Price", "Time", "Source"]).style(theme::header_row_style()));
        frame.render_widget(table, area);
    }
}

impl Component for FlareFtsoPanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::FlareOraclePrices(prices) => {
                self.prices = prices.clone();
                self.prices.sort_by(|a, b| a.pair.cmp(&b.pair));
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let block = Block::bordered()
            .title(" FTSOv2 ")
            .border_style(if self.is_focused {
                theme::accent_style()
            } else {
                theme::dim_style()
            });
        let inner = block.inner(area);
        frame.render_widget(block, area);
        self.render_content(frame, inner);
        Ok(())
    }
}
