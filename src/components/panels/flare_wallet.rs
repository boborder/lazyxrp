use std::sync::Arc;

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
        shared::{
            fmt, theme,
            widgets::{render_loading, titled_block},
        },
    },
    config::Config,
    xrpl::FlareWalletSummary,
};

#[derive(Default)]
pub struct FlareWalletPanel {
    summary: Option<FlareWalletSummary>,
    wallet_address: Option<String>,
    tick: usize,
    pub is_focused: bool,
}

impl FlareWalletPanel {
    fn guidance_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(
                "Flare wallet not configured",
                theme::secondary_style(),
            )),
            Line::from(Span::styled(
                "Set [flare.wallet] address = \"0x…\"",
                theme::dim_style(),
            )),
            Line::from(Span::styled(
                "Read-only · native FLR + FXRP balances",
                theme::dim_style(),
            )),
        ]
    }

    pub(crate) fn render_content(&mut self, frame: &mut Frame, area: Rect) {
        if self.wallet_address.is_none() {
            frame.render_widget(Paragraph::new(self.guidance_lines()), area);
            return;
        }

        let Some(summary) = &self.summary else {
            render_loading(
                frame,
                area,
                "Flare",
                self.tick,
                "loading wallet balances…",
                self.is_focused,
            );
            return;
        };

        let label = theme::dim_style();
        let value = theme::accent_style();
        let execute = if summary.execute_enabled {
            Span::styled("on", theme::success_style())
        } else {
            Span::styled("off", theme::warning_style())
        };
        let key = if summary.executor_key_configured {
            Span::styled("set", theme::success_style())
        } else {
            Span::styled("unset", theme::warning_style())
        };

        let lines = vec![
            Line::from(vec![
                Span::styled("Addr ", label),
                Span::styled(fmt::truncate_middle(&summary.address, 24), value),
            ]),
            Line::from(vec![
                Span::styled("FLR ", label),
                Span::styled(summary.native_balance_display.clone(), value),
            ]),
            Line::from(vec![
                Span::styled("FXRP ", label),
                Span::styled(summary.fxrp_balance_display.clone(), value),
            ]),
            Line::from(vec![
                Span::styled("execute ", label),
                execute,
                Span::raw("  "),
                Span::styled("key ", label),
                key,
            ]),
            Line::from(Span::styled("Read-only", theme::dim_style())),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }
}

impl Component for FlareWalletPanel {
    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
        self.wallet_address = config.flare.wallet.address.clone();
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::FlareWalletBalance(summary) => {
                self.summary = Some((**summary).clone());
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let block = titled_block("Flare Wallet", self.is_focused);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        self.render_content(frame, inner);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_panel(panel: &mut FlareWalletPanel) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).unwrap();
        terminal
            .draw(|frame| panel.draw(frame, frame.area()).unwrap())
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// TC-121: unconfigured wallet shows setup guidance.
    #[test]
    fn flare_wallet_guidance_when_unconfigured() {
        let mut panel = FlareWalletPanel::default();
        let out = render_panel(&mut panel);
        assert!(out.contains("not configured"));
        assert!(out.contains("[flare.wallet]"));
    }

    /// TC-122: configured wallet shows balance rows after poll action.
    #[test]
    fn flare_wallet_shows_balances_when_configured() {
        let mut panel = FlareWalletPanel {
            wallet_address: Some("0xabcdef0123456789abcdef0123456789abcdef01".into()),
            ..Default::default()
        };
        panel
            .update(&Action::FlareWalletBalance(Box::new(FlareWalletSummary {
                address: "0xabcdef0123456789abcdef0123456789abcdef01".into(),
                native_balance_display: "12.5000".into(),
                fxrp_balance_display: "100.000000".into(),
                execute_enabled: true,
                executor_key_configured: false,
            })))
            .unwrap();
        let out = render_panel(&mut panel);
        assert!(out.contains("12.5000"));
        assert!(out.contains("100.000000"));
        assert!(out.contains("execute"));
    }
}
