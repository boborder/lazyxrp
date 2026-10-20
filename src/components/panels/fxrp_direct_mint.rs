use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{fmt, theme, widgets::render_loading},
    },
    flare::{bips_to_percent_display, format_token_units, uba_to_xrp_display},
    xrpl::{FxrpDirectMintInfo, MintLimits, SmartAccountInfo, sanitize_text},
};

#[derive(Default)]
pub struct FxrpDirectMintPanel {
    info: Option<FxrpDirectMintInfo>,
    smart: Option<SmartAccountInfo>,
    tick: usize,
    pub is_focused: bool,
}

/// `Some(label)` when FXRP minting is stopped on Flare (governance mint pause or emergency level).
fn pause_label(info: &FxrpDirectMintInfo) -> Option<String> {
    let level = info.emergency_pause_level.unwrap_or(0);
    if info.minting_paused == Some(true) {
        Some("MINT PAUSED".to_string())
    } else if level > 0 {
        Some(format!("EMERGENCY PAUSE L{level}"))
    } else {
        None
    }
}

/// Largest single mint that is neither delayed (window / large threshold) nor over the cap.
fn limit_now_display(l: &MintLimits) -> String {
    let large_below = l.large_threshold_uba.saturating_sub(l.granularity_uba);
    let mut max = l.window_max_now_uba.min(large_below);
    if let Some(h) = l.cap_headroom_uba {
        max = max.min(h);
    }
    uba_to_xrp_display(max)
}

/// Share of the FXRP minting cap used by the supply (`None` when the cap is disabled).
fn cap_used_percent(l: &MintLimits) -> Option<u128> {
    (l.cap_uba > 0).then(|| l.supply_uba.saturating_mul(100) / l.cap_uba)
}

impl FxrpDirectMintPanel {
    pub(crate) fn compact_summary(&self) -> String {
        match &self.info {
            None => "loading…".to_string(),
            Some(info) => {
                let base = format!(
                    "fee {} · min {} XRP",
                    bips_to_percent_display(info.fee_bips),
                    uba_to_xrp_display(info.min_fee_uba),
                );
                match pause_label(info) {
                    Some(p) => format!("{p} · {base}"),
                    None => base,
                }
            }
        }
    }

    pub(crate) fn render_compact(&self, frame: &mut Frame, area: Rect) {
        let line = Line::from(vec![
            Span::styled("FXRP ", theme::dim_style()),
            Span::styled(self.compact_summary(), theme::secondary_style()),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    pub(crate) fn render_content(&mut self, frame: &mut Frame, area: Rect) {
        let Some(info) = &self.info else {
            render_loading(
                frame,
                area,
                "FXRP",
                self.tick,
                "loading AssetManager…",
                self.is_focused,
            );
            return;
        };

        let label = theme::dim_style();
        let value = theme::accent_style();
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Vault ", label),
                Span::styled(fmt::truncate_middle(&info.core_vault_xrpl, 28), value),
            ]),
            Line::from(vec![
                Span::styled("Min fee ", label),
                Span::styled(
                    format!("{} XRP", uba_to_xrp_display(info.min_fee_uba)),
                    value,
                ),
            ]),
            Line::from(vec![
                Span::styled("Mint fee ", label),
                Span::styled(bips_to_percent_display(info.fee_bips), value),
            ]),
            Line::from(vec![
                Span::styled("Exec fee ", label),
                Span::styled(
                    format!("{} XRP", uba_to_xrp_display(info.executor_fee_uba)),
                    value,
                ),
            ]),
        ];
        if let Some(l) = &info.limits {
            lines.push(Line::from(vec![
                Span::styled("Max now ", label),
                Span::styled(format!("{} XRP (est.)", limit_now_display(l)), value),
            ]));
            if let Some(pct) = cap_used_percent(l) {
                lines.push(Line::from(vec![
                    Span::styled("Cap used ", label),
                    Span::styled(format!("{pct}% (est.)"), value),
                ]));
            }
        }
        if let Some(p) = pause_label(info) {
            lines.insert(0, Line::from(Span::styled(p, theme::error_style())));
        }
        if let Some(smart) = &self.smart {
            lines.extend(smart_account_lines(smart));
        }
        lines.extend([
            Line::from(vec![
                Span::styled("AM ", label),
                Span::styled(
                    fmt::truncate_middle(&info.asset_manager, 22),
                    theme::dim_style(),
                ),
            ]),
            Line::from(Span::styled(
                "Direct Mint · read-only",
                theme::secondary_style(),
            )),
        ]);
        frame.render_widget(Paragraph::new(lines), area);
    }
}

