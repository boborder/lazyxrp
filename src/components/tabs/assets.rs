use std::sync::Arc;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect, Size},
};

use crate::{
    action::Action,
    components::{
        Component,
        panels::ledger_objects::{LedgerObjectFilter, LedgerObjectsPanel},
        tabs::nft::NftTab,
    },
    config::Config,
};

pub struct AssetsTab {
    nft: NftTab,
    objects: LedgerObjectsPanel,
    pay: LedgerObjectsPanel,
    escrow: LedgerObjectsPanel,
    focus_index: usize,
}

impl AssetsTab {
    pub fn new() -> Self {
        let mut nft = NftTab::new();
        nft.is_focused = true;
        Self {
            nft,
            objects: LedgerObjectsPanel::new("Objects", LedgerObjectFilter::ObjectsTab),
            pay: LedgerObjectsPanel::new("Pay channels", LedgerObjectFilter::PayChannelOnly),
            escrow: LedgerObjectsPanel::new("Escrows", LedgerObjectFilter::EscrowOnly),
            focus_index: 0,
        }
    }

    fn update_focus(&mut self) {
        self.nft.is_focused = self.focus_index == 0;
        self.objects.is_focused = self.focus_index == 1;
        self.pay.is_focused = self.focus_index == 2;
        self.escrow.is_focused = self.focus_index == 3;
    }
}

impl Component for AssetsTab {
    fn init(&mut self, area: Size) -> color_eyre::Result<()> {
        self.nft.init(area)?;
        Ok(())
    }

    fn register_action_handler(
        &mut self,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> color_eyre::Result<()> {
        self.nft.register_action_handler(action_tx.clone())?;
        self.objects.register_action_handler(action_tx.clone())?;
        self.pay.register_action_handler(action_tx.clone())?;
        self.escrow.register_action_handler(action_tx)?;
        Ok(())
    }

    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
        self.nft.register_config_handler(Arc::clone(&config))?;
        self.objects.register_config_handler(Arc::clone(&config))?;
        self.pay.register_config_handler(Arc::clone(&config))?;
        self.escrow.register_config_handler(config)?;
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::FocusNext => {
                self.focus_index = (self.focus_index + 1) % 4;
                self.update_focus();
            }
            Action::FocusPrev => {
                self.focus_index = (self.focus_index + 3) % 4;
                self.update_focus();
            }
            _ => {}
        }

        if let Some(a) = self.nft.update(action)? {
            return Ok(Some(a));
        }
        if let Some(a) = self.objects.update(action)? {
            return Ok(Some(a));
        }
        if let Some(a) = self.pay.update(action)? {
            return Ok(Some(a));
        }
        self.escrow.update(action)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let [a, b, c, d] = Layout::vertical([
            Constraint::Percentage(30),
            Constraint::Percentage(30),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .areas(area);

        self.nft.draw(frame, a)?;
        self.objects.draw(frame, b)?;
        self.pay.draw(frame, c)?;
        self.escrow.draw(frame, d)?;
        Ok(())
    }
}
