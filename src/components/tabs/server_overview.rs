use std::sync::Arc;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};

use crate::{
    action::Action,
    components::{
        Component,
        panels::{server::ServerPanel, wallet::WalletPanel},
    },
    config::Config,
    tui::Event,
};

/// Combined panel showing Server info (top) and Wallet overview (bottom).
pub struct ServerOverviewTab {
    server: ServerPanel,
    wallet: WalletPanel,
    focus_index: usize,
}

impl ServerOverviewTab {
    pub fn new(server_url: String, skip_mainnet_prompt: bool) -> Self {
        let mut server = ServerPanel::new(server_url);
        server.is_focused = true;
        Self {
            server,
            wallet: WalletPanel::new(skip_mainnet_prompt),
            focus_index: 0,
        }
    }

    fn update_focus(&mut self) {
        self.server.is_focused = self.focus_index == 0;
        self.wallet.is_focused = self.focus_index == 1;
    }
}

impl Component for ServerOverviewTab {
    fn register_action_handler(
        &mut self,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> color_eyre::Result<()> {
        self.server.register_action_handler(action_tx.clone())?;
        self.wallet.register_action_handler(action_tx)?;
        Ok(())
    }

    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
        self.server.register_config_handler(Arc::clone(&config))?;
        self.wallet.register_config_handler(config)?;
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::FocusNext | Action::FocusPrev => {
                self.focus_index = 1 - self.focus_index;
                self.update_focus();
            }
            _ => {}
        }
        if let Some(a) = self.server.update(action)? {
            return Ok(Some(a));
        }
        self.wallet.update(action)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let [top, bottom] =
            Layout::vertical([Constraint::Percentage(40), Constraint::Percentage(60)]).areas(area);
        self.server.draw(frame, top)?;
        self.wallet.draw(frame, bottom)?;
        Ok(())
    }

    fn handle_events(&mut self, event: Option<&Event>) -> color_eyre::Result<Option<Action>> {
        match event {
            Some(Event::Key(key)) if self.focus_index == 1 => self.wallet.handle_key_event(*key),
            _ => Ok(None),
        }
    }
}
