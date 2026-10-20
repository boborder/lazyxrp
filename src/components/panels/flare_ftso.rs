use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::{Line, Span},
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
    xrpl::FlareFeedPrice,
};

/// A feed older than this is flagged: FTSOv2 block-latency feeds update about every 1.8 s, so a
/// minute without an update means a stalled feed or a lagging RPC.
const FEED_STALE_SECS: u64 = 60;

/// Age of a feed value (`now - timestamp`) as a label, and whether it is stale. A zero
/// timestamp (never updated) is stale; a timestamp ahead of `now` (clock skew) reads as 0s.
fn feed_age(now_secs: u64, timestamp: u64) -> (String, bool) {
    if timestamp == 0 {
        return ("never".to_string(), true);
    }
    let age = now_secs.saturating_sub(timestamp);
    let label = match age {
        0..=119 => format!("{age}s"),
        120..=7199 => format!("{}m", age / 60),
        _ => format!("{}h", age / 3600),
    };
    (label, age > FEED_STALE_SECS)
}

#[derive(Default)]
pub struct FlareFtsoPanel {
    prices: Vec<FlareFeedPrice>,
    table_state: SelectableTableState,
    tick: usize,
    pub is_focused: bool,
}

impl FlareFtsoPanel {
    /// One-line FTSO summary for compact display (first feed + overflow count).
    pub(crate) fn compact_summary(&self) -> String {
        if self.prices.is_empty() {
            return "loading…".to_string();
        }
        let first = &self.prices[0];
        let extra = self.prices.len().saturating_sub(1);
        if extra == 0 {
            format!("{} {}", first.pair, first.price)
        } else {
            format!("{} {} · +{}", first.pair, first.price, extra)
        }
    }

    pub(crate) fn render_compact(&self, frame: &mut Frame, area: Rect) {
        let line = Line::from(vec![
            Span::styled("FTSO ", theme::dim_style()),
            Span::styled(self.compact_summary(), theme::secondary_style()),
        ]);
        frame.render_widget(Paragraph::new(line), area);
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

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let rows = self.prices.iter().map(|fp| {
            let (age, stale) = feed_age(now, fp.timestamp);
            let age_style = if stale {
                theme::warning_style()
            } else {
                theme::dim_style()
            };
            Row::new(vec![
                Cell::from(fp.pair.as_str()),
                Cell::from(fp.price.as_str()),
                Cell::from(age).style(age_style),
                Cell::from(fp.source.as_str()),
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
        .header(Row::new(vec!["Pair", "Price", "Age", "Source"]).style(theme::header_row_style()));
        render_selectable_table(frame, area, table, &mut self.table_state, self.is_focused);
    }
}

impl Component for FlareFtsoPanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::FlareOraclePrices(prices) => {
                self.prices = prices.clone();
                self.prices.sort_by(|a, b| a.pair.cmp(&b.pair));
                self.table_state.reset_len(self.prices.len());
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
            self.render_content(frame, area);
            return Ok(());
        }
        let block = titled_block_with_count(
            "FTSOv2",
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

    /// TC-338: feed age labels and the stale threshold (boundary, skew, never updated).
    #[test]
    fn feed_age_labels_and_staleness() {
        assert_eq!(feed_age(1_000, 998), ("2s".to_string(), false));
        assert_eq!(feed_age(1_060, 1_000), ("60s".to_string(), false));
        assert_eq!(feed_age(1_061, 1_000), ("61s".to_string(), true));
        assert_eq!(feed_age(1_000 + 600, 1_000), ("10m".to_string(), true));
        assert_eq!(feed_age(1_000 + 7_200, 1_000), ("2h".to_string(), true));
        assert_eq!(feed_age(1_000, 1_500), ("0s".to_string(), false));
        assert_eq!(feed_age(1_000, 0), ("never".to_string(), true));
    }
}
