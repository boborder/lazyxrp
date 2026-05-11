use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            theme,
            widgets::{render_error, render_loading, titled_block},
        },
    },
    xrpl::AmmSummary,
};

#[derive(Default)]
pub struct AmmPanel {
    amm: Option<AmmSummary>,
    tick: usize,
    received: bool,
    error: Option<String>,
    pub is_focused: bool,
}

impl AmmPanel {
    pub fn new() -> Self {
        Self {
            is_focused: false,
            ..Self::default()
        }
    }
}

impl Component for AmmPanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplAmmInfo(amm) => {
                self.amm = Some((**amm).clone());
                self.received = true;
                self.error = None;
            }
            Action::XrplError(e) if e.contains("amm_info") => {
                self.received = true;
                self.error = Some(e.to_string());
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if !self.received {
            render_loading(
                frame,
                area,
                "AMM Pool",
                self.tick,
                "loading AMM info...",
                self.is_focused,
            );
            return Ok(());
        }
        if let Some(err) = &self.error {
            render_error(frame, area, "AMM Pool", err, self.is_focused);
            return Ok(());
        }
        let Some(amm) = &self.amm else {
            frame.render_widget(
                Paragraph::new(Span::styled("(no AMM data)", theme::dim_style()))
                    .block(titled_block("AMM Pool", self.is_focused)),
                area,
            );
            return Ok(());
        };
        let block = titled_block("AMM Pool", self.is_focused);
        let label = theme::dim_style();
        let value = theme::accent_style();
        let lines = vec![
            Line::from(vec![
                Span::styled("Asset1:      ", label),
                Span::styled(amm.asset1.clone(), value),
            ]),
            Line::from(vec![
                Span::styled("Asset2:      ", label),
                Span::styled(amm.asset2.clone(), value),
            ]),
            Line::from(vec![
                Span::styled("Pool1:       ", label),
                Span::raw(amm.pool1.clone()),
            ]),
            Line::from(vec![
                Span::styled("Pool2:       ", label),
                Span::raw(amm.pool2.clone()),
            ]),
            Line::from(vec![
                Span::styled("LP Token:    ", label),
                Span::raw(amm.lp_token.clone()),
            ]),
            Line::from(vec![
                Span::styled("Trading Fee: ", label),
                Span::raw(format!("{} (× 0.001%)", amm.trading_fee)),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines).block(block), area);
        Ok(())
    }
}
