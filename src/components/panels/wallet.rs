use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};
use secrecy::ExposeSecret;

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            fmt, theme,
            widgets::{render_empty, render_error, render_loading, titled_block},
        },
    },
    config::Config,
    network::Network,
    xrpl::{AccountSummary, WalletProposeResult},
};

/// Dropdown options for SetFlag / ClearFlag (must match [`crate::signing::parse_account_set_flag_choice`]).
const FLAG_OPTIONS: &[&str] = &[
    "(none)",
    "RequireDest",
    "DefaultRipple",
    "DepositAuth",
    "DisallowXRP",
    "GlobalFreeze",
    "DisableMaster",
    "NoFreeze",
    "RequireAuth",
];

const ACCOUNT_SET_ROWS: usize = 5;

#[derive(Clone)]
enum SubmitFlash {
    Success(String),
    Error(String),
}

#[derive(Clone, Debug)]
enum ComposerPhase {
    PickKind {
        selected: usize,
    },
    AccountSet,
    Payment {
        row: usize,
        destination: String,
        amount: String,
        iou_currency: String,
        iou_issuer: String,
        is_iou: bool,
    },
    /// Empty `regular_key` clears the existing regular key (dangerous).
    SetRegularKey {
        regular_key: String,
    },
}

const COMPOSER_KIND_COUNT: usize = 3;

#[path = "wallet_composer.rs"]
mod composer;
#[path = "wallet_keygen.rs"]
mod keygen;
#[path = "wallet_keys.rs"]
mod keys;

pub struct WalletPanel {
    account: Option<AccountSummary>,
    tick: usize,
    has_received_wallet_data: bool,
    /// False when no signing key is configured; the wallet tab shows a hint.
    wallet_configured: bool,
    seed: Option<String>,
    seed_address: Option<Result<String, String>>,
    pub is_focused: bool,
    skip_mainnet_prompt: bool,
    network: Network,
    config: Option<Arc<Config>>,
    composer: Option<ComposerPhase>,
    /// Suppresses global `h`/`l` while the tx composer modal is open or a field is being edited.
    is_form_editing: bool,
    field_row: usize,
    set_flag_ix: usize,
    clear_flag_ix: usize,
    domain: String,
    tick_size: String,
    transfer_rate: String,
    submit_flash: Option<SubmitFlash>,
    /// Key generation result overlay (WalletProposeOk → show, Esc to dismiss).
    keygen_result: Option<WalletProposeResult>,
}

impl Default for WalletPanel {
    fn default() -> Self {
        Self {
            account: None,
            tick: 0,
            has_received_wallet_data: false,
            wallet_configured: true,
            seed: None,
            seed_address: None,
            is_focused: false,
            skip_mainnet_prompt: false,
            network: Network::Mainnet,
            config: None,
            composer: None,
            is_form_editing: false,
            field_row: 0,
            set_flag_ix: 0,
            clear_flag_ix: 0,
            domain: String::new(),
            tick_size: String::new(),
            transfer_rate: String::new(),
            submit_flash: None,
            keygen_result: None,
        }
    }
}

impl WalletPanel {
    pub fn new(skip_mainnet_prompt: bool) -> Self {
        Self {
            skip_mainnet_prompt,
            ..Self::default()
        }
    }

    fn label_for_flag(ix: usize) -> String {
        FLAG_OPTIONS.get(ix).unwrap_or(&"(none)").to_string()
    }

    fn config_seed(&self) -> Option<String> {
        self.config.as_ref().and_then(|c| {
            c.xrpl
                .signing
                .secret_seed
                .as_ref()
                .map(|s| s.expose_secret().to_string())
        })
    }
}

