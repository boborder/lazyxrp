use std::sync::Arc;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};

use crate::{
    action::Action,
    components::{
        Component,
        panels::{
            combined_oracle::CombinedOraclePanel, flare_wallet::FlareWalletPanel,
            server::ServerPanel,
        },
    },
    config::{Config, FlareDisplay},
};

/// Dashboard tab: server (left), Oracle/FTSO/FXRP combined (right).
pub struct OverviewTab {
    server: ServerPanel,
    combined: CombinedOraclePanel,
    flare_wallet: FlareWalletPanel,
    focus_index: usize,
    flare_display: FlareDisplay,
}

impl OverviewTab {
    pub fn new(server_url: String, flare_display: FlareDisplay) -> Self {
        let mut server = ServerPanel::new(server_url);
        server.is_focused = true;
        let combined = CombinedOraclePanel::new(flare_display);
        Self {
            server,
            combined,
            flare_wallet: FlareWalletPanel::default(),
            focus_index: 0,
            flare_display,
        }
    }

    fn max_focus(&self) -> usize {
        if self.flare_display == FlareDisplay::Off {
            1
        } else {
            2
        }
    }

    fn update_focus(&mut self) {
        self.server.is_focused = self.focus_index == 0;
        self.combined.is_focused = self.flare_display != FlareDisplay::Off && self.focus_index == 1;
    }
}

impl Component for OverviewTab {
    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
        self.flare_wallet.register_config_handler(config)?;
        Ok(())
    }

    fn register_action_handler(
        &mut self,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> color_eyre::Result<()> {
        self.server.register_action_handler(action_tx.clone())?;
        self.combined.register_action_handler(action_tx.clone())?;
        self.flare_wallet.register_action_handler(action_tx)?;
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::FocusNext if self.focus_index + 1 < self.max_focus() => {
                self.focus_index += 1;
                self.update_focus();
            }
            Action::FocusPrev if self.focus_index > 0 => {
                self.focus_index -= 1;
                self.update_focus();
            }
            _ => {}
        }

        if let Some(a) = self.server.update(action)? {
            return Ok(Some(a));
        }
        if let Some(a) = self.flare_wallet.update(action)? {
            return Ok(Some(a));
        }
        self.combined.update(action)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if self.flare_display == FlareDisplay::Off {
            self.server.draw(frame, area)?;
            return Ok(());
        }
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(44), Constraint::Percentage(56)])
                .areas(area);

        self.server.draw(frame, left)?;
        let [combined_area, wallet_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(7)]).areas(right);
        self.combined.draw(frame, combined_area)?;
        self.flare_wallet.draw(frame, wallet_area)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_tab(tab: &mut OverviewTab) -> String {
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal
            .draw(|frame| tab.draw(frame, frame.area()).unwrap())
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// TC-116: Overview Off omits the combined Flare panel.
    #[test]
    fn overview_off_omits_combined_flare_panel() {
        let mut tab = OverviewTab::new("wss://example".into(), FlareDisplay::Off);
        let out = render_tab(&mut tab);
        assert!(!out.contains("Oracle / FTSO / FXRP"));
        assert!(!out.contains("FTSO"));
    }

    /// TC-117: Overview Compact keeps split but uses one-line FTSO/FXRP summaries.
    #[test]
    fn overview_compact_uses_one_line_flare_summaries() {
        let mut tab = OverviewTab::new("wss://example".into(), FlareDisplay::Compact);
        tab.update(&Action::FlareOraclePrices(vec![
            crate::xrpl::FlareFeedPrice {
                pair: "XRP/USD".into(),
                price: "0.52".into(),
                timestamp: 1,
                source: "test".into(),
            },
        ]))
        .unwrap();
        let out = render_tab(&mut tab);
        assert!(out.contains("Oracle / FTSO / FXRP"));
        assert!(out.contains("XRP/USD 0.52"));
        assert!(!out.contains("Pair"));
    }
}
