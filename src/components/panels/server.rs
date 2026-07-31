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
            fmt,
            selectable_table::SelectableTableState,
            theme,
            widgets::{render_loading, titled_block},
        },
    },
    xrpl::{DunlSummary, ServerInfoSummary},
};

const FEE_HISTORY_LEN: usize = 40;
const METRICS_LINES: u16 = 4;
#[path = "server_detail.rs"]
mod detail;
#[path = "server_dunl.rs"]
mod dunl;
#[path = "server_metrics.rs"]
mod metrics;

use detail::{ValidatorDetail, render_validator_detail};
use metrics::{dunl_expiry_tag, quorum_match_tag};

#[derive(Default)]
pub struct ServerPanel {
    server_url: String,
    server_info: Option<ServerInfoSummary>,
    dunl: Option<DunlSummary>,
    dunl_table: SelectableTableState,
    base_fee: Option<u32>,
    reserve_base: Option<u32>,
    fee_history: VecDeque<u64>,
    tick: usize,
    pub is_focused: bool,
    detail: ValidatorDetail,
    action_tx: Option<tokio::sync::mpsc::UnboundedSender<Action>>,
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

    fn dunl_len(&self) -> usize {
        self.dunl.as_ref().map(|d| d.validators.len()).unwrap_or(0)
    }
}

impl Component for ServerPanel {
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
                Action::XrplTomlFetched {
                    status,
                    content_type,
                    raw,
                    result,
                    ..
                } => {
                    if let Some(idx) = self.dunl_table.selected()
                        && let (Some(d), Some(row)) = (
                            self.dunl.as_ref(),
                            self.dunl.as_ref().and_then(|d| d.validators.get(idx)),
                        )
                    {
                        self.detail.set_toml(
                            row,
                            idx,
                            d,
                            *status,
                            content_type.clone(),
                            raw.clone(),
                            result.clone(),
                        );
                    }
                    return Ok(None);
                }
                _ => return Ok(None),
            }
        }
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplServerInfo(info) => self.server_info = Some((**info).clone()),
            Action::XrplDunl(dunl) => {
                self.dunl_table.reset_len(dunl.validators.len());
                self.dunl = Some(dunl.clone());
            }
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
            Action::SelectNext if self.is_focused && self.dunl_len() > 0 => {
                self.dunl_table.select_next(self.dunl_len());
            }
            Action::SelectPrev if self.is_focused && self.dunl_len() > 0 => {
                self.dunl_table.select_prev(self.dunl_len());
            }
            Action::TxDetailToggle if self.is_focused && self.dunl_len() > 0 => {
                if let Some(idx) = self.dunl_table.selected()
                    && let (Some(d), Some(row)) = (
                        self.dunl.as_ref(),
                        self.dunl.as_ref().and_then(|d| d.validators.get(idx)),
                    )
                {
                    self.detail.open(row, idx, d);
                    if let (Some(domain), Some(tx)) =
                        (row.domain.as_deref(), self.action_tx.as_ref())
                    {
                        let expected_pubkey = row
                            .master_public_key
                            .as_deref()
                            .unwrap_or(&row.validation_public_key)
                            .to_string();
                        let _ = tx.send(Action::RequestXrplToml {
                            domain: domain.to_string(),
                            expected_pubkey,
                        });
                    }
                }
            }
            Action::XrplTomlFetched {
                status,
                content_type,
                raw,
                result,
                ..
            } if self.detail.visible => {
                if let Some(idx) = self.dunl_table.selected()
                    && let (Some(d), Some(row)) = (
                        self.dunl.as_ref(),
                        self.dunl.as_ref().and_then(|d| d.validators.get(idx)),
                    )
                {
                    self.detail.set_toml(
                        row,
                        idx,
                        d,
                        *status,
                        content_type.clone(),
                        raw.clone(),
                        result.clone(),
                    );
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn register_action_handler(
        &mut self,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> color_eyre::Result<()> {
        self.action_tx = Some(action_tx);
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if self.detail.visible {
            render_validator_detail(frame, area, &mut self.detail);
            return Ok(());
        }

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
        let node_unl_line = self
            .server_info
            .as_ref()
            .and_then(|s| s.validator_list.as_ref())
            .is_some();
        let metrics_lines = METRICS_LINES + u16::from(node_unl_line);

        let [metrics_area, list_area, spark_area] = Layout::vertical([
            Constraint::Length(metrics_lines),
            Constraint::Fill(1),
            Constraint::Length(sparkline_height),
        ])
        .areas(inner);

        let label_style = theme::dim_style();
        let server_info = self.server_info.as_ref();
        let ledger = server_info
            .map(|s| fmt::group_digits_u64(u64::from(s.ledger_index)))
            .unwrap_or_else(|| "-".to_string());
        let host = server_info
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
        let quorum = server_info
            .and_then(|s| s.validation_quorum)
            .map(|q| q.to_string())
            .unwrap_or_else(|| "-".to_string());

        let dunl_count = self.dunl.as_ref().map(|d| d.validator_count).unwrap_or(0);
        let dunl_seq = self
            .dunl
            .as_ref()
            .map(|d| d.sequence.to_string())
            .unwrap_or_else(|| "-".to_string());
        let dunl_exp = self.dunl.as_ref().map(dunl_expiry_tag).unwrap_or_default();

        let mut lines = vec![
            Line::from(vec![
                Span::styled("URL:     ", label_style),
                Span::styled(
                    fmt::truncate_middle(&self.server_url, 48),
                    theme::accent_style(),
                ),
            ]),
            Line::from(vec![
                Span::styled("Ledger:  ", label_style),
                Span::styled(ledger, theme::accent_style()),
                Span::styled("  Host: ", label_style),
                Span::raw(host),
            ]),
            Line::from(vec![
                Span::styled("Fee:     ", label_style),
                Span::raw(format!("{fee} drops")),
                Span::styled("  Reserve: ", label_style),
                Span::raw(format!("{reserve} XRP")),
            ]),
            Line::from(vec![
                Span::styled("Quorum:  ", label_style),
                Span::raw(&quorum),
                Span::styled("  dUNL: ", label_style),
                Span::styled(
                    format!("{dunl_count} · seq {dunl_seq}"),
                    theme::accent_style(),
                ),
                Span::styled("  ", label_style),
                Span::styled(
                    dunl_exp.clone(),
                    if dunl_exp.contains('!') {
                        theme::warning_style()
                    } else {
                        theme::dim_style()
                    },
                ),
            ]),
        ];

        if let Some(vl) = server_info.and_then(|s| s.validator_list.as_ref()) {
            let match_note =
                quorum_match_tag(server_info.and_then(|s| s.validation_quorum), vl.count);
            let mut spans = vec![
                Span::styled("Node UNL:", label_style),
                Span::raw(" "),
                Span::styled(
                    format!("{} · {} val", vl.status, vl.count),
                    theme::accent_style(),
                ),
                Span::styled(format!(" · exp {}", vl.expiration), theme::dim_style()),
            ];
            if let Some(note) = match_note {
                let style = if note == "matches dUNL" {
                    theme::success_style()
                } else {
                    theme::warning_style()
                };
                spans.push(Span::raw(" · "));
                spans.push(Span::styled(note, style));
            }
            lines.push(Line::from(spans));
        }
        frame.render_widget(Paragraph::new(lines), metrics_area);

        match self.dunl.as_ref() {
            None => dunl::draw_dunl_loading(frame, list_area, self.tick, self.is_focused),
            Some(d) => {
                dunl::draw_dunl_panel(frame, list_area, d, &mut self.dunl_table, self.is_focused)
            }
        }

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