/// Smart Account rows (personal account, FXRP balance, nonce, operator, instruction fees).
/// Remote strings are sanitized (a hostile RPC must not inject escape sequences).
fn smart_account_lines(smart: &SmartAccountInfo) -> Vec<Line<'static>> {
    let label = theme::dim_style();
    let value = theme::accent_style();
    let operator = smart
        .operator_wallets
        .first()
        .map_or_else(|| "(none)".to_string(), |o| sanitize_text(o, 64));
    let row = |name: &'static str, text: String| {
        Line::from(vec![Span::styled(name, label), Span::styled(text, value)])
    };
    let mut lines = vec![
        row(
            "Smart acct ",
            fmt::truncate_middle(&sanitize_text(&smart.personal_account, 64), 16),
        ),
        row(
            "SA FXRP ",
            format_token_units(smart.fxrp_balance_units, smart.fxrp_decimals),
        ),
        row(
            "Nonce ",
            format!(
                "{} · {}",
                smart.nonce,
                if smart.deployed {
                    "deployed"
                } else {
                    "not deployed"
                }
            ),
        ),
        row("Operator ", fmt::truncate_middle(&operator, 22)),
        row(
            "SA fee ",
            format!(
                "xfer {} · redeem {}",
                uba_to_xrp_display(u128::from(smart.transfer_fee_drops)),
                uba_to_xrp_display(u128::from(smart.redeem_fee_drops)),
            ),
        ),
    ];
    for v in &smart.vaults {
        let kind = match v.kind {
            1 => "Firelight".to_string(),
            2 => "Upshift".to_string(),
            n => format!("type {n}"),
        };
        let held = v.assets.map_or_else(
            || format!("{} shares", v.shares),
            |a| format!("{} FXRP", format_token_units(a, smart.fxrp_decimals)),
        );
        lines.push(Line::from(vec![
            Span::styled(format!("Vault {} ", v.id), label),
            Span::styled(format!("{kind} {held}"), value),
        ]));
    }
    lines
}

