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
            fmt, theme,
            widgets::{render_loading, titled_block},
        },
    },
    flare::{bips_to_percent_display, uba_to_xrp_display},
    xrpl::FxrpDirectMintInfo,
};

#[derive(Default)]
pub struct FxrpDirectMintPanel {
    info: Option<FxrpDirectMintInfo>,
    tick: usize,
    pub is_focused: bool,
}

impl FxrpDirectMintPanel {
    pub(crate) fn compact_summary(&self) -> String {
        match &self.info {
            None => "loading…".to_string(),
            Some(info) => format!(
                "fee {} · min {} XRP",
                bips_to_percent_display(info.fee_bips),
                uba_to_xrp_display(info.min_fee_uba),
            ),
        }
    }

    pub(crate) fn render_compact(&self, frame: &mut Frame, area: Rect) {
        let line = Line::from(vec![
            Span::styled("FXRP ", theme::dim_style()),
            Span::styled(self.compact_summary(), theme::secondary_style()),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    pub(crate) fn render_content(&mut self, frame: &mut Frame, area: Rect) {
        let Some(info) = &self.info else {
            render_loading(
                frame,
                area,
                "FXRP",
                self.tick,
                "loading AssetManager…",
                self.is_focused,
            );
            return;
        };

        let label = theme::dim_style();
        let value = theme::accent_style();
        let lines = vec![
            Line::from(vec![
                Span::styled("Vault ", label),
                Span::styled(fmt::truncate_middle(&info.core_vault_xrpl, 28), value),
            ]),
            Line::from(vec![
                Span::styled("Min fee ", label),
                Span::styled(
                    format!("{} XRP", uba_to_xrp_display(info.min_fee_uba)),
                    value,
                ),
            ]),
            Line::from(vec![
                Span::styled("Mint fee ", label),
                Span::styled(bips_to_percent_display(info.fee_bips), value),
            ]),
            Line::from(vec![
                Span::styled("Exec fee ", label),
                Span::styled(
                    format!("{} XRP", uba_to_xrp_display(info.executor_fee_uba)),
                    value,
                ),
            ]),
            Line::from(vec![
                Span::styled("AM ", label),
                Span::styled(
                    fmt::truncate_middle(&info.asset_manager, 22),
                    theme::dim_style(),
                ),
            ]),
            Line::from(Span::styled(
                "Direct Mint · read-only",
                theme::secondary_style(),
            )),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }
}

impl Component for FxrpDirectMintPanel {
    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::FxrpDirectMintInfo(info) => {
                self.info = Some((**info).clone());
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let block = titled_block("FXRP Direct Mint", self.is_focused);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        self.render_content(frame, inner);
        Ok(())
    }
}
