use std::collections::VecDeque;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        BarChart, Block, BorderType, Cell, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, Wrap,
    },
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            fmt,
            selectable_table::SelectableTableState,
            theme,
            widgets::{centered_popup_rect, render_loading, titled_block, titled_block_with_count},
        },
    },
    xrpl::{DunlSummary, DunlValidatorRow, ServerInfoSummary, XrplTomlData},
};

const FEE_HISTORY_LEN: usize = 40;
const METRICS_LINES: u16 = 4;
const DUNL_FOOTER_LINES: u16 = 1;

#[derive(Default)]
struct ValidatorDetail {
    visible: bool,
    scroll: usize,
    lines: Vec<Line<'static>>,
    toml: Option<Result<XrplTomlData, String>>,
    toml_raw: Option<String>,
    status: u16,
    content_type: Option<String>,
}

impl ValidatorDetail {
    fn open(&mut self, row: &DunlValidatorRow, index: usize, dunl: &DunlSummary) {
        self.visible = true;
        self.scroll = 0;
        self.toml = None;
        self.toml_raw = None;
        self.status = 0;
        self.content_type = None;
        self.lines = validator_detail_lines(
            row,
            index,
            dunl,
            &self.toml,
            self.toml_raw.as_deref(),
            self.status,
            self.content_type.as_deref(),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn set_toml(
        &mut self,
        row: &DunlValidatorRow,
        index: usize,
        dunl: &DunlSummary,
        status: u16,
        content_type: Option<String>,
        raw: Option<String>,
        result: Result<XrplTomlData, String>,
    ) {
        self.toml = Some(result);
        self.toml_raw = raw;
        self.status = status;
        self.content_type = content_type;
        self.lines = validator_detail_lines(
            row,
            index,
            dunl,
            &self.toml,
            self.toml_raw.as_deref(),
            self.status,
            self.content_type.as_deref(),
        );
    }

    fn close(&mut self) {
        self.visible = false;
        self.scroll = 0;
        self.lines.clear();
        self.toml = None;
        self.toml_raw = None;
    }
}

fn validator_row_label(v: &DunlValidatorRow, max_chars: usize) -> String {
    if let Some(d) = &v.domain {
        fmt::truncate_middle(d, max_chars)
    } else if v.has_manifest {
        "(no domain)".to_string()
    } else {
        fmt::short_hex(&v.validation_public_key, 8, 6)
    }
}

fn validator_detail_lines(
    row: &DunlValidatorRow,
    index: usize,
    dunl: &DunlSummary,
    toml: &Option<Result<XrplTomlData, String>>,
    raw: Option<&str>,
    status: u16,
    content_type: Option<&str>,
) -> Vec<Line<'static>> {
    let label = theme::dim_style();
    let value = theme::accent_style();
    let ok = theme::success_style();
    let warn = theme::warning_style();

    let mut lines = vec![
        Line::from(vec![Span::styled(
            format!("Validator #{}", index + 1),
            theme::accent_style(),
        )]),
        Line::from(""),
    ];

    let push = |lines: &mut Vec<Line<'static>>, key: &str, val: String, style: Style| {
        lines.push(Line::from(vec![
            Span::styled(format!("{key:<16}"), label),
            Span::styled(val, style),
        ]));
    };