impl Component for FxrpDirectMintPanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::FxrpDirectMintInfo(info) => {
                self.info = Some((**info).clone());
            }
            Action::SmartAccountInfo(info) => {
                self.smart = Some((**info).clone());
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let block = theme::panel_block("FXRP Direct Mint", self.is_focused);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        self.render_content(frame, inner);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(panel: &mut FxrpDirectMintPanel) -> String {
        crate::test_support::render_to_string(48, 14, |f| panel.draw(f, f.area()).unwrap())
    }

    /// TC-245: `SmartAccountInfo` renders its values (personal account, FXRP balance, nonce,
    /// deployment, operator, both fees) and strips control characters from remote strings.
    #[test]
    fn smart_account_rows_render_values_and_sanitize_remote_text() {
        let mut panel = FxrpDirectMintPanel::default();
        panel
            .update(&Action::FxrpDirectMintInfo(Box::new(FxrpDirectMintInfo {
                core_vault_xrpl: "rVault".into(),
                asset_manager: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                min_fee_uba: 100_000,
                fee_bips: 10,
                executor_fee_uba: 200_000,
                minting_paused: None,
                emergency_pause_level: None,
                limits: None,
            })))
            .unwrap();
        let before = render(&mut panel);
        assert!(!before.contains("Nonce"), "{before}");

        let mut info = SmartAccountInfo {
            xrpl_owner: "rOwner".into(),
            controller: "0xcccccccccccccccccccccccccccccccccccccccc".into(),
            personal_account: "0x1234567890abcdef1234567890abcdef12345678".into(),
            deployed: false,
            nonce: 7,
            operator_wallets: vec!["rOper\u{1b}[31matorX".into()],
            transfer_fee_drops: 200_000,
            redeem_fee_drops: 1_500_000,
            fxrp_decimals: 6,
            fxrp_balance_units: 12_500_000,
            vaults: vec![crate::xrpl::SmartAccountVault {
                id: 2,
                address: "0xvault".into(),
                kind: 2,
                shares: 5_000_000,
                assets: Some(5_250_000),
            }],
        };
        panel
            .update(&Action::SmartAccountInfo(Box::new(info.clone())))
            .unwrap();
        let out = render(&mut panel);
        assert!(out.contains("0x12345"), "{out}");
        assert!(out.contains("Vault 2 Upshift 5.25 FXRP"), "{out}");
        assert!(out.contains("2345678"), "{out}");
        assert!(out.contains("12.5"), "{out}");
        assert!(out.contains("7 · not deployed"), "{out}");
        assert!(out.contains("rOper[31matorX"), "{out}");
        assert!(!out.contains('\u{1b}'), "{out}");
        assert!(out.contains("xfer 0.2 · redeem 1.5"), "{out}");

        info.deployed = true;
        panel
            .update(&Action::SmartAccountInfo(Box::new(info)))
            .unwrap();
        assert!(render(&mut panel).contains("7 · deployed"));
    }

    /// TC-332: a paused FXRP (governance mint pause or emergency level) is visible in both the
    /// full and the compact panel; unknown or clear states show nothing.
    #[test]
    fn pause_state_is_rendered() {
        let info = |paused, level| FxrpDirectMintInfo {
            core_vault_xrpl: "rVault".into(),
            asset_manager: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            min_fee_uba: 100_000,
            fee_bips: 10,
            executor_fee_uba: 200_000,
            minting_paused: paused,
            emergency_pause_level: level,
            limits: None,
        };
        let mut panel = FxrpDirectMintPanel::default();
        panel
            .update(&Action::FxrpDirectMintInfo(Box::new(info(
                Some(true),
                Some(0),
            ))))
            .unwrap();
        assert!(render(&mut panel).contains("MINT PAUSED"));
        assert!(panel.compact_summary().starts_with("MINT PAUSED"));

        panel
            .update(&Action::FxrpDirectMintInfo(Box::new(info(
                Some(false),
                Some(2),
            ))))
            .unwrap();
        assert!(render(&mut panel).contains("EMERGENCY PAUSE L2"));

        panel
            .update(&Action::FxrpDirectMintInfo(Box::new(info(None, None))))
            .unwrap();
        assert!(!render(&mut panel).contains("PAUSE"));
        assert!(!panel.compact_summary().contains("PAUSE"));
    }

    /// TC-336: the limit rows show the largest immediate mint (min of window, large threshold
    /// and cap headroom) and the cap usage; no limits → no rows.
    #[test]
    fn limit_rows_render() {
        let mut info = FxrpDirectMintInfo {
            core_vault_xrpl: "rVault".into(),
            asset_manager: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            min_fee_uba: 100_000,
            fee_bips: 10,
            executor_fee_uba: 200_000,
            minting_paused: None,
            emergency_pause_level: None,
            limits: None,
        };
        let mut panel = FxrpDirectMintPanel::default();
        panel
            .update(&Action::FxrpDirectMintInfo(Box::new(info.clone())))
            .unwrap();
        assert!(!render(&mut panel).contains("Max now"));

        info.limits = Some(MintLimits {
            cap_uba: 1_000_000_000,
            supply_uba: 250_000_000,
            cap_headroom_uba: Some(750_000_000),
            window_max_now_uba: 60_000_000,
            large_threshold_uba: 4_000_000_000,
            large_delay_secs: 7200,
            granularity_uba: 1,
        });
        panel
            .update(&Action::FxrpDirectMintInfo(Box::new(info)))
            .unwrap();
        let out = render(&mut panel);
        assert!(out.contains("Max now 60 XRP"), "{out}");
        assert!(out.contains("Cap used 25%"), "{out}");
    }
}
