#![allow(dead_code)]
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
    pub fn new() -> Self {
        Self {
            is_focused: false,
            ..Self::default()
        }
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
                Span::styled(shorten(&info.core_vault_xrpl, 28), value),
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
                Span::styled(shorten(&info.asset_manager, 22), theme::dim_style()),
            ]),
            Line::from(Span::styled(
                "Direct Mint · read-only",
                theme::secondary_style(),
            )),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }
}

fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let keep = max.saturating_sub(1);
        format!("{}…", s.chars().take(keep).collect::<String>())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_stores_direct_mint_info() {
        let mut panel = FxrpDirectMintPanel::new();
        let info = FxrpDirectMintInfo {
            core_vault_xrpl: "rCoreVaultTestAddressXXXXXXXXXXXXX".into(),
            asset_manager: "0xabc".into(),
            min_fee_uba: 100_000,
            fee_bips: 10,
            executor_fee_uba: 200_000,
        };
        panel
            .update(&Action::FxrpDirectMintInfo(Box::new(info.clone())))
            .unwrap();
        assert_eq!(panel.info.as_ref(), Some(&info));
    }
}