    push(
        &mut lines,
        "Domain",
        row.domain.clone().unwrap_or_else(|| "—".to_string()),
        if row.domain.is_some() { ok } else { warn },
    );
    push(
        &mut lines,
        "Manifest",
        if row.has_manifest {
            "present".to_string()
        } else {
            "missing".to_string()
        },
        if row.has_manifest { ok } else { warn },
    );
    push(
        &mut lines,
        "Sequence",
        row.sequence
            .map(|s| s.to_string())
            .unwrap_or_else(|| "—".to_string()),
        value,
    );
    push(
        &mut lines,
        "Signing key",
        row.validation_public_key.clone(),
        theme::secondary_style(),
    );
    push(
        &mut lines,
        "Master key",
        row.master_public_key
            .clone()
            .unwrap_or_else(|| "—".to_string()),
        theme::secondary_style(),
    );
    if row.master_differs_from_signing() {
        lines.push(Line::from(Span::styled(
            "  (master ≠ signing — rotated or dual-key setup)",
            warn,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("dUNL list", label),
        Span::raw(format!(
            " · seq {} · {} validators · exp {}",
            dunl.sequence, dunl.validator_count, dunl.expiration_utc
        )),
    ]));
    if let Some(days) = dunl.days_until_expiry() {
        let style = if days < 14 { warn } else { label };
        lines.push(Line::from(Span::styled(
            format!("  expires in {days} day(s)"),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled("xrp-ledger.toml", label)]));
    let status_style = if (200..300).contains(&status) {
        ok
    } else if status >= 400 {
        theme::error_style()
    } else {
        warn
    };
    lines.push(Line::from(vec![
        Span::styled("  HTTP status:", label),
        Span::styled(
            if status > 0 {
                status.to_string()
            } else {
                "—".to_string()
            },
            status_style,
        ),
    ]));
    if let Some(ct) = content_type {
        lines.push(Line::from(vec![
            Span::styled("  Content-Type:", label),
            Span::styled(ct.to_string(), value),
        ]));
    }
    match toml {
        None => {
            lines.push(Line::from(Span::styled(
                "  fetching...",
                theme::dim_style(),
            )));
        }
        Some(Ok(data)) => {
            let verified_style = if data.validator_found { ok } else { warn };
            lines.push(Line::from(vec![
                Span::styled("  Domain verified:", label),
                Span::styled(
                    if data.validator_found { "Yes" } else { "No" },
                    verified_style,
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  Validators listed:", label),
                Span::styled(data.validator_count.to_string(), value),
            ]));
            if let Some(att) = &data.attestation {
                lines.push(Line::from(vec![
                    Span::styled("  Attestation:", label),
                    Span::styled(att.clone(), theme::secondary_style()),
                ]));
            }
        }
        Some(Err(e)) => {
            lines.push(Line::from(Span::styled(
                format!("  Error: {e}"),
                theme::error_style(),
            )));
        }
    }

    if let Some(raw) = raw {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled("Raw TOML", label)]));
        for line_text in raw.lines() {
            lines.push(Line::from(Span::styled(
                line_text.to_string(),
                theme::dim_style(),
            )));
        }
    }

    lines
}

fn render_validator_detail(frame: &mut Frame, area: Rect, state: &mut ValidatorDetail) {
    if !state.visible {
        return;
    }

    let height = (state.lines.len() as u16 + 2).clamp(14, 24);
    let popup = centered_popup_rect(area, 44, height);

    frame.render_widget(Clear, popup);

    let title = state
        .lines
        .first()
        .and_then(|l| l.spans.first())
        .map(|s| s.content.to_string())
        .unwrap_or_else(|| "Validator".to_string());

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme::ACCENT))
        .title_style(Style::new().fg(theme::TITLE).add_modifier(Modifier::BOLD))
        .title(format!(" {title} "));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let line_count = state.lines.len();
    let paragraph = Paragraph::new(state.lines.clone())
        .wrap(Wrap { trim: true })
        .scroll((state.scroll as u16, 0));

    let [content_area, sb_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);

    frame.render_widget(paragraph, content_area);

    let content_height = content_area.height as usize;
    let max_scroll = line_count.saturating_sub(content_height);
    let mut sb_state = ScrollbarState::new(max_scroll).position(state.scroll);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(theme::dim_style())
            .thumb_style(theme::secondary_style()),
        sb_area,
        &mut sb_state,
    );
}

fn dunl_expiry_tag(dunl: &DunlSummary) -> String {
    dunl.days_until_expiry()
        .map(|d| {
            if d < 0 {
                "expired".to_string()
            } else if d < 14 {
                format!("{d}d left!")
            } else {
                format!("{d}d left")
            }
        })
        .unwrap_or_default()
}

