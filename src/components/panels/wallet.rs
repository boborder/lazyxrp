use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Row, Table},
};

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
    xrpl::{AccountSetSubmitParams, AccountSummary, PaymentSubmitParams, TxRow},
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
enum ComposerPhase {
    PickKind {
        selected: usize,
    },
    AccountSet,
    Payment {
        row: usize,
        destination: String,
        amount_xrp: String,
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
    txs: Vec<TxRow>,
    tick: usize,
    received: bool,
    seed: Option<String>,
    seed_address: Option<Result<String, String>>,
    pub is_focused: bool,
    skip_mainnet_prompt: bool,
    network: Network,
    config: Option<Arc<Config>>,
    composer: Option<ComposerPhase>,
    /// Suppresses global `h`/`l` while the tx composer modal is open or a field is being edited.
    form_edit: bool,
    field_row: usize,
    set_flag_ix: usize,
    clear_flag_ix: usize,
    domain: String,
    tick_size: String,
    transfer_rate: String,
    submit_note: Option<String>,
}

impl Default for WalletPanel {
    fn default() -> Self {
        Self {
            account: None,
            txs: Vec::new(),
            tick: 0,
            received: false,
            seed: None,
            seed_address: None,
            is_focused: false,
            skip_mainnet_prompt: false,
            network: Network::Mainnet,
            config: None,
            composer: None,
            form_edit: false,
            field_row: 0,
            set_flag_ix: 0,
            clear_flag_ix: 0,
            domain: String::new(),
            tick_size: String::new(),
            transfer_rate: String::new(),
            submit_note: None,
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

    fn queue_submit_account_set(&mut self) -> Option<Action> {
        let config_seed = self
            .config
            .as_ref()
            .and_then(|c| c.xrpl.signing.seed.clone());
        let set = Self::label_for_flag(self.set_flag_ix);
        let clr = Self::label_for_flag(self.clear_flag_ix);
        Some(Action::AccountSetSubmit(AccountSetSubmitParams {
            set_flag: if set == "(none)" { None } else { Some(set) },
            clear_flag: if clr == "(none)" { None } else { Some(clr) },
            domain_ascii: self.domain.clone(),
            tick_size: self.tick_size.clone(),
            transfer_rate: self.transfer_rate.clone(),
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            config_seed,
        }))
    }

    fn queue_submit_payment(dest: String, amt: String, panel: &Self) -> Action {
        let config_seed = panel
            .config
            .as_ref()
            .and_then(|c| c.xrpl.signing.seed.clone());
        Action::PaymentSubmit(PaymentSubmitParams {
            destination: dest,
            amount_xrp: amt,
            skip_mainnet_prompt: panel.skip_mainnet_prompt,
            config_seed,
        })
    }

    fn push_submit_feedback_line(&mut self, note: String) {
        self.submit_note = Some(note);
    }

    fn account_set_edit_keys(&mut self, key: &KeyEvent) -> bool {
        if !self.form_edit || self.field_row < 2 {
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

    fn payment_edit_keys(dest: &mut String, amt: &mut String, row: usize, key: &KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if row == 0 && c.is_ascii_graphic() {
                    dest.push(c);
                    true
                } else if row == 1 && (c.is_ascii_digit() || (c == '.' && !amt.contains('.'))) {
                    amt.push(c);
                    true
                } else {
                    false
                }
            }
            KeyCode::Backspace => {
                if row == 0 {
                    dest.pop();
                } else {
                    amt.pop();
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
                self.form_edit = !self.form_edit;
                return Some(Action::SetKeymapSuppression(true));
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) || !self.form_edit {
                    return self.queue_submit_account_set();
                }
            }
            KeyCode::Char('[') => {
                self.field_row = (self.field_row + ACCOUNT_SET_ROWS - 1) % ACCOUNT_SET_ROWS;
            }
            KeyCode::Char(']') => {
                self.field_row = (self.field_row + 1) % ACCOUNT_SET_ROWS;
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

        let popup_w = area.width.min(74).max(54);
        let popup_h = match phase {
            ComposerPhase::PickKind { .. } => 11u16,
            ComposerPhase::AccountSet => 21u16,
            ComposerPhase::Payment { .. } => 13u16,
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
            ComposerPhase::Payment { .. } => "Payment (XRP)",
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
                    Line::from(Span::styled("j/k or ↑/↓ · Enter open · Esc close", label)),
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
                        format!("[ ] row · e edit · s / ^S submit · Esc ← type picker{net_note}"),
                        label,
                    )),
                ])
            }
            ComposerPhase::Payment {
                row,
                destination,
                amount_xrp,
            } => Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!("Destination [{}]", if *row == 0 { "*" } else { " " }),
                        if *row == 0 { hi } else { label },
                    ),
                    Span::styled(destination.clone(), value),
                ]),
                Line::from(vec![
                    Span::styled(
                        format!("Amount XRP [{}]", if *row == 1 { "*" } else { " " }),
                        if *row == 1 { hi } else { label },
                    ),
                    Span::styled(amount_xrp.clone(), value),
                ]),
                Line::from(""),
                Line::from(Span::styled(
                    "[ ] row · e edit · s / ^S submit · Esc ← type picker · tf flags N/A here",
                    label,
                )),
            ]),
        };
        frame.render_widget(body, inner);
    }
}

