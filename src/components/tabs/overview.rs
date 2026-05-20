use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};

use crate::{
    action::Action,
    components::{
        Component,
        panels::{combined_oracle::CombinedOraclePanel, server::ServerPanel},
    },
};

/// Dashboard tab: server (left), Oracle/FTSO combined (right).
pub struct OverviewTab {
    server: ServerPanel,
    combined: CombinedOraclePanel,
    focus_index: usize,
}

impl OverviewTab {
    pub fn new(server_url: String) -> Self {
        let mut server = ServerPanel::new(server_url);
        server.is_focused = true;
        let mut combined = CombinedOraclePanel::new();
        combined.is_focused = false;
        Self {
            server,
            combined,
            focus_index: 0,
        }
    }

    fn update_focus(&mut self) {
        self.server.is_focused = self.focus_index == 0;
        self.combined.is_focused = self.focus_index == 1;
    }
}

impl Component for OverviewTab {
    fn register_action_handler(
        &mut self,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> color_eyre::Result<()> {
        self.server.register_action_handler(action_tx.clone())?;
        self.combined.register_action_handler(action_tx)?;
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::FocusNext if self.focus_index + 1 < 2 => {
                self.focus_index += 1;
                self.update_focus();
            }
            Action::FocusPrev if self.focus_index > 0 => {
                self.focus_index -= 1;
                self.update_focus();
            }
            _ => {}
        }

        if let Some(a) = self.server.update(action)? {
            return Ok(Some(a));
        }
        self.combined.update(action)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(44), Constraint::Percentage(56)])
                .areas(area);

        self.server.draw(frame, left)?;
        self.combined.draw(frame, right)?;
        Ok(())
    }
}
