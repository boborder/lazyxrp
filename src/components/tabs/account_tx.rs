use std::sync::Arc;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};

use crate::{
    action::Action,
    components::{
        Component,
        panels::{account::AccountPanel, tx_history::TxHistoryPanel},
    },
    config::Config,
};

/// Combined panel showing Account info (top) and Tx History (bottom).
pub struct AccountTxTab {
    account: AccountPanel,
    tx: TxHistoryPanel,
    focus_index: usize,
}

impl AccountTxTab {
    pub fn new() -> Self {
        let mut account = AccountPanel::new();
        account.is_focused = true;
        Self {
            account,
            tx: TxHistoryPanel::new(),
            focus_index: 0,
        }
    }

    fn update_focus(&mut self) {
        self.account.is_focused = self.focus_index == 0;
        self.tx.is_focused = self.focus_index == 1;
    }
}

impl Component for AccountTxTab {
    fn register_action_handler(
        &mut self,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> color_eyre::Result<()> {
        self.account.register_action_handler(action_tx.clone())?;
        self.tx.register_action_handler(action_tx)?;
        Ok(())
    }

    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
        self.account.register_config_handler(Arc::clone(&config))?;
        self.tx.register_config_handler(config)?;
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
        if let Some(a) = self.account.update(action)? {
            return Ok(Some(a));
        }
        self.tx.update(action)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let [top, bottom] =
            Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(area);
        self.account.draw(frame, top)?;
        self.tx.draw(frame, bottom)?;
        Ok(())
    }
}