impl Component for WalletPanel {
    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
        self.seed = config
            .xrpl
            .signing
            .secret_seed
            .as_ref()
            .map(|s| s.expose_secret().to_string());
        self.network = config.xrpl.network;
        self.config = Some(config);
        if let Some(ref s) = self.seed {
            self.seed_address = Some(crate::signing::seed_to_address(s));
        }
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplWalletOverview(acc) => {
                self.account = acc.clone();
                self.has_received_wallet_data = true;
                self.wallet_configured = true;
            }
            Action::XrplWalletNotConfigured => {
                self.has_received_wallet_data = true;
                self.wallet_configured = false;
            }
            Action::AccountSetSubmitOk(hash) => {
                self.set_submit_flash(SubmitFlash::Success(format!(
                    "AccountSet submitted · {hash}"
                )));
                if matches!(&self.composer, Some(ComposerPhase::AccountSet)) {
                    self.composer = None;
                    self.is_form_editing = false;
                    return Ok(Some(Action::SetKeymapSuppression(false)));
                }
            }
            Action::AccountSetSubmitErr(msg) => {
                self.set_submit_flash(SubmitFlash::Error(format!("AccountSet · {msg}")));
            }
            Action::PaymentSubmitOk(hash) => {
                self.set_submit_flash(SubmitFlash::Success(format!(
                    "Payment sent · {hash} (see Recent Transactions below)"
                )));
                if matches!(&self.composer, Some(ComposerPhase::Payment { .. })) {
                    self.composer = None;
                    self.is_form_editing = false;
                    return Ok(Some(Action::SetKeymapSuppression(false)));
                }
            }
            Action::PaymentSubmitErr(msg) => {
                self.set_submit_flash(SubmitFlash::Error(format!("Payment · {msg}")));
            }
            Action::SetRegularKeySubmitOk(hash) => {
                self.set_submit_flash(SubmitFlash::Success(format!(
                    "SetRegularKey submitted · {hash}"
                )));
                if matches!(&self.composer, Some(ComposerPhase::SetRegularKey { .. })) {
                    self.composer = None;
                    self.is_form_editing = false;
                    return Ok(Some(Action::SetKeymapSuppression(false)));
                }
            }
            Action::SetRegularKeySubmitErr(msg) => {
                self.set_submit_flash(SubmitFlash::Error(format!("SetRegularKey · {msg}")));
            }
            Action::WalletProposeOk(result) => {
                self.keygen_result = Some(result.clone());
            }
            Action::WalletProposeErr(msg) => {
                self.set_submit_flash(SubmitFlash::Error(format!("Keygen · {msg}")));
            }
            Action::NetworkChange(net) => {
                self.network = *net;
            }
            _ => {}
        }
        Ok(None)
    }

    fn on_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<Option<Action>> {
        if !self.is_focused {
            return Ok(None);
        }

        if key.code == KeyCode::Esc {
            if self.is_form_editing {
                self.is_form_editing = false;
                return Ok(None);
            }
            if self.keygen_result.is_some() {
                self.keygen_result = None;
                return Ok(None);
            }
            return Ok(match &self.composer {
                None => None,
                Some(ComposerPhase::PickKind { .. }) => {
                    self.composer = None;
                    Some(Action::SetKeymapSuppression(false))
                }
                Some(ComposerPhase::AccountSet) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 0 });
                    None
                }
                Some(ComposerPhase::Payment { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 1 });
                    None
                }
                Some(ComposerPhase::SetRegularKey { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 2 });
                    None
                }
            });
        }

        // 'i' toggles between XRP and IOU payment mode (disabled while typing)
        if let Some(ComposerPhase::Payment {
            ref mut is_iou,
            ref mut iou_currency,
            ref mut iou_issuer,
            ..
        }) = self.composer
            && !self.is_form_editing
            && matches!(key.code, KeyCode::Char('i') | KeyCode::Char('I'))
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            *is_iou = !*is_iou;
            if !*is_iou {
                iou_currency.clear();
                iou_issuer.clear();
            }
            return Ok(None);
        }

        if let Some(ComposerPhase::Payment {
            row,
            ref mut destination,
            ref mut amount,
            ref mut iou_currency,
            ref mut iou_issuer,
            is_iou,
        }) = self.composer
            && self.is_form_editing
            && Self::payment_edit_keys(
                destination,
                amount,
                iou_currency,
                iou_issuer,
                is_iou,
                row,
                &key,
            )
        {
            return Ok(None);
        }

        if matches!(&self.composer, Some(ComposerPhase::SetRegularKey { .. }))
            && matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
            && (key.modifiers.contains(KeyModifiers::CONTROL) || !self.is_form_editing)
        {
            return Ok(Some(self.queue_submit_set_regular_key()));
        }

        if let Some(ComposerPhase::SetRegularKey {
            ref mut regular_key,
        }) = self.composer
            && self.is_form_editing
        {
            match key.code {
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    regular_key.push(c);
                    return Ok(None);
                }
                KeyCode::Backspace => {
                    regular_key.pop();
                    return Ok(None);
                }
                _ => {}
            }
        }

        let payment_submit_pairs = match &self.composer {
            Some(ComposerPhase::Payment {
                destination,
                amount,
                iou_currency,
                iou_issuer,
                is_iou,
                ..
            }) if matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
                && (key.modifiers.contains(KeyModifiers::CONTROL) || !self.is_form_editing) =>
            {
                Some((
                    destination.clone(),
                    amount.clone(),
                    iou_currency.clone(),
                    iou_issuer.clone(),
                    *is_iou,
                ))
            }
            _ => None,
        };

        if let Some((d, a, cur, iss, iou)) = payment_submit_pairs {
            if let Err(m) = Self::payment_validate(&d, &a, iou, &cur, &iss) {
                self.set_submit_flash(SubmitFlash::Error(m.to_string()));
                return Ok(None);
            }
            // XRP mode must not leak leftover IOU fields into PaymentSubmitParams.
            let (cur, iss) = if iou {
                (cur, iss)
            } else {
                (String::new(), String::new())
            };
            return Ok(Some(self.queue_submit_payment(d, a, cur, iss)));
        }

        match &mut self.composer {
            Some(ComposerPhase::PickKind { selected }) => {
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        *selected = (*selected + 1) % COMPOSER_KIND_COUNT;
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        *selected = (*selected + COMPOSER_KIND_COUNT - 1) % COMPOSER_KIND_COUNT;
                    }
                    KeyCode::Tab => {
                        *selected = (*selected + 1) % COMPOSER_KIND_COUNT;
                    }
                    KeyCode::BackTab => {
                        *selected = (*selected + COMPOSER_KIND_COUNT - 1) % COMPOSER_KIND_COUNT;
                    }
                    KeyCode::Enter => match *selected {
                        0 => self.open_account_set_composer(),
                        1 => self.open_payment_composer(),
                        2 => self.open_set_regular_key_composer(),
                        _ => {}
                    },
                    _ => {}
                }
                return Ok(None);
            }
            Some(ComposerPhase::AccountSet) => {
                return Ok(self.account_set_modal_key_to_action(key));
            }
            Some(ComposerPhase::Payment { row, is_iou, .. }) => {
                let row_count: usize = if *is_iou { 4 } else { 2 };
                match key.code {
                    KeyCode::Char('e') | KeyCode::Char('E')
                        if !key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        self.is_form_editing = !self.is_form_editing;
                        if self.is_form_editing {
                            return Ok(Some(Action::SetKeymapSuppression(true)));
                        }
                    }
                    KeyCode::Enter => {
                        if self.is_form_editing {
                            *row = (*row + 1) % row_count;
                        } else {
                            self.is_form_editing = true;
                            return Ok(Some(Action::SetKeymapSuppression(true)));
                        }
                    }
                    KeyCode::Char('[') | KeyCode::BackTab => {
                        *row = (*row + row_count - 1) % row_count;
                    }
                    KeyCode::Char(']') | KeyCode::Tab => {
                        *row = (*row + 1) % row_count;
                    }
                    _ => {}
                }
                return Ok(None);
            }
            Some(ComposerPhase::SetRegularKey { .. }) => {
                match key.code {
                    KeyCode::Char('e') | KeyCode::Char('E')
                        if !key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        self.is_form_editing = !self.is_form_editing;
                        if self.is_form_editing {
                            return Ok(Some(Action::SetKeymapSuppression(true)));
                        }
                    }
                    KeyCode::Enter if !self.is_form_editing => {
                        self.is_form_editing = true;
                        return Ok(Some(Action::SetKeymapSuppression(true)));
                    }
                    _ => {}
                }
                return Ok(None);
            }
            None => {}
        }

        if self.composer.is_none()
            && matches!(key.code, KeyCode::Char('t') | KeyCode::Char('T'))
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.is_form_editing = false;
            self.composer = Some(ComposerPhase::PickKind { selected: 0 });
            return Ok(Some(Action::SetKeymapSuppression(true)));
        }

        // Key generation (g — only when no composer open)
        if self.composer.is_none()
            && key.code == KeyCode::Char('g')
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.keygen_result = None;
            return Ok(Some(Action::WalletPropose));
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if self.seed.is_none() {
            render_empty(
                frame,
                area,
                "Wallet",
                "seed not set — set XRPL_SEED env var or configure in config file",
                self.is_focused,
            );
            return Ok(());
        }

        if let Some(Err(ref e)) = self.seed_address {
            render_error(
                frame,
                area,
                "Wallet",
                &format!("invalid seed: {e}"),
                self.is_focused,
            );
            return Ok(());
        }

        if !self.wallet_configured {
            render_empty(
                frame,
                area,
                "Wallet",
                "set XRPL_SEED to view wallet",
                self.is_focused,
            );
            return Ok(());
        }

        if !self.has_received_wallet_data {
            render_loading(
                frame,
                area,
                "Wallet",
                self.tick,
                "loading...",
                self.is_focused,
            );
            return Ok(());
        }

        let Some(account) = self.account.as_ref() else {
            render_empty(frame, area, "Wallet", "None", self.is_focused);
            return Ok(());
        };

        let block = titled_block("Wallet", self.is_focused);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let label_style = theme::dim_style();
        let value_style = theme::accent_style();

        let [summary_area, hint] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);

        let flag_labels = Self::flag_labels(account.flags);
        let flag_spans: Vec<Span> = if flag_labels.is_empty() {
            vec![Span::styled(" (none)", theme::dim_style())]
        } else {
            flag_labels
                .iter()
                .flat_map(|l| {
                    vec![
                        Span::raw(" "),
                        Span::styled(format!("[{l}]"), theme::flag_style()),
                    ]
                })
                .collect()
        };

        let mut summary = vec![
            Line::from(vec![
                Span::styled("Account ", label_style),
                Span::styled(Self::shorten_display(&account.account, 42), value_style),
            ]),
            Line::from(vec![
                Span::styled("Balance ", label_style),
                Span::styled(
                    format!(
                        "{} XRP",
                        fmt::fmt_xrp(account.balance_xrp.parse().unwrap_or(0.0))
                    ),
                    value_style,
                ),
                Span::styled(
                    format!(
                        "  seq {}",
                        fmt::group_digits_u64(u64::from(account.sequence))
                    ),
                    theme::dim_style(),
                ),
                Span::styled(
                    format!("  owner {}", account.owner_count),
                    theme::dim_style(),
                ),
            ]),
            {
                let mut spans: Vec<Span> = vec![Span::styled("Flags", label_style)];
                spans.extend(flag_spans);
                spans.push(Span::styled(
                    "  ·  t composer  g keygen",
                    theme::dim_style(),
                ));
                Line::from(spans)
            },
        ];
        if let Some(ref rk) = account.regular_key {
            summary.push(Line::from(vec![
                Span::styled("RegKey ", label_style),
                Span::styled(Self::shorten_display(rk, 36), theme::warning_style()),
            ]));
        }
        if let Some(ref dh) = account.domain_hex {
            let domain_str = Self::decode_domain_hex(dh).unwrap_or_else(|| dh.clone());
            summary.push(Line::from(vec![
                Span::styled("Domain ", label_style),
                Span::styled(domain_str, theme::dim_style()),
            ]));
        }
        summary.push(Line::from(Span::styled(
            "Transactions: lower pane · j/k f m Enter",
            theme::dim_style(),
        )));
        frame.render_widget(Paragraph::new(summary), summary_area);

        let note_line = self
            .submit_flash
            .as_ref()
            .map(|f| match f {
                SubmitFlash::Success(s) => {
                    Line::from(Span::styled(s.clone(), theme::success_style()))
                }
                SubmitFlash::Error(s) => Line::from(Span::styled(s.clone(), theme::error_style())),
            })
            .unwrap_or_default();
        frame.render_widget(Paragraph::new(note_line), hint);

        self.render_composer(frame, area);
        self.render_keygen_popup(frame, area);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
    }

    #[test]
    #[allow(clippy::type_complexity)]
    fn payment_validate_table() {
        let cases: &[(&str, &str, bool, &str, &str, Result<(), &str>)] = &[
            ("", "1", false, "", "", Err("destination required")),
            ("rDest", "", false, "", "", Err("amount required")),
            (
                "rDest",
                "abc",
                false,
                "",
                "",
                Err("amount must be a number"),
            ),
            ("rDest", "0", false, "", "", Err("amount must be > 0")),
            ("rDest", "-1", false, "", "", Err("amount must be > 0")),
            ("rDest", "1", false, "", "", Ok(())),
            (
                "rDest",
                "1",
                true,
                "US",
                "rIssuer",
                Err("IOU currency must be 3 characters"),
            ),
            ("rDest", "1", true, "USD", "", Err("IOU issuer required")),
            (
                "rDest",
                "1",
                true,
                "USD",
                "notClassic",
                Err("issuer must start with 'r'"),
            ),
            ("rDest", "1", true, "USD", "rIssuer123", Ok(())),
            ("  rDest  ", " 1.5 ", true, " usd ", " rIssuer ", Ok(())),
            ("rDest", "NaN", false, "", "", Err("amount must be > 0")),
            ("rDest", "inf", false, "", "", Err("amount must be > 0")),
        ];
        for (dest, amt, is_iou, cur, iss, expected) in cases {
            assert_eq!(
                WalletPanel::payment_validate(dest, amt, *is_iou, cur, iss),
                *expected,
                "dest={dest:?} amt={amt:?} iou={is_iou} cur={cur:?} iss={iss:?}"
            );
        }
    }

    #[test]
    fn open_payment_composer_defaults_amount() {
        let mut panel = WalletPanel::new(false);
        panel.open_payment_composer();
        match &panel.composer {
            Some(ComposerPhase::Payment {
                amount,
                is_iou,
                destination,
                iou_currency,
                iou_issuer,
                row,
            }) => {
                assert_eq!(amount, "1");
                assert!(!is_iou);
                assert!(destination.is_empty());
                assert!(iou_currency.is_empty());
                assert!(iou_issuer.is_empty());
                assert_eq!(*row, 0);
            }
            _ => panic!("expected payment composer"),
        }
    }

    #[test]
    fn payment_i_toggles_iou_and_clears_fields_on_xrp() {
        let mut panel = WalletPanel::new(false);
        panel.is_focused = true;
        panel.open_payment_composer();
        panel.on_key_event(key('i')).expect("toggle to iou");
        match &panel.composer {
            Some(ComposerPhase::Payment { is_iou, .. }) => assert!(*is_iou),
            _ => panic!("expected payment composer"),
        }
        // Seed IOU fields then toggle back to XRP — fields must clear.
        if let Some(ComposerPhase::Payment {
            iou_currency,
            iou_issuer,
            ..
        }) = &mut panel.composer
        {
            *iou_currency = "usd".into();
            *iou_issuer = "rIssuer".into();
        }
        panel.on_key_event(key('i')).expect("toggle to xrp");
        match &panel.composer {
            Some(ComposerPhase::Payment {
                is_iou,
                iou_currency,
                iou_issuer,
                ..
            }) => {
                assert!(!is_iou);
                assert!(iou_currency.is_empty());
                assert!(iou_issuer.is_empty());
            }
            _ => panic!("expected payment composer"),
        }
    }

    #[test]
    fn payment_i_while_editing_types_into_destination() {
        let mut panel = WalletPanel::new(false);
        panel.is_focused = true;
        panel.open_payment_composer();
        panel.is_form_editing = true;
        panel.on_key_event(key('i')).expect("type i while editing");
        match &panel.composer {
            Some(ComposerPhase::Payment {
                is_iou,
                destination,
                ..
            }) => {
                assert!(!is_iou, "must not toggle while editing");
                assert_eq!(destination, "i");
            }
            _ => panic!("expected payment composer"),
        }
    }

    #[test]
    fn queue_submit_payment_uppercases_currency_and_omits_empty() {
        let panel = WalletPanel::new(true);
        match panel.queue_submit_payment(
            "rDest".into(),
            "2.5".into(),
            "usd".into(),
            "rIssuer".into(),
        ) {
            Action::PaymentSubmit(p) => {
                assert_eq!(p.destination, "rDest");
                assert_eq!(p.amount, "2.5");
                assert_eq!(p.iou_currency.as_deref(), Some("USD"));
                assert_eq!(p.iou_issuer.as_deref(), Some("rIssuer"));
                assert!(p.skip_mainnet_prompt);
            }
            other => panic!("expected PaymentSubmit, got {other:?}"),
        }
        match panel.queue_submit_payment("rDest".into(), "1".into(), String::new(), String::new()) {
            Action::PaymentSubmit(p) => {
                assert!(p.iou_currency.is_none());
                assert!(p.iou_issuer.is_none());
            }
            other => panic!("expected PaymentSubmit, got {other:?}"),
        }
    }

    #[test]
    fn payment_submit_key_queues_action_and_xrp_mode_drops_iou_fields() {
        let mut panel = WalletPanel::new(false);
        panel.is_focused = true;
        panel.open_payment_composer();
        if let Some(ComposerPhase::Payment {
            destination,
            amount,
            iou_currency,
            iou_issuer,
            is_iou,
            ..
        }) = &mut panel.composer
        {
            *destination = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into();
            *amount = "1.25".into();
            // Leftover IOU fields must be ignored while is_iou=false.
            *iou_currency = "usd".into();
            *iou_issuer = "rIssuer".into();
            *is_iou = false;
        }
        let action = panel
            .on_key_event(key('s'))
            .expect("submit")
            .expect("PaymentSubmit action");
        match action {
            Action::PaymentSubmit(p) => {
                assert_eq!(p.destination, "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh");
                assert_eq!(p.amount, "1.25");
                assert!(p.iou_currency.is_none());
                assert!(p.iou_issuer.is_none());
            }
            other => panic!("expected PaymentSubmit, got {other:?}"),
        }
    }

    #[test]
    fn payment_preview_xrp_and_iou_ready_messages() {
        let (xrp, _) = WalletPanel::payment_preview("rDestAddressLongEnough", "1.5", "", "", false);
        assert!(xrp.contains("Send 1.5 XRP"), "{xrp}");
        assert!(xrp.contains('→'), "{xrp}");

        let (iou, _) = WalletPanel::payment_preview(
            "rDestAddressLongEnough",
            "10",
            "USD",
            "rIssuerAddressLong",
            true,
        );
        assert!(iou.contains("Pay 10 USD"), "{iou}");
        assert!(iou.contains("issued by"), "{iou}");
    }

    #[test]
    fn open_set_regular_key_composer_and_queue_submit() {
        let mut panel = WalletPanel::new(true);
        panel.open_set_regular_key_composer();
        match &panel.composer {
            Some(ComposerPhase::SetRegularKey { regular_key }) => assert!(regular_key.is_empty()),
            other => panic!("expected SetRegularKey, got {other:?}"),
        }
        if let Some(ComposerPhase::SetRegularKey { regular_key }) = &mut panel.composer {
            *regular_key = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into();
        }
        match panel.queue_submit_set_regular_key() {
            Action::SetRegularKeySubmit(p) => {
                assert_eq!(p.regular_key, "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh");
                assert!(p.skip_mainnet_prompt);
            }
            other => panic!("expected SetRegularKeySubmit, got {other:?}"),
        }
    }

    #[test]
    fn open_account_set_prefills_domain_from_account() {
        let mut panel = WalletPanel::new(false);
        panel.account = Some(AccountSummary {
            account: "rTest".into(),
            balance_xrp: "10".into(),
            sequence: 1,
            flags: 0,
            owner_count: 0,
            regular_key: None,
            domain_hex: Some("6578616d706c652e636f6d".into()), // "example.com"
        });
        panel.open_account_set_composer();
        assert_eq!(panel.domain, "example.com");
    }
}