fn quorum_match_tag(quorum: Option<u32>, dunl_count: u32) -> Option<&'static str> {
    quorum.map(|q| {
        if q == dunl_count {
            "matches dUNL"
        } else {
            "≠ dUNL size"
        }
    })
}

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
        let quorum = info
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
                Span::styled("URL:     ", label),
                Span::styled(
                    fmt::truncate_middle(&self.server_url, 48),
                    theme::accent_style(),
                ),
            ]),
            Line::from(vec![
                Span::styled("Ledger:  ", label),
                Span::styled(ledger, theme::accent_style()),
                Span::styled("  Host: ", label),
                Span::raw(host),
            ]),
            Line::from(vec![
                Span::styled("Fee:     ", label),
                Span::raw(format!("{fee} drops")),
                Span::styled("  Reserve: ", label),
                Span::raw(format!("{reserve} XRP")),
            ]),
            Line::from(vec![
                Span::styled("Quorum:  ", label),
                Span::raw(&quorum),
                Span::styled("  dUNL: ", label),
                Span::styled(
                    format!("{dunl_count} · seq {dunl_seq}"),
                    theme::accent_style(),
                ),
                Span::styled("  ", label),
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

        if let Some(vl) = info.and_then(|s| s.validator_list.as_ref()) {
            let match_note = quorum_match_tag(info.and_then(|s| s.validation_quorum), vl.count);
            let mut spans = vec![
                Span::styled("Node UNL:", label),
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

        if self.dunl.is_none() {
            render_loading(
                frame,
                list_area,
                "dUNL",
                self.tick,
                "loading validator list...",
                self.is_focused,
            );
        } else if let Some(d) = &self.dunl {
            let stats = d.stats();
            let exp_short = d
                .expiration_utc
                .split_whitespace()
                .take(2)
                .collect::<Vec<_>>()
                .join(" ");
            let dunl_title = format!("dUNL validators · seq {} · exp {exp_short}", d.sequence);
            let dunl_block = titled_block_with_count(
                &dunl_title,
                self.dunl_table.selected(),
                d.validators.len(),
                self.is_focused,
            );
            let dunl_inner = dunl_block.inner(list_area);
            frame.render_widget(dunl_block, list_area);

            let [footer_area, table_area] =
                Layout::vertical([Constraint::Length(DUNL_FOOTER_LINES), Constraint::Fill(1)])
                    .areas(dunl_inner);

            let footer = Line::from(vec![
                Span::styled(
                    format!(
                        "manifest {}/{} · domain {}/{} · master≠ {}",
                        stats.with_manifest,
                        stats.total,
                        stats.with_domain,
                        stats.total,
                        stats.master_distinct,
                    ),
                    theme::dim_style(),
                ),
                Span::styled("  │  ", theme::dim_style()),
                Span::styled("j/k scroll", theme::secondary_style()),
                Span::styled("  Enter detail", theme::secondary_style()),
            ]);
            frame.render_widget(Paragraph::new(footer), footer_area);

            if d.validators.is_empty() {
                frame.render_widget(
                    Paragraph::new("(no validators in blob)").style(theme::dim_style()),
                    table_area,
                );
            } else {
                let label_max = table_area.width.saturating_sub(28).clamp(14, 36) as usize;
                let rows = d.validators.iter().enumerate().map(|(i, v)| {
                    let name = validator_row_label(v, label_max);
                    let seq = v
                        .sequence
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "—".to_string());
                    let master_col = if v.master_differs_from_signing() {
                        "≠"
                    } else {
                        "·"
                    };
                    let name_style = if v.domain.is_some() {
                        theme::success_style()
                    } else if v.has_manifest {
                        theme::warning_style()
                    } else {
                        theme::dim_style()
                    };
                    let master_style = if v.master_differs_from_signing() {
                        theme::secondary_style()
                    } else {
                        theme::dim_style()
                    };
                    Row::new(vec![
                        Cell::from((i + 1).to_string()),
                        Cell::from(name).style(name_style),
                        Cell::from(seq),
                        Cell::from(master_col).style(master_style),
                        Cell::from(fmt::short_hex(&v.validation_public_key, 8, 6))
                            .style(theme::secondary_style()),
                    ])
                });
                let table = Table::new(
                    rows,
                    [
                        Constraint::Length(3),
                        Constraint::Fill(1),
                        Constraint::Length(4),
                        Constraint::Length(2),
                        Constraint::Length(18),
                    ],
                )
                .header(
                    Row::new(vec!["#", "Domain / key", "Seq", "M", "Signing"])
                        .style(theme::header_row_style()),
                )
                .row_highlight_style(theme::selected_row_style(self.is_focused))
                .highlight_symbol("▶ ");

                let [tbl_area, sb_area] =
                    Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)])
                        .areas(table_area);

                frame.render_stateful_widget(table, tbl_area, self.dunl_table.table_mut());
                frame.render_stateful_widget(
                    Scrollbar::new(ScrollbarOrientation::VerticalRight)
                        .style(theme::dim_style())
                        .thumb_style(theme::accent_style()),
                    sb_area,
                    self.dunl_table.scroll_mut(),
                );
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
