use std::sync::Arc;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};

use crate::{
    action::Action,
    components::{
        Component,
        panels::{amm::AmmPanel, book::BookPanel, trust_lines::TrustLinesPanel},
    },
    config::Config,
};

/// Combined panel showing Book Offers, Trust Lines, and AMM info.
pub struct MarketTab {
    book: BookPanel,
    lines: TrustLinesPanel,
    amm: AmmPanel,
    focus_index: usize,
}

impl MarketTab {
    pub fn new() -> Self {
        let mut book = BookPanel::new();
        book.is_focused = true;
        Self {
            book,
            lines: TrustLinesPanel::new(),
            amm: AmmPanel::new(),
            focus_index: 0,
        }
    }

    fn update_focus(&mut self) {
        self.book.is_focused = self.focus_index == 0;
        self.lines.is_focused = self.focus_index == 1;
        self.amm.is_focused = self.focus_index == 2;
    }
}

impl Component for MarketTab {
    fn register_action_handler(
        &mut self,
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    ) -> color_eyre::Result<()> {
        self.book.register_action_handler(action_tx.clone())?;
        self.lines.register_action_handler(action_tx.clone())?;
        self.amm.register_action_handler(action_tx)?;
        Ok(())
    }

    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
        self.book.register_config_handler(Arc::clone(&config))?;
        self.lines.register_config_handler(Arc::clone(&config))?;
        self.amm.register_config_handler(config)?;
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::FocusNext => {
                self.focus_index = (self.focus_index + 1) % 3;
                self.update_focus();
            }
            Action::FocusPrev => {
                self.focus_index = if self.focus_index == 0 {
                    2
                } else {
                    self.focus_index - 1
                };
                self.update_focus();
            }
            _ => {}
        }

        if let Some(a) = self.book.update(action)? {
            return Ok(Some(a));
        }
        if let Some(a) = self.lines.update(action)? {
            return Ok(Some(a));
        }
        self.amm.update(action)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let [top, mid, bottom] = Layout::vertical([
            Constraint::Percentage(45),
            Constraint::Percentage(30),
            Constraint::Percentage(25),
        ])
        .areas(area);
        self.book.draw(frame, top)?;
        self.lines.draw(frame, mid)?;
        self.amm.draw(frame, bottom)?;
        Ok(())
    }
}
