use std::collections::VecDeque;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{BarChart, Block, Paragraph},
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            fmt, theme,
            widgets::{render_loading, titled_block},
        },
    },
    xrpl::ServerInfoSummary,
};

const FEE_HISTORY_LEN: usize = 40;

#[derive(Default)]
pub struct ServerPanel {
    server_url: String,
    server_info: Option<ServerInfoSummary>,
    base_fee: Option<u32>,
    reserve_base: Option<u32>,
    fee_history: VecDeque<u64>,
    tick: usize,
    pub is_focused: bool,
}

impl ServerPanel {
    pub fn new(server_url: String) -> Self {
        Self {
            server_url,
            is_focused: false,
            ..Self::default()
        }
    }

    fn push_fee(&mut self, drops: u32) {
        if self.fee_history.len() >= FEE_HISTORY_LEN {
            self.fee_history.pop_front();
        }
        self.fee_history.push_back(drops as u64);
    }
}

impl Component for ServerPanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplServerInfo(info) => self.server_info = Some((**info).clone()),
            Action::XrplFee(fee) => {
                self.push_fee(fee.open_ledger_fee_drops);
                self.base_fee = Some(fee.open_ledger_fee_drops);
            }
            Action::XrplLedgerClose {
                base_fee,
                reserve_base,
                ..
            } => {
                self.push_fee(*base_fee);
                self.base_fee = Some(*base_fee);
                self.reserve_base = Some(*reserve_base);
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if self.server_info.is_none() {
            render_loading(
                frame,
                area,
                "Server",
                self.tick,
                "loading server info...",
                self.is_focused,
            );
            return Ok(());
        }
        let block = titled_block("Server", self.is_focused);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let sparkline_height = if self.fee_history.len() > 1 { 4 } else { 0 };
        let [info_area, spark_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(sparkline_height)])
                .areas(inner);

        let label = theme::dim_style();
        let info = self.server_info.as_ref();
        let ledger = info
            .map(|s| fmt::group_digits_u64(u64::from(s.ledger_index)))
            .unwrap_or_else(|| "-".to_string());
        let host = info
            .map(|s| s.hostid.clone())
            .unwrap_or_else(|| "-".to_string());
        let fee = self
            .base_fee
            .map(|v| fmt::fmt_drops(v as u64))
            .unwrap_or_else(|| "-".to_string());
        let reserve = self
            .reserve_base
            .map(|v| fmt::fmt_xrp(v as f64 / 1_000_000.0))
            .unwrap_or_else(|| "-".to_string());
        let lines = vec![
            Line::from(vec![
                Span::styled("URL:          ", label),
                Span::styled(self.server_url.clone(), theme::accent_style()),
            ]),
            Line::from(vec![
                Span::styled("Ledger:       ", label),
                Span::styled(ledger, theme::accent_style()),
                Span::styled("   HostID: ", label),
                Span::raw(host),
            ]),
            Line::from(vec![
                Span::styled("Open Fee:     ", label),
                Span::raw(format!("{fee} drops")),
                Span::styled("   Reserve Base: ", label),
                Span::raw(format!("{reserve} XRP")),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines), info_area);

        if sparkline_height > 0 {
            let fee_chart_data: Vec<(&str, u64)> =
                self.fee_history.iter().map(|&v| ("", v)).collect();
            let spark_block = Block::default()
                .title_style(theme::dim_style())
                .title(" open-ledger fee history (drops) ");
            let barchart = BarChart::default()
                .data(&fee_chart_data)
                .bar_width(2)
                .bar_gap(1)
                .bar_style(theme::accent_style())
                .value_style(
                    ratatui::style::Style::new()
                        .fg(ratatui::style::Color::Black)
                        .bg(theme::ACCENT),
                )
                .block(spark_block);
            frame.render_widget(barchart, spark_area);
        }

        Ok(())
    }
}
