use std::sync::Arc;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};

use crate::{
    action::Action,
    components::{
        Component,
        panels::ledger_objects::{LedgerObjectFilter, LedgerObjectsPanel},
    },
    config::Config,
};

/// Single tab: misc ledger objects (Check, Ticket, MPT, DID, …), Pay channels, Escrows (`account_objects`).
pub struct AccountObjectsTab {
    misc: LedgerObjectsPanel,
    pay: LedgerObjectsPanel,
    escrow: LedgerObjectsPanel,
    focus_index: usize,
}

impl AccountObjectsTab {
    pub fn new() -> Self {
        let mut misc = LedgerObjectsPanel::new("Ledger objects", LedgerObjectFilter::ObjectsTab);
        misc.is_focused = true;
        let pay = LedgerObjectsPanel::new("Pay channels", LedgerObjectFilter::PayChannelOnly);
        let escrow = LedgerObjectsPanel::new("Escrows", LedgerObjectFilter::EscrowOnly);
        Self {
            misc,
            pay,
            escrow,
            focus_index: 0,
        }
    }

    fn update_focus(&mut self) {
        self.misc.is_focused = self.focus_index == 0;
        self.pay.is_focused = self.focus_index == 1;
        self.escrow.is_focused = self.focus_index == 2;
    }
}

impl Component for AccountObjectsTab {
    fn register_action_handler(
        &mut self,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> color_eyre::Result<()> {
        self.misc.register_action_handler(action_tx.clone())?;
        self.pay.register_action_handler(action_tx.clone())?;
        self.escrow.register_action_handler(action_tx)?;
        Ok(())
    }

    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
        self.misc.register_config_handler(Arc::clone(&config))?;
        self.pay.register_config_handler(Arc::clone(&config))?;
        self.escrow.register_config_handler(config)?;
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::FocusNext => {
                self.focus_index = (self.focus_index + 1) % 3;
                self.update_focus();
            }
            Action::FocusPrev => {
                self.focus_index = (self.focus_index + 2) % 3;
                self.update_focus();
            }
            _ => {}
        }
        if let Some(a) = self.misc.update(action)? {
            return Ok(Some(a));
        }
        if let Some(a) = self.pay.update(action)? {
            return Ok(Some(a));
        }
        self.escrow.update(action)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let [top, bottom] =
            Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).areas(area);
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(bottom);
        self.misc.draw(frame, top)?;
        self.pay.draw(frame, left)?;
        self.escrow.draw(frame, right)?;
        Ok(())
    }
}
