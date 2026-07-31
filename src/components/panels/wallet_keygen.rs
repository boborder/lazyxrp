//! Local keygen result popup for [`super::WalletPanel`].
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use super::WalletPanel;
use crate::components::shared::theme;

impl WalletPanel {
    pub(super) fn render_keygen_popup(&self, frame: &mut Frame, area: Rect) {
        let Some(ref result) = self.keygen_result else {
            return;
        };

        let popup_w = 60u16;
        let popup_h = 11u16;
        let popup_x = area.x + (area.width.saturating_sub(popup_w)) / 2;
        let popup_y = area.y + (area.height.saturating_sub(popup_h)) / 2;
        let popup = Rect::new(popup_x, popup_y, popup_w, popup_h);

        frame.render_widget(Clear, popup);
        let block = theme::panel_block("New Key (local)", true);
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let label_style = theme::dim_style();
        let value_style = theme::accent_style();
        let warning_style = theme::warning_style();

        let body = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Seed:   ", label_style),
                Span::styled(result.master_seed.clone(), warning_style),
            ]),
            Line::from(vec![
                Span::styled("Addr:   ", label_style),
                Span::styled(result.account_id.clone(), value_style),
            ]),
            Line::from(vec![
                Span::styled("PubKey: ", label_style),
                Span::styled(result.public_key.clone(), theme::secondary_style()),
            ]),
            Line::from(vec![
                Span::styled("Type:   ", label_style),
                Span::raw(format!(
                    "{}  Seed hex: {}",
                    result.key_type, result.master_seed_hex
                )),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "⚠ Save the seed offline!  Set XRPL_SEED=<seed>",
                warning_style.add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "   to activate · Esc / g to dismiss",
                theme::secondary_style(),
            )),
        ]);
        frame.render_widget(body, inner);
    }
}
