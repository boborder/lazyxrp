#![allow(dead_code)]
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::Block,
};

use crate::{
    action::Action,
    components::{
        Component,
        panels::{
            flare_ftso::FlareFtsoPanel, fxrp_direct_mint::FxrpDirectMintPanel, oracle::OraclePanel,
        },
        shared::theme,
    },
};

pub struct CombinedOraclePanel {
    oracle: OraclePanel,
    ftso: FlareFtsoPanel,
    fxrp: FxrpDirectMintPanel,
    pub is_focused: bool,
}

impl CombinedOraclePanel {
    pub fn new() -> Self {
        Self {
            oracle: OraclePanel::new(),
            ftso: FlareFtsoPanel::new(),
            fxrp: FxrpDirectMintPanel::new(),
            is_focused: false,
        }
    }
}

impl Component for CombinedOraclePanel {
    fn register_action_handler(
        &mut self,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> color_eyre::Result<()> {
        self.oracle.register_action_handler(action_tx.clone())?;
        self.ftso.register_action_handler(action_tx.clone())?;
        self.fxrp.register_action_handler(action_tx)?;
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        // Overview has no sub-focus: route selection keys to Oracle only.
        self.oracle.is_focused = self.is_focused;
        self.ftso.is_focused = false;
        self.fxrp.is_focused = false;
        if let Some(a) = self.oracle.update(action)? {
            return Ok(Some(a));
        }
        if let Some(a) = self.ftso.update(action)? {
            return Ok(Some(a));
        }
        self.fxrp.update(action)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let block = Block::bordered()
            .title(" Oracle / FTSO / FXRP ")
            .border_style(if self.is_focused {
                theme::accent_style()
            } else {
                theme::dim_style()
            });
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let [oracle_area, _gap, right] = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(inner);

        let [ftso_area, fxrp_area] =
            Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(right);

        let sub_border = if self.is_focused {
            theme::accent_style()
        } else {
            theme::dim_style()
        };

        let oracle_block = Block::bordered().title(" Oracle ").border_style(sub_border);
        let oracle_inner = oracle_block.inner(oracle_area);
        frame.render_widget(oracle_block, oracle_area);
        self.oracle.render_content(frame, oracle_inner);

        let ftso_block = Block::bordered().title(" FTSO ").border_style(sub_border);
        let ftso_inner = ftso_block.inner(ftso_area);
        frame.render_widget(ftso_block, ftso_area);
        self.ftso.render_content(frame, ftso_inner);

        let fxrp_block = Block::bordered()
            .title(" FXRP Direct Mint ")
            .border_style(sub_border);
        let fxrp_inner = fxrp_block.inner(fxrp_area);
        frame.render_widget(fxrp_block, fxrp_area);
        self.fxrp.render_content(frame, fxrp_inner);
        Ok(())
    }
}
