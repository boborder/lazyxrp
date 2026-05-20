use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
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
    xrpl::{AccountSetSubmitParams, AccountSummary, PaymentSubmitParams, WalletProposeResult},
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

#[derive(Clone)]
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
}

/// Derive XRPL address from a family seed.
pub fn seed_to_address(seed: &str) -> Result<String, String> {
    crate::signing::wallet_from_family_seed(crate::signing::trim_family_seed(seed), 0)
        .map(|w| w.classic_address.clone())
        .map_err(|e| format!("{e}"))
}

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

    fn open_account_set_composer(&mut self) {
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
    fn open_payment_composer(&mut self) {
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

    fn queue_submit_account_set(&mut self) -> Option<Action> {
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

    fn queue_submit_payment(
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
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            config_seed: self.config_seed(),
        })
    }

    fn set_submit_flash(&mut self, flash: SubmitFlash) {
        self.submit_flash = Some(flash);
    }

    fn payment_validate(
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
        if v <= 0.0 {
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

    fn payment_preview(
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
        let amount_ok = amount_trimmed.parse::<f64>().ok().filter(|v| *v > 0.0);

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

    fn flag_labels(flags: u32) -> Vec<&'static str> {
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

    fn shorten_display(s: &str, max: usize) -> String {
        let t = s.trim();
        if t.len() <= max {
            return t.to_string();
        }
        let keep = max.saturating_sub(1).max(8);
        let head = keep * 2 / 3;
        let tail = keep - head;
        format!("{}…{}", &t[..head], &t[t.len() - tail..])
    }

    /// Decode `AccountRoot.domain` hex → ASCII string, or None if invalid.
    fn decode_domain_hex(hex: &str) -> Option<String> {
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

    fn account_set_edit_keys(&mut self, key: &KeyEvent) -> bool {
        if !self.is_form_editing || self.field_row < 2 {
            return false;
        }
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                match self.field_row {
                    2 => self.domain.push(c),
                    3 if c.is_ascii_digit() => self.tick_size.push(c),
                    4 if c.is_ascii_digit() => self.transfer_rate.push(c),
                    _ => {}
                }
                true
            }
            KeyCode::Backspace => {
                match self.field_row {
                    2 => {
                        self.domain.pop();
                    }
                    3 => {
                        self.tick_size.pop();
                    }
                    4 => {
                        self.transfer_rate.pop();
                    }
                    _ => {}
                }
                true
            }
            _ => false,
        }
    }

    fn payment_edit_keys(
        dest: &mut String,
        amt: &mut String,
        currency: &mut String,
        issuer: &mut String,
        is_iou: bool,
        row: usize,
        key: &KeyEvent,
    ) -> bool {
        // In IOU mode, rows 1 (currency) and 2 (issuer) are text fields;
        // row 3 is amount. In XRP mode, row 1 is amount.
        let target_row = if is_iou && row == 1 {
            currency
        } else if is_iou && row == 2 {
            issuer
        } else {
            amt
        };
        let is_dest = row == 0;
        let is_iou_text = is_iou && (row == 1 || row == 2);

        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let accept = if is_dest {
                    c.is_ascii_graphic()
                } else if is_iou_text && row == 1 {
                    c.is_ascii_alphabetic() && target_row.len() < 3
                } else if is_iou_text && row == 2 {
                    c.is_ascii_graphic()
                } else if !is_iou_text {
                    c.is_ascii_digit() || (c == '.' && !target_row.contains('.'))
                } else {
                    false
                };
                if accept {
                    if is_dest {
                        dest.push(c);
                    } else {
                        target_row.push(c);
                    }
                    true
                } else {
                    false
                }
            }
            KeyCode::Backspace => {
                if is_dest {
                    dest.pop();
                } else {
                    target_row.pop();
                }
                true
            }
            _ => false,
        }
    }

    fn handle_account_set_modal_keys(&mut self, key: KeyEvent) -> Option<Action> {
        if self.account_set_edit_keys(&key) {
            return None;
        }
        match key.code {
            KeyCode::Char('e') | KeyCode::Char('E')
                if !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.is_form_editing = !self.is_form_editing;
                return Some(Action::SetKeymapSuppression(true));
            }
            KeyCode::Char('s') | KeyCode::Char('S')
                if key.modifiers.contains(KeyModifiers::CONTROL) || !self.is_form_editing =>
            {
                return self.queue_submit_account_set();
            }
            KeyCode::Char('[') => {
                self.field_row = (self.field_row + ACCOUNT_SET_ROWS - 1) % ACCOUNT_SET_ROWS;
            }
            KeyCode::Char(']') => {
                self.field_row = (self.field_row + 1) % ACCOUNT_SET_ROWS;
            }
            KeyCode::Tab => {
                self.field_row = (self.field_row + 1) % ACCOUNT_SET_ROWS;
            }
            KeyCode::BackTab => {
                self.field_row = (self.field_row + ACCOUNT_SET_ROWS - 1) % ACCOUNT_SET_ROWS;
            }
            KeyCode::Char(',') if self.field_row <= 1 => {
                if self.field_row == 0 {
                    self.set_flag_ix =
                        (self.set_flag_ix + FLAG_OPTIONS.len() - 1) % FLAG_OPTIONS.len();
                } else {
                    self.clear_flag_ix =
                        (self.clear_flag_ix + FLAG_OPTIONS.len() - 1) % FLAG_OPTIONS.len();
                }
            }
            KeyCode::Char('.') if self.field_row <= 1 => {
                if self.field_row == 0 {
                    self.set_flag_ix = (self.set_flag_ix + 1) % FLAG_OPTIONS.len();
                } else {
                    self.clear_flag_ix = (self.clear_flag_ix + 1) % FLAG_OPTIONS.len();
                }
            }
            _ => {}
        }
        None
    }

    fn render_composer(&self, frame: &mut Frame, area: Rect) {
        let Some(phase) = &self.composer else {
            return;
        };

        let popup_w = area.width.clamp(54, 74);
        let popup_h = match phase {
            ComposerPhase::PickKind { .. } => 12u16,
            ComposerPhase::AccountSet => 21u16,
            ComposerPhase::Payment { is_iou, .. } => {
                if *is_iou {
                    20u16
                } else {
                    16u16
                }
            }
        }
        .min(area.height.saturating_sub(2))
        .max(8);

        let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
        let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
        let popup = Rect::new(x, y, popup_w, popup_h);

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
        };
        let block = theme::panel_block(inner_title, true);
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let label = theme::dim_style();
        let value = theme::accent_style();
        let hi = Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED);

        let body = match phase {
            ComposerPhase::PickKind { selected } => {
                let hi0 = if *selected == 0 { hi } else { label };
                let hi1 = if *selected == 1 { hi } else { label };
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
                            if self.field_row == 0 { hi } else { label },
                        ),
                        Span::styled(Self::label_for_flag(self.set_flag_ix), value),
                        Span::raw("  , ."),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "ClearFlag [{}]",
                                if self.field_row == 1 { "*" } else { " " }
                            ),
                            if self.field_row == 1 { hi } else { label },
                        ),
                        Span::styled(Self::label_for_flag(self.clear_flag_ix), value),
                        Span::raw("  , ."),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "Domain(ascii) [{}]",
                                if self.field_row == 2 { "*" } else { " " }
                            ),
                            if self.field_row == 2 { hi } else { label },
                        ),
                        Span::styled(self.domain.clone(), value),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            format!("TickSize [{}]", if self.field_row == 3 { "*" } else { " " }),
                            if self.field_row == 3 { hi } else { label },
                        ),
                        Span::styled(self.tick_size.clone(), value),
                        Span::raw("  0 or 3–15, empty=skip"),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "TransferRate [{}]",
                                if self.field_row == 4 { "*" } else { " " }
                            ),
                            if self.field_row == 4 { hi } else { label },
                        ),
                        Span::styled(self.transfer_rate.clone(), value),
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
                        if *row == 0 { hi } else { labels },
                    ),
                    Span::styled(destination.clone(), value_st),
                ])];

                if *is_iou {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("Currency (3c) [{}]", if *row == 1 { "*" } else { " " }),
                            if *row == 1 { hi } else { labels },
                        ),
                        Span::styled(iou_currency.clone(), value_st),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("Issuer (r…)   [{}]", if *row == 2 { "*" } else { " " }),
                            if *row == 2 { hi } else { labels },
                        ),
                        Span::styled(iou_issuer.clone(), value_st),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("Amount        [{}]", if *row == 3 { "*" } else { " " }),
                            if *row == 3 { hi } else { labels },
                        ),
                        Span::styled(amount.clone(), value_st),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("Amount XRP    [{}]", if *row == 1 { "*" } else { " " }),
                            if *row == 1 { hi } else { labels },
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
        };
        frame.render_widget(body, inner);
    }

    fn render_keygen_popup(&self, frame: &mut Frame, area: Rect) {
        let Some(ref result) = self.keygen_result else {
            return;
        };

        let popup_w = 60u16;
        let popup_h = 11u16;
        let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
        let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
        let popup = Rect::new(x, y, popup_w, popup_h);

        frame.render_widget(Clear, popup);
        let block = theme::panel_block("New Key (wallet_propose)", true);
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let label = theme::dim_style();
        let value = theme::accent_style();
        let warn = theme::warning_style();

        let body = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Seed:   ", label),
                Span::styled(result.master_seed.clone(), warn),
            ]),
            Line::from(vec![
                Span::styled("Addr:   ", label),
                Span::styled(result.account_id.clone(), value),
            ]),
            Line::from(vec![
                Span::styled("PubKey: ", label),
                Span::styled(result.public_key.clone(), theme::secondary_style()),
            ]),
            Line::from(vec![
                Span::styled("Type:   ", label),
                Span::raw(format!(
                    "{}  Seed hex: {}",
                    result.key_type, result.master_seed_hex
                )),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "⚠ Save the seed offline!  Set XRPL_SEED=<seed>",
                warn.add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "   to activate · Esc / g to dismiss",
                theme::secondary_style(),
            )),
        ]);
        frame.render_widget(body, inner);
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
            self.seed_address = Some(seed_to_address(s));
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

    fn handle_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<Option<Action>> {
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
            });
        }

        // 'i' toggles between XRP and IOU payment mode
        if let Some(ComposerPhase::Payment {
            ref mut is_iou,
            ref mut iou_currency,
            ref mut iou_issuer,
            row: _,
            ..
        }) = self.composer
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
            return Ok(Some(self.queue_submit_payment(d, a, cur, iss)));
        }

        match &mut self.composer {
            Some(ComposerPhase::PickKind { selected }) => {
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        *selected = (*selected + 1) % 2;
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        *selected = (*selected + 2 - 1) % 2;
                    }
                    KeyCode::Tab => {
                        *selected = (*selected + 1) % 2;
                    }
                    KeyCode::BackTab => {
                        *selected = (*selected + 2 - 1) % 2;
                    }
                    KeyCode::Enter => match *selected {
                        0 => self.open_account_set_composer(),
                        1 => self.open_payment_composer(),
                        _ => {}
                    },
                    _ => {}
                }
                return Ok(None);
            }
            Some(ComposerPhase::AccountSet) => {
                return Ok(self.handle_account_set_modal_keys(key));
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

        let label = theme::dim_style();
        let value = theme::accent_style();

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
                Span::styled("Account ", label),
                Span::styled(Self::shorten_display(&account.account, 42), value),
            ]),
            Line::from(vec![
                Span::styled("Balance ", label),
                Span::styled(
                    format!(
                        "{} XRP",
                        fmt::fmt_xrp(account.balance_xrp.parse().unwrap_or(0.0))
                    ),
                    value,
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
                let mut spans: Vec<Span> = vec![Span::styled("Flags", label)];
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
                Span::styled("RegKey ", label),
                Span::styled(Self::shorten_display(rk, 36), theme::warning_style()),
            ]));
        }
        if let Some(ref dh) = account.domain_hex {
            let domain_str = Self::decode_domain_hex(dh).unwrap_or_else(|| dh.clone());
            summary.push(Line::from(vec![
                Span::styled("Domain ", label),
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

    #[test]
    fn payment_validate_rejects_invalid_iou_currency() {
        assert!(WalletPanel::payment_validate("rTest", "1", true, "US", "rIssuer").is_err());
        assert!(WalletPanel::payment_validate("rTest", "1", true, "USD", "rIssuer123").is_ok());
    }

    #[test]
    fn open_payment_composer_defaults_amount() {
        let mut panel = WalletPanel::new(false);
        panel.open_payment_composer();
        match &panel.composer {
            Some(ComposerPhase::Payment { amount, is_iou, .. }) => {
                assert_eq!(amount, "1");
                assert!(!is_iou);
            }
            _ => panic!("expected payment composer"),
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