impl Component for WalletPanel {
    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
        self.seed = config.xrpl.signing.seed.clone();
        self.network = config.xrpl.network.clone();
        self.config = Some(config);
        if let Some(ref s) = self.seed {
            self.seed_address = Some(seed_to_address(s));
        }
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplWalletOverview(acc, txs) => {
                self.account = acc.clone();
                self.txs = txs.to_vec();
                self.received = true;
            }
            Action::AccountSetSubmitOk(hash) => {
                self.push_submit_feedback_line(format!("AccountSet submitted · {hash}"));
            }
            Action::AccountSetSubmitErr(msg) => {
                self.push_submit_feedback_line(format!("AccountSet error · {msg}"));
            }
            Action::PaymentSubmitOk(hash) => {
                self.push_submit_feedback_line(format!("Payment submitted · {hash}"));
            }
            Action::PaymentSubmitErr(msg) => {
                self.push_submit_feedback_line(format!("Payment error · {msg}"));
            }
            Action::NetworkChange(net) => {
                self.network = net.clone();
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
            if self.form_edit {
                self.form_edit = false;
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

        if let Some(ComposerPhase::Payment {
            row,
            ref mut destination,
            ref mut amount_xrp,
        }) = self.composer
            && self.form_edit
            && Self::payment_edit_keys(destination, amount_xrp, row, &key)
        {
            return Ok(None);
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
                    KeyCode::Enter => match *selected {
                        0 => {
                            self.composer = Some(ComposerPhase::AccountSet);
                            self.form_edit = false;
                        }
                        1 => {
                            self.composer = Some(ComposerPhase::Payment {
                                row: 0,
                                destination: String::new(),
                                amount_xrp: String::new(),
                            });
                            self.form_edit = false;
                        }
                        _ => {}
                    },
                    _ => {}
                }
                return Ok(None);
            }
            Some(ComposerPhase::AccountSet) => {
                return Ok(self.handle_account_set_modal_keys(key));
            }
            Some(ComposerPhase::Payment {
                row,
                destination,
                amount_xrp,
            }) => {
                if !self.form_edit {
                    match key.code {
                        KeyCode::Char('e') | KeyCode::Char('E')
                            if !key.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            self.form_edit = true;
                            return Ok(Some(Action::SetKeymapSuppression(true)));
                        }
                        KeyCode::Char('[') => {
                            *row = (*row + 2 - 1) % 2;
                        }
                        KeyCode::Char(']') => {
                            *row = (*row + 1) % 2;
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) || !self.form_edit {
                                return Ok(Some(Self::queue_submit_payment(
                                    destination.clone(),
                                    amount_xrp.clone(),
                                    self,
                                )));
                            }
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('e') | KeyCode::Char('E')
                            if !key.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            self.form_edit = false;
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                return Ok(Some(Self::queue_submit_payment(
                                    destination.clone(),
                                    amount_xrp.clone(),
                                    self,
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                return Ok(None);
            }
            None => {}
        }

        if self.composer.is_none()
            && matches!(key.code, KeyCode::Char('t') | KeyCode::Char('T'))
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.form_edit = false;
            self.composer = Some(ComposerPhase::PickKind { selected: 0 });
            return Ok(Some(Action::SetKeymapSuppression(true)));
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

        if !self.received {
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

        let [top, bottom, hint] = Layout::vertical([
            Constraint::Length(4),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(inner);

        let lines = vec![
            Line::from(vec![
                Span::styled("Account: ", label),
                Span::styled(account.account.clone(), value),
            ]),
            Line::from(vec![
                Span::styled("Balance: ", label),
                Span::styled(
                    format!(
                        "{} XRP",
                        fmt::fmt_xrp(account.balance_xrp.parse().unwrap_or(0.0))
                    ),
                    value,
                ),
            ]),
            Line::from(vec![
                Span::styled("Sequence: ", label),
                Span::raw(fmt::group_digits(&account.sequence.to_string())),
            ]),
            Line::from(Span::styled("t: composer (AccountSet / Payment)", label)),
        ];
        frame.render_widget(Paragraph::new(lines), top);

        if self.txs.is_empty() {
            render_empty(frame, bottom, "Recent Transactions", "None", false);
        } else {
            let header =
                Row::new(vec!["Hash", "Type", "Ledger", "Result"]).style(theme::header_row_style());
            let rows = self.txs.iter().map(|t| {
                let short_hash = if t.hash.len() > 16 {
                    format!("{}…", &t.hash[..16])
                } else {
                    t.hash.clone()
                };
                Row::new(vec![
                    short_hash,
                    t.tx_type.clone(),
                    fmt::group_digits(&t.ledger_index.to_string()),
                    t.result.clone(),
                ])
            });
            let table = Table::new(
                rows,
                [
                    Constraint::Length(18),
                    Constraint::Length(16),
                    Constraint::Length(10),
                    Constraint::Fill(1),
                ],
            )
            .header(header);
            frame.render_widget(table, bottom);
        }

        let note_line = self.submit_note.as_ref().map_or(Line::default(), |n| {
            Line::from(Span::styled(n.clone(), theme::accent_style()))
        });
        frame.render_widget(Paragraph::new(note_line), hint);

        self.render_composer(frame, area);
        Ok(())
    }
}
