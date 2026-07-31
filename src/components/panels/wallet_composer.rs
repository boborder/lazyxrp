//! Composer open/validate/preview/submit + modal rendering for [`super::WalletPanel`].
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use super::{ComposerPhase, SubmitFlash, WalletPanel};
use crate::{
    action::Action,
    components::shared::theme,
    xrpl::{
        AccountSetSubmitParams, OfferCreateSubmitParams, PaymentSubmitParams,
        SetRegularKeySubmitParams,
    },
};

impl WalletPanel {
    pub(super) fn open_account_set_composer(&mut self) {
        self.domain = self
            .account
            .as_ref()
            .and_then(|a| a.domain_hex.as_deref())
            .and_then(Self::decode_domain_hex)
            .unwrap_or_default();
        self.tick_size.clear();
        self.transfer_rate.clear();
        self.set_flag_ix = 0;
        self.clear_flag_ix = 0;
        self.field_row = 0;
        self.is_form_editing = false;
        self.composer = Some(ComposerPhase::AccountSet);
    }

    /// XRP payment defaults to 1 XRP; IOU fields start empty until toggled.
    pub(super) fn open_payment_composer(&mut self) {
        self.composer = Some(ComposerPhase::Payment {
            row: 0,
            destination: String::new(),
            amount: "1".to_string(),
            iou_currency: String::new(),
            iou_issuer: String::new(),
            is_iou: false,
        });
        self.is_form_editing = false;
    }

    pub(super) fn open_set_regular_key_composer(&mut self) {
        self.composer = Some(ComposerPhase::SetRegularKey {
            regular_key: String::new(),
        });
        self.is_form_editing = false;
    }

    pub(super) fn queue_submit_set_regular_key(&self) -> Action {
        let regular_key = match &self.composer {
            Some(ComposerPhase::SetRegularKey { regular_key }) => regular_key.clone(),
            _ => String::new(),
        };
        Action::SetRegularKeySubmit(SetRegularKeySubmitParams {
            regular_key,
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            config_seed: self.config_seed(),
        })
    }

    pub(super) fn open_offer_create_composer(&mut self) {
        self.composer = Some(ComposerPhase::OfferCreate {
            row: 0,
            taker_gets: "XRP:1000000".to_string(),
            taker_pays: "USD:rIssuerReplaceMe:10".to_string(),
        });
        self.is_form_editing = false;
    }

    pub(super) fn queue_submit_offer_create(&self) -> Action {
        let (taker_gets, taker_pays) = match &self.composer {
            Some(ComposerPhase::OfferCreate {
                taker_gets,
                taker_pays,
                ..
            }) => (taker_gets.clone(), taker_pays.clone()),
            _ => (String::new(), String::new()),
        };
        Action::OfferCreateSubmit(OfferCreateSubmitParams {
            taker_gets,
            taker_pays,
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            config_seed: self.config_seed(),
        })
    }

    pub(super) fn queue_submit_account_set(&mut self) -> Option<Action> {
        let set = Self::label_for_flag(self.set_flag_ix);
        let clr = Self::label_for_flag(self.clear_flag_ix);
        Some(Action::AccountSetSubmit(AccountSetSubmitParams {
            set_flag: if set == "(none)" { None } else { Some(set) },
            clear_flag: if clr == "(none)" { None } else { Some(clr) },
            domain_ascii: self.domain.clone(),
            tick_size: self.tick_size.clone(),
            transfer_rate: self.transfer_rate.clone(),
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            config_seed: self.config_seed(),
        }))
    }

    pub(super) fn queue_submit_payment(
        &self,
        dest: String,
        amt: String,
        iou_currency: String,
        iou_issuer: String,
    ) -> Action {
        Action::PaymentSubmit(PaymentSubmitParams {
            destination: dest,
            amount: amt,
            iou_currency: if iou_currency.is_empty() {
                None
            } else {
                Some(iou_currency.to_uppercase())
            },
            iou_issuer: if iou_issuer.is_empty() {
                None
            } else {
                Some(iou_issuer)
            },
            destination_tag: None,
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            config_seed: self.config_seed(),
        })
    }

    pub(super) fn set_submit_flash(&mut self, flash: SubmitFlash) {
        self.submit_flash = Some(flash);
    }

