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
    action_tx: Option<tokio::sync::mpsc::UnboundedSender<Action>>,
}

impl AccountWalletTab {
    /// `wallet`: the signing wallet's address (or why it has none), derived once by the app;
    /// `None` = no wallet configured (`--watch` or no key).
    pub fn new(skip_mainnet_prompt: bool, wallet: Option<Result<String, String>>) -> Self {
        let mut account = AccountPanel::default();
        account.is_focused = true;
        let has_wallet = wallet.is_some();
        Self {
            wallet: WalletPanel::new(skip_mainnet_prompt).with_seed_address(wallet),
            account,
            tx: TxHistoryPanel::default(),
            focus_index: 0,
            has_wallet,
            action_tx: None,
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
        self.action_tx = Some(action_tx.clone());
        self.wallet.register_action_handler(action_tx.clone())?;
        self.account.register_action_handler(action_tx.clone())?;
        self.tx.register_action_handler(action_tx)?;
        Ok(())
    }

    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
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
            Action::TxComposerOpen { .. } => {
                self.focus_index = 0;
                self.update_focus();
            }
            _ => {}
        }

        let tx = self.action_tx.as_ref();
        if self.has_wallet {
            crate::components::update_children(&mut [&mut self.wallet, &mut self.tx], action, tx)
        } else {
            crate::components::update_children(
                &mut [&mut self.wallet, &mut self.account, &mut self.tx],
                action,
                tx,
            )
        }
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

    /// Keys and paste go to the focused pane only (wallet composer / tx history filter).
    fn on_event(&mut self, event: Option<&Event>) -> color_eyre::Result<Option<Action>> {
        match event {
            Some(Event::Key(key)) if self.has_wallet && self.wallet.is_focused => {
                self.wallet.on_key_event(*key)
            }
            Some(Event::Key(key)) => self.tx.on_key_event(*key),
            Some(Event::Paste(text)) => {
                self.wallet.paste(text);
                self.tx.paste(text);
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;

    fn tab_with_signing(seed: Option<&str>, mnemonic: Option<&str>) -> AccountWalletTab {
        let secrets = crate::config::Secrets {
            seed: seed.map(|s| SecretString::from(s.to_string())),
            mnemonic: mnemonic.map(|m| SecretString::from(m.to_string())),
            flare_evm_key: None,
        };
        let wallet = secrets
            .credential()
            .unwrap()
            .map(|c| c.address().map_err(|e| e.to_string()));
        let mut tab = AccountWalletTab::new(false, wallet);
        tab.register_config_handler(Arc::new(Config::default()))
            .unwrap();
        tab
    }

    /// TC-170: wallet detection seed / mnemonic / none
    /// has_wallet reflects any signing config (seed or mnemonic), not chain selection.
    #[test]
    fn account_wallet_tab_has_wallet_matches_signing_config() {
        let cases: &[(&str, Option<&str>, Option<&str>, bool)] = &[
            (
                "family seed config sets has_wallet",
                Some("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB"),
                None,
                true,
            ),
            (
                "mnemonic config sets has_wallet",
                None,
                Some(
                    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
                ),
                true,
            ),
            (
                "no seed/mnemonic config leaves has_wallet false",
                None,
                None,
                false,
            ),
        ];
        for (desc, seed, mnemonic, expected) in cases {
            assert_eq!(
                tab_with_signing(*seed, *mnemonic).has_wallet,
                *expected,
                "{desc}"
            );
        }
    }

    fn key_event(c: char) -> Event {
        Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(c),
            crossterm::event::KeyModifiers::empty(),
        ))
    }

    /// With a wallet configured, keys reach the focused pane: the wallet first, then (after
    /// FocusNext) Tx History — `f` starts the filter and `m` pages.
    #[test]
    fn keys_reach_focused_tx_history_pane() {
        let mut tab = tab_with_signing(Some("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB"), None);
        let marker = serde_json::json!({ "ledger": 1, "seq": 2 });
        tab.update(&Action::XrplTxHistory(vec![], Some(marker.clone())))
            .unwrap();

        // Wallet focused: `m` is not a Tx History key.
        assert_eq!(tab.on_event(Some(&key_event('m'))).unwrap(), None);

        tab.update(&Action::FocusNext).unwrap();
        assert_eq!(
            tab.on_event(Some(&key_event('m'))).unwrap(),
            Some(Action::RefreshTxHistoryMore(Some(marker)))
        );
        assert_eq!(
            tab.on_event(Some(&key_event('f'))).unwrap(),
            Some(Action::SetKeymapSuppression(true))
        );
    }
}
