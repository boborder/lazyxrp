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
    xrpl::{FxrpDirectMintInfo, SmartAccountInfo, sanitize_text},
};

#[derive(Default)]
pub struct FxrpDirectMintPanel {
    info: Option<FxrpDirectMintInfo>,
    smart: Option<SmartAccountInfo>,
    tick: usize,
    pub is_focused: bool,
}

impl FxrpDirectMintPanel {
    pub(crate) fn compact_summary(&self) -> String {
        match &self.info {
            None => "loading…".to_string(),
            Some(info) => format!(
                "fee {} · min {} XRP",
                bips_to_percent_display(info.fee_bips),
                uba_to_xrp_display(info.min_fee_uba),
            ),
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
    vec![
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
    ]
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
        };
        panel
            .update(&Action::SmartAccountInfo(Box::new(info.clone())))
            .unwrap();
        let out = render(&mut panel);
        assert!(out.contains("0x12345"), "{out}");
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
}
