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
            amm::AmmPanel, book::BookPanel, flare_ftso::FlareFtsoPanel, oracle::OraclePanel,
            path_find::PathFindPanel, trust_lines::TrustLinesPanel,
        },
    },
    config::Config,
};

pub struct MarketOracleTab {
    book: BookPanel,
    path: PathFindPanel,
    amm: AmmPanel,
    trust: TrustLinesPanel,
    ftso: FlareFtsoPanel,
    oracle: OraclePanel,
    focus_index: usize,
}

impl MarketOracleTab {
    pub fn new() -> Self {
        let mut book = BookPanel::new();
        book.is_focused = true;
        Self {
            book,
            path: PathFindPanel::new(),
            amm: AmmPanel::new(),
            trust: TrustLinesPanel::new(),
            ftso: FlareFtsoPanel::new(),
            oracle: OraclePanel::new(),
            focus_index: 0,
        }
    }

    fn update_focus(&mut self) {
        self.book.is_focused = self.focus_index == 0;
        self.path.is_focused = self.focus_index == 1;
        self.amm.is_focused = self.focus_index == 2;
        self.trust.is_focused = self.focus_index == 3;
        self.ftso.is_focused = self.focus_index == 4;
        self.oracle.is_focused = self.focus_index == 5;
    }
}

impl Component for MarketOracleTab {
    fn register_action_handler(
        &mut self,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> color_eyre::Result<()> {
        self.book.register_action_handler(action_tx.clone())?;
        self.path.register_action_handler(action_tx.clone())?;
        self.amm.register_action_handler(action_tx.clone())?;
        self.trust.register_action_handler(action_tx.clone())?;
        self.ftso.register_action_handler(action_tx.clone())?;
        self.oracle.register_action_handler(action_tx)?;
        Ok(())
    }

    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
        self.book.register_config_handler(Arc::clone(&config))?;
        self.path.register_config_handler(Arc::clone(&config))?;
        self.amm.register_config_handler(Arc::clone(&config))?;
        self.trust.register_config_handler(Arc::clone(&config))?;
        self.ftso.register_config_handler(Arc::clone(&config))?;
        self.oracle.register_config_handler(config)?;
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::FocusNext => {
                self.focus_index = (self.focus_index + 1) % 6;
                self.update_focus();
            }
            Action::FocusPrev => {
                self.focus_index = (self.focus_index + 5) % 6;
                self.update_focus();
            }
            _ => {}
        }

        if let Some(a) = self.book.update(action)? {
            return Ok(Some(a));
        }
        if let Some(a) = self.path.update(action)? {
            return Ok(Some(a));
        }
        if let Some(a) = self.amm.update(action)? {
            return Ok(Some(a));
        }
        if let Some(a) = self.trust.update(action)? {
            return Ok(Some(a));
        }
        if let Some(a) = self.ftso.update(action)? {
            return Ok(Some(a));
        }
        self.oracle.update(action)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let [top, bottom] =
            Layout::vertical([Constraint::Percentage(70), Constraint::Percentage(30)]).areas(area);
        let [top_left, top_right] =
            Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).areas(top);
        let [path_a, amm_a, trust_a] = Layout::vertical([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .areas(top_right);
        let [ftso_a, oracle_a] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(bottom);

        self.book.draw(frame, top_left)?;
        self.path.draw(frame, path_a)?;
        self.amm.draw(frame, amm_a)?;
        self.trust.draw(frame, trust_a)?;
        self.ftso.draw(frame, ftso_a)?;
        self.oracle.draw(frame, oracle_a)?;
        Ok(())
    }
}
