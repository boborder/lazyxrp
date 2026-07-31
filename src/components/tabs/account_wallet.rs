use std::sync::Arc;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};

use crate::{
    action::Action,
    components::{
        Component,
        panels::{account::AccountPanel, tx_history::TxHistoryPanel, wallet::WalletPanel},
    },
    config::Config,
    tui::Event,
};

pub struct AccountWalletTab {
    wallet: WalletPanel,
    account: AccountPanel,
    tx: TxHistoryPanel,
    focus_index: usize,
    has_wallet: bool,
}

impl AccountWalletTab {
    pub fn new(skip_mainnet_prompt: bool) -> Self {
        let mut account = AccountPanel::new();
        account.is_focused = true;
        Self {
            wallet: WalletPanel::new(skip_mainnet_prompt),
            account,
            tx: TxHistoryPanel::new(),
            focus_index: 0,
            has_wallet: false,
        }
    }

    fn focus_len(&self) -> usize {
        2
    }

    fn update_focus(&mut self) {
        // 固定2ペイン: 上( wallet or account ) / 下( tx )
        self.tx.is_focused = self.focus_index == 1;
        if self.has_wallet {
            self.wallet.is_focused = self.focus_index == 0;
            self.account.is_focused = false;
        } else {
            self.wallet.is_focused = false;
            self.account.is_focused = self.focus_index == 0;
        }
    }
}

impl Component for AccountWalletTab {
    fn register_action_handler(
        &mut self,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> color_eyre::Result<()> {
        self.wallet.register_action_handler(action_tx.clone())?;
        self.account.register_action_handler(action_tx.clone())?;
        self.tx.register_action_handler(action_tx)?;
        Ok(())
    }

    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
        self.has_wallet = config.xrpl.signing.secret_seed.is_some();
        self.wallet.register_config_handler(Arc::clone(&config))?;
        self.account.register_config_handler(Arc::clone(&config))?;
        self.tx.register_config_handler(config)?;
        self.focus_index = 0;
        self.update_focus();
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::FocusNext => {
                self.focus_index = (self.focus_index + 1) % self.focus_len();
                self.update_focus();
            }
            Action::FocusPrev => {
                self.focus_index = (self.focus_index + self.focus_len() - 1) % self.focus_len();
                self.update_focus();
            }
            _ => {}
        }

        if let Some(a) = self.wallet.update(action)? {
            return Ok(Some(a));
        }
        if !self.has_wallet
            && let Some(a) = self.account.update(action)?
        {
            return Ok(Some(a));
        }
        self.tx.update(action)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let [top, bottom] =
            Layout::vertical([Constraint::Percentage(46), Constraint::Percentage(54)]).areas(area);

        if self.has_wallet {
            self.wallet.draw(frame, top)?;
        } else {
            self.account.draw(frame, top)?;
        }

        self.tx.draw(frame, bottom)?;
        Ok(())
    }

    fn on_event(&mut self, event: Option<&Event>) -> color_eyre::Result<Option<Action>> {
        match event {
            Some(Event::Key(key)) if self.has_wallet && self.wallet.is_focused => {
                self.wallet.on_key_event(*key)
            }
            _ => Ok(None),
        }
    }
}
