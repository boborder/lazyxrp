//! Local keygen result popup for [`super::WalletPanel`].
use ratatui::{
    Frame,
    layout::Rect,
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
                warning_style.bold(),
            )),
            Line::from(Span::styled(
                "   to activate · Esc / g to dismiss",
                theme::secondary_style(),
            )),
        ]);
        frame.render_widget(body, inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::components::Component;
    use crate::xrpl::GeneratedWalletKeys;

    fn popup_rendered(panel: &WalletPanel, width: u16, height: u16) -> String {
        crate::test_support::render_to_string(width, height, |frame| {
            panel.render_keygen_popup(frame, frame.area())
        })
    }

    fn keygen_result() -> GeneratedWalletKeys {
        GeneratedWalletKeys {
            master_seed: "sEdTestOnlySeedValue".into(),
            master_seed_hex: "DEADBEEF01".into(),
            account_id: "rTestAccountAddress123".into(),
            public_key: "aTestPublicKey99".into(),
            public_key_hex: "CAFEBABE".into(),
            key_type: "ed25519".into(),
        }
    }

    #[test]
    fn keygen_popup_renders_nothing_when_no_keygen_result() {
        let panel = WalletPanel::new(false);
        assert!(popup_rendered(&panel, 80, 24).trim().is_empty());
    }

    #[test]
    fn keygen_popup_renders_seed_address_pubkey_and_offline_warning() {
        let mut panel = WalletPanel::new(false);
        panel
            .update(&Action::GenerateWalletKeysOk(keygen_result()))
            .unwrap();

        let out = popup_rendered(&panel, 100, 24);
        assert!(out.contains("New Key (local)"));
        assert!(out.contains("Seed:   sEdTestOnlySeedValue"));
        assert!(out.contains("Addr:   rTestAccountAddress123"));
        assert!(out.contains("PubKey: aTestPublicKey99"));
        assert!(out.contains("Type:   ed25519  Seed hex: DEADBEEF01"));
        assert!(out.contains("Save the seed offline!"));
        assert!(out.contains("Set XRPL_SEED=<seed>"));
        assert!(out.contains("Esc / g to dismiss"));
    }
}
