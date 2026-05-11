use std::sync::Arc;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Gauge, Paragraph},
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
    config::Config,
    xrpl::AccountSummary,
};

/// Default XRPL reserve_increment if no ledger has been observed yet.
/// (Mainnet currently 0.2 XRP = 200_000 drops; the live ledgerClose value
/// always overrides this.)
const DEFAULT_RESERVE_INC_DROPS: u32 = 200_000;
const DEFAULT_RESERVE_BASE_DROPS: u32 = 1_000_000;

#[derive(Default)]
pub struct AccountPanel {
    account: Option<AccountSummary>,
    last_tx_hash: Option<String>,
    last_quality: Option<String>,
    last_price: Option<String>,
    reserve_base_drops: Option<u32>,
    reserve_inc_drops: Option<u32>,
    tick: usize,
    config: Option<Arc<Config>>,
    pub is_focused: bool,
}

impl AccountPanel {
    pub fn new() -> Self {
        Self {
            is_focused: false,
            ..Self::default()
        }
    }
}

impl Component for AccountPanel {
    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
        self.config = Some(config);
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplAccount(account) => self.account = Some((**account).clone()),
            Action::XrplAccountTx(tx) => self.last_tx_hash = Some(tx.hash.clone()),
            Action::XrplBookOffers(offers) => {
                if let Some(o) = offers.first() {
                    self.last_quality = Some(o.quality.clone());
                    self.last_price = Some(o.price.clone());
                }
            }
            Action::XrplLedgerClose {
                reserve_base,
                reserve_inc,
                ..
            } => {
                self.reserve_base_drops = Some(*reserve_base);
                self.reserve_inc_drops = Some(*reserve_inc);
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let Some(account) = &self.account else {
            render_loading(
                frame,
                area,
                "Account",
                self.tick,
                "loading account...",
                self.is_focused,
            );
            return Ok(());
        };

        let block = titled_block("Account", self.is_focused);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Two columns: identity (left) | balance / reserve (right)
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(inner);

        // ── Left: identity / activity ─────────────────────────────────────
        let label = theme::dim_style();
        let value = theme::accent_style();
        let currency = self
            .config
            .as_ref()
            .map(|c| c.xrpl.currency.clone())
            .unwrap_or_else(|| "CCY".to_string());
        let balance_xrp: f64 = account.balance_xrp.parse().unwrap_or(0.0);
        let est_value = if let Some(price) = self.last_price.as_ref() {
            if let Ok(p) = price.parse::<f64>() {
                format!("{:.3} {}", balance_xrp * p, currency)
            } else {
                "-".to_string()
            }
        } else {
            "-".to_string()
        };
        let id_lines = vec![
            Line::from(vec![
                Span::styled("Account:    ", label),
                Span::styled(account.account.clone(), value),
            ]),
            Line::from(vec![
                Span::styled("Sequence:   ", label),
                Span::raw(fmt::group_digits(&account.sequence.to_string())),
            ]),
            Line::from(vec![
                Span::styled("OwnerCount: ", label),
                Span::raw(account.owner_count.to_string()),
            ]),
            Line::from(vec![
                Span::styled("Last Tx:    ", label),
                Span::raw(self.last_tx_hash.clone().unwrap_or_else(|| "-".to_string())),
            ]),
            Line::from(vec![
                Span::styled("Top Price:  ", label),
                Span::raw(self.last_price.clone().unwrap_or_else(|| "-".to_string())),
            ]),
            Line::from(vec![
                Span::styled("Est. Value: ", label),
                Span::raw(est_value),
            ]),
        ];
        frame.render_widget(Paragraph::new(id_lines), left);

        // ── Right: balance + reserve gauge ────────────────────────────────
        let [balance_area, gauge_label_area, gauge_area, footnote_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(right);

        let reserve_base = self
            .reserve_base_drops
            .unwrap_or(DEFAULT_RESERVE_BASE_DROPS) as u64;
        let reserve_inc = self.reserve_inc_drops.unwrap_or(DEFAULT_RESERVE_INC_DROPS) as u64;
        let required_reserve_drops = reserve_base + (account.owner_count as u64) * reserve_inc;
        let required_reserve_xrp = required_reserve_drops as f64 / 1_000_000.0;
        let spendable_xrp = (balance_xrp - required_reserve_xrp).max(0.0);

        let balance_lines = vec![
            Line::from(vec![
                Span::styled("Balance:    ", label),
                Span::styled(
                    format!("{} XRP", fmt::fmt_xrp(balance_xrp)),
                    theme::success_style().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Spendable:  ", label),
                Span::styled(
                    format!("{} XRP", fmt::fmt_xrp(spendable_xrp)),
                    theme::accent_style(),
                ),
            ]),
            Line::from(vec![
                Span::styled("Reserve:    ", label),
                Span::raw(format!(
                    "{} XRP  ({} base + {} owners × {} XRP)",
                    fmt::fmt_xrp(required_reserve_xrp),
                    fmt::fmt_xrp(reserve_base as f64 / 1_000_000.0),
                    account.owner_count,
                    fmt::fmt_xrp(reserve_inc as f64 / 1_000_000.0),
                )),
            ]),
        ];
        frame.render_widget(Paragraph::new(balance_lines), balance_area);

        // Reserve usage gauge
        let usage_ratio: f64 = if balance_xrp > 0.0 {
            (required_reserve_xrp / balance_xrp).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let usage_pct = (usage_ratio * 100.0).round() as u16;
        let gauge_color = if usage_pct >= 90 {
            theme::ERROR
        } else if usage_pct >= 60 {
            theme::WARNING
        } else {
            theme::SUCCESS
        };
        frame.render_widget(
            Paragraph::new(Span::styled("Reserve usage", label)),
            gauge_label_area,
        );
        let gauge = Gauge::default()
            .gauge_style(Style::new().fg(gauge_color))
            .use_unicode(true)
            .ratio(usage_ratio)
            .label(format!("{usage_pct}%"));
        frame.render_widget(gauge, gauge_area);

        if self.reserve_base_drops.is_none() {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    "(reserve values are defaults until first ledger close)",
                    theme::dim_style().add_modifier(Modifier::ITALIC),
                )),
                footnote_area,
            );
        }

        Ok(())
    }
}