    pub(super) fn payment_validate(
        dest: &str,
        amt: &str,
        is_iou: bool,
        currency: &str,
        issuer: &str,
    ) -> Result<(), &'static str> {
        if dest.trim().is_empty() {
            return Err("destination required");
        }
        if amt.trim().is_empty() {
            return Err("amount required");
        }
        let Ok(v) = amt.trim().parse::<f64>() else {
            return Err("amount must be a number");
        };
        if !v.is_finite() || v <= 0.0 {
            return Err("amount must be > 0");
        }
        if is_iou {
            let cur = currency.trim();
            if cur.len() != 3 {
                return Err("IOU currency must be 3 characters");
            }
            let iss = issuer.trim();
            if iss.is_empty() {
                return Err("IOU issuer required");
            }
            if !iss.starts_with('r') {
                return Err("issuer must start with 'r'");
            }
        }
        Ok(())
    }

    pub(super) fn payment_preview(
        destination: &str,
        amount: &str,
        iou_currency: &str,
        iou_issuer: &str,
        is_iou: bool,
    ) -> (String, Style) {
        let destination_trimmed = destination.trim();
        let amount_trimmed = amount.trim();
        let currency_trimmed = iou_currency.trim();
        let issuer_trimmed = iou_issuer.trim();
        let amount_ok = amount_trimmed
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v > 0.0);

        if destination_trimmed.is_empty() {
            return (
                "Need destination (classic r… or X-address)".to_string(),
                theme::warning_style(),
            );
        }
        if amount_trimmed.is_empty() {
            return ("Need amount".to_string(), theme::warning_style());
        }
        if amount_ok.is_none() {
            return (
                "Amount must be a number > 0".to_string(),
                theme::warning_style(),
            );
        }
        if is_iou && currency_trimmed.len() != 3 {
            return (
                "IOU mode: need 3-char currency code".to_string(),
                theme::warning_style(),
            );
        }
        if is_iou && issuer_trimmed.is_empty() {
            return (
                "IOU mode: need issuer address (r…)".to_string(),
                theme::warning_style(),
            );
        }
        if is_iou && !issuer_trimmed.starts_with('r') {
            return (
                "Issuer must start with 'r'".to_string(),
                theme::warning_style(),
            );
        }
        if is_iou {
            return (
                format!(
                    "▸ Pay {} {currency_trimmed} (issued by {}) → {}",
                    amount_trimmed,
                    Self::shorten_display(issuer_trimmed, 20),
                    Self::shorten_display(destination_trimmed, 20)
                ),
                theme::success_style(),
            );
        }
        (
            format!(
                "▸ Send {} XRP → {}",
                amount_trimmed,
                Self::shorten_display(destination_trimmed, 30)
            ),
            theme::success_style(),
        )
    }

    pub(super) fn flag_labels(flags: u32) -> Vec<&'static str> {
        let mut labels = Vec::new();
        if flags & 0x00800000 != 0 {
            labels.push("DefaultRipple");
        }
        if flags & 0x01000000 != 0 {
            labels.push("DepositAuth");
        }
        if flags & 0x00100000 != 0 {
            labels.push("DisableMaster");
        }
        if flags & 0x00080000 != 0 {
            labels.push("DisallowXRP");
        }
        if flags & 0x00400000 != 0 {
            labels.push("GlobalFreeze");
        }
        if flags & 0x00200000 != 0 {
            labels.push("NoFreeze");
        }
        if flags & 0x00040000 != 0 {
            labels.push("RequireAuth");
        }
        if flags & 0x00020000 != 0 {
            labels.push("RequireDest");
        }
        if flags & 0x00010000 != 0 {
            labels.push("PasswordSpent");
        }
        labels
    }

    pub(super) fn shorten_display(text: &str, max: usize) -> String {
        let trimmed = text.trim();
        if trimmed.len() <= max {
            return trimmed.to_string();
        }
        let keep = max.saturating_sub(1).max(8);
        let head = keep * 2 / 3;
        let tail = keep - head;
        format!("{}…{}", &trimmed[..head], &trimmed[trimmed.len() - tail..])
    }

    /// Decode `AccountRoot.domain` hex → ASCII string, or None if invalid.
    pub(super) fn decode_domain_hex(hex: &str) -> Option<String> {
        let hex = hex.trim();
        if !hex.len().is_multiple_of(2) {
            return None;
        }
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
            .collect();
        if bytes.len() != hex.len() / 2 {
            return None;
        }
        String::from_utf8(bytes).ok()
    }

    pub(super) fn render_composer(&self, frame: &mut Frame, area: Rect) {
        let Some(phase) = &self.composer else {
            return;
        };

        let popup_w = area.width.clamp(54, 74);
        let popup_h = match phase {
            ComposerPhase::PickKind { .. } => 16u16,
            ComposerPhase::AccountSet => 21u16,
            ComposerPhase::Payment { is_iou, .. } => {
                if *is_iou {
                    20u16
                } else {
                    16u16
                }
            }
            ComposerPhase::SetRegularKey { .. } => 14u16,
            ComposerPhase::OfferCreate { .. } => 14u16,
        }
        .min(area.height.saturating_sub(2))
        .max(8);

        let popup_x = area.x + (area.width.saturating_sub(popup_w)) / 2;
        let popup_y = area.y + (area.height.saturating_sub(popup_h)) / 2;
        let popup = Rect::new(popup_x, popup_y, popup_w, popup_h);

        frame.render_widget(Clear, popup);
        let inner_title = match phase {
            ComposerPhase::PickKind { .. } => "Transaction type",
            ComposerPhase::AccountSet => "AccountSet",
            ComposerPhase::Payment { is_iou, .. } => {
                if *is_iou {
                    "Payment (IOU)"
                } else {
                    "Payment (XRP)"
                }
            }
            ComposerPhase::SetRegularKey { .. } => "SetRegularKey",
            ComposerPhase::OfferCreate { .. } => "OfferCreate",
        };
        let block = theme::panel_block(inner_title, true);
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let label_style = theme::dim_style();
        let value_style = theme::accent_style();
        let highlight_style = Style::new().bold().underlined();

        let body = match phase {
            ComposerPhase::PickKind { selected } => {
                let hi0 = if *selected == 0 {
                    highlight_style
                } else {
                    label_style
                };
                let hi1 = if *selected == 1 {
                    highlight_style
                } else {
                    label_style
                };
                let hi2 = if *selected == 2 {
                    highlight_style
                } else {
                    label_style
                };
                let hi3 = if *selected == 3 {
                    highlight_style
                } else {
                    label_style
                };
                Paragraph::new(vec![
                    Line::from(""),
                    Line::from(vec![
                        Span::raw(if *selected == 0 { "⟩ " } else { "  " }),
                        Span::styled(
                            "AccountSet — SetFlag / ClearFlag, domain, TickSize, TransferRate",
                            hi0,
                        ),
                    ]),
                    Line::from(vec![
                        Span::raw(if *selected == 1 { "⟩ " } else { "  " }),
                        Span::styled("Payment — XRP to classic `r…` or X-address", hi1),
                    ]),
                    Line::from(vec![
                        Span::raw(if *selected == 2 { "⟩ " } else { "  " }),
                        Span::styled(
                            "SetRegularKey — assign or CLEAR regular key (dangerous)",
                            hi2,
                        ),
                    ]),
                    Line::from(vec![
                        Span::raw(if *selected == 3 { "⟩ " } else { "  " }),
                        Span::styled(
                            "OfferCreate — TakerGets/Pays (XRP:drops or CUR:issuer:value)",
                            hi3,
                        ),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled(
                        "j/k · ↑/↓ · Tab · Enter open · Esc close",
                        theme::secondary_style(),
                    )),
                ])
            }
            ComposerPhase::AccountSet => {
                let net_note = if self.network.is_mainnet() && !self.skip_mainnet_prompt {
                    " · mainnet writes need --yes"
                } else {
                    ""
                };
                Paragraph::new(vec![
                    Line::from(vec![
                        Span::styled(
                            format!("SetFlag [{}]", if self.field_row == 0 { "*" } else { " " }),
                            if self.field_row == 0 {
                                highlight_style
                            } else {
                                label_style
                            },
                        ),
                        Span::styled(Self::label_for_flag(self.set_flag_ix), value_style),
                        Span::raw("  , ."),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "ClearFlag [{}]",
                                if self.field_row == 1 { "*" } else { " " }
                            ),
                            if self.field_row == 1 {
                                highlight_style
                            } else {
                                label_style
                            },
                        ),
                        Span::styled(Self::label_for_flag(self.clear_flag_ix), value_style),
                        Span::raw("  , ."),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "Domain(ascii) [{}]",
                                if self.field_row == 2 { "*" } else { " " }
                            ),
                            if self.field_row == 2 {
                                highlight_style
                            } else {
                                label_style
                            },
                        ),
                        Span::styled(self.domain.clone(), value_style),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            format!("TickSize [{}]", if self.field_row == 3 { "*" } else { " " }),
                            if self.field_row == 3 {
                                highlight_style
                            } else {
                                label_style
                            },
                        ),
                        Span::styled(self.tick_size.clone(), value_style),
                        Span::raw("  0 or 3–15, empty=skip"),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "TransferRate [{}]",
                                if self.field_row == 4 { "*" } else { " " }
                            ),
                            if self.field_row == 4 {
                                highlight_style
                            } else {
                                label_style
                            },
                        ),
                        Span::styled(self.transfer_rate.clone(), value_style),
                        Span::raw("  empty=skip"),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled(
                        format!("[ ] Tab row · e edit · s send · ^S send · Esc ← picker{net_note}"),
                        theme::secondary_style(),
                    )),
                ])
            }
            ComposerPhase::Payment {
                row,
                destination,
                amount,
                iou_currency,
                iou_issuer,
                is_iou,
            } => {
                let net_note = if self.network.is_mainnet() && !self.skip_mainnet_prompt {
                    " · mainnet sends need --yes"
                } else {
                    ""
                };
                let (preview_text, preview_st) =
                    Self::payment_preview(destination, amount, iou_currency, iou_issuer, *is_iou);

                let value_st = theme::accent_style();
                let labels = theme::dim_style();

                let mut lines: Vec<Line> = vec![Line::from(vec![
                    Span::styled(
                        format!("Destination   [{}]", if *row == 0 { "*" } else { " " }),
                        if *row == 0 { highlight_style } else { labels },
                    ),
                    Span::styled(destination.clone(), value_st),
                ])];

                if *is_iou {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("Currency (3c) [{}]", if *row == 1 { "*" } else { " " }),
                            if *row == 1 { highlight_style } else { labels },
                        ),
                        Span::styled(iou_currency.clone(), value_st),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("Issuer (r…)   [{}]", if *row == 2 { "*" } else { " " }),
                            if *row == 2 { highlight_style } else { labels },
                        ),
                        Span::styled(iou_issuer.clone(), value_st),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("Amount        [{}]", if *row == 3 { "*" } else { " " }),
                            if *row == 3 { highlight_style } else { labels },
                        ),
                        Span::styled(amount.clone(), value_st),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("Amount XRP    [{}]", if *row == 1 { "*" } else { " " }),
                            if *row == 1 { highlight_style } else { labels },
                        ),
                        Span::styled(amount.clone(), value_st),
                    ]));
                }

                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(preview_text, preview_st)));
                lines.push(Line::from(Span::styled(
                    format!(
                        "[ ] Tab rows · Enter edit · i toggle XRP/IOU · e type · s send · Esc back{net_note}",
                    ),
                    theme::secondary_style(),
                )));
                Paragraph::new(lines)
            }
            ComposerPhase::SetRegularKey { regular_key } => {
                let net_note = if self.network.is_mainnet() && !self.skip_mainnet_prompt {
                    " · mainnet writes need --yes"
                } else {
                    ""
                };
                let key_disp = if regular_key.is_empty() {
                    "(empty = CLEAR regular key)"
                } else {
                    regular_key.as_str()
                };
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        format!("WARNING: empty field CLEARS the regular key{net_note}"),
                        theme::error_style(),
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("RegularKey ", label_style),
                        Span::styled(key_disp, value_style),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled(
                        "e edit · Ctrl+S / S submit · Esc back",
                        theme::secondary_style(),
                    )),
                ])
            }
            ComposerPhase::OfferCreate {
                row,
                taker_gets,
                taker_pays,
            } => {
                let net_note = if self.network.is_mainnet() && !self.skip_mainnet_prompt {
                    " · mainnet writes need --yes"
                } else {
                    ""
                };
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        format!("Specs: XRP:drops or CUR:issuer:value{net_note}"),
                        theme::secondary_style(),
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(
                            format!("TakerGets [{}] ", if *row == 0 { "*" } else { " " }),
                            if *row == 0 {
                                highlight_style
                            } else {
                                label_style
                            },
                        ),
                        Span::styled(taker_gets.clone(), value_style),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            format!("TakerPays [{}] ", if *row == 1 { "*" } else { " " }),
                            if *row == 1 {
                                highlight_style
                            } else {
                                label_style
                            },
                        ),
                        Span::styled(taker_pays.clone(), value_style),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled(
                        "e edit · Tab rows · Ctrl+S / S submit · Esc back",
                        theme::secondary_style(),
                    )),
                ])
            }
        };
        frame.render_widget(body, inner);
    }
}
