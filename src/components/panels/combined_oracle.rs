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
        panels::{flare_ftso::FlareFtsoPanel, oracle::OraclePanel},
        shared::theme,
    },
};

pub struct CombinedOraclePanel {
    oracle: OraclePanel,
    ftso: FlareFtsoPanel,
    pub is_focused: bool,
}

impl CombinedOraclePanel {
    pub fn new() -> Self {
        Self {
            oracle: OraclePanel::new(),
            ftso: FlareFtsoPanel::new(),
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
        self.ftso.register_action_handler(action_tx)?;
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        if let Some(a) = self.oracle.update(action)? {
            return Ok(Some(a));
        }
        self.ftso.update(action)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let block = Block::bordered()
            .title(" Oracle / FTSO ")
            .border_style(if self.is_focused {
                theme::accent_style()
            } else {
                theme::dim_style()
            });
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let [oracle_area, ftso_area] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(inner);

        self.oracle.render_content(frame, oracle_area);
        self.ftso.render_content(frame, ftso_area);
        Ok(())
    }
}
