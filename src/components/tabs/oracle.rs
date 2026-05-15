use ratatui::{Frame, layout::Rect};

use crate::{
    action::Action,
    components::{Component, panels::oracle::OraclePanel},
};

pub struct OracleTab {
    panel: OraclePanel,
}

impl OracleTab {
    pub fn new() -> Self {
        Self {
            panel: OraclePanel::new(),
        }
    }
}

impl Component for OracleTab {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        self.panel.update(action)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        self.panel.draw(frame, area)
    }
}
