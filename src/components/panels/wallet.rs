use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            fmt,
            selectable_table::SelectableTableState,
            theme,
            tx_detail::{TxDetailState, render_tx_detail},
            widgets::{
                render_empty, render_error, render_loading, render_tx_scroll_table, titled_block,
            },
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
    tx_table: SelectableTableState,
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
    submit_flash: Option<SubmitFlash>,
    detail: TxDetailState,
    marker: Option<serde_json::Value>,
    has_more: bool,
    loading_more: bool,
    filtered: Option<Vec<TxRow>>,
    filter_mode: bool,
    filter_input: String,
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
            tx_table: SelectableTableState::default(),
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
            submit_flash: None,
            detail: TxDetailState::default(),
            marker: None,
            has_more: false,
            loading_more: false,
            filtered: None,
            filter_mode: false,
            filter_input: String::new(),
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

    fn reapply_filter(&mut self) {
        if self.filter_input.is_empty() {
            self.filtered = None;
        } else {
            let f = self.filter_input.to_lowercase();
            self.filtered = Some(
                self.txs
                    .iter()
                    .filter(|r| {
                        r.tx_type.to_lowercase().contains(&f) || r.hash.to_lowercase().contains(&f)
                    })
                    .cloned()
                    .collect(),
            );
        }
        let count = self
            .filtered
            .as_ref()
            .map(|v| v.len())
            .unwrap_or(self.txs.len());
        self.tx_table.reset_len(count);
    }

    fn row_count(&self) -> usize {
        self.filtered
            .as_ref()
            .map(|v| v.len())
            .unwrap_or(self.txs.len())
    }

    fn display_rows(&self) -> &[TxRow] {
        self.filtered.as_deref().unwrap_or(&self.txs)
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

    fn set_submit_flash(&mut self, flash: SubmitFlash) {
        self.submit_flash = Some(flash);
    }

    fn payment_validate(dest: &str, amt: &str) -> Result<(), &'static str> {
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
        Ok(())
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
            KeyCode::Char('s') | KeyCode::Char('S')
                if key.modifiers.contains(KeyModifiers::CONTROL) || !self.form_edit =>
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
            ComposerPhase::PickKind { .. } => 11u16,
            ComposerPhase::AccountSet => 21u16,
            ComposerPhase::Payment { .. } => 16u16,
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
                amount_xrp,
            } => {
                let net_note = if self.network.is_mainnet() && !self.skip_mainnet_prompt {
                    " · mainnet sends need --yes"
                } else {
                    ""
                };
                let d = destination.trim();
                let a = amount_xrp.trim();
                let (preview_text, preview_st) = if d.is_empty() && a.is_empty() {
                    (
                        "Type destination + XRP amount, then s to send".to_string(),
                        label,
                    )
                } else if d.is_empty() {
                    (
                        "Need destination (classic r… or X-address)".to_string(),
                        theme::warning_style(),
                    )
                } else if a.is_empty() {
                    (
                        "Need amount in XRP (e.g. 1.25)".to_string(),
                        theme::warning_style(),
                    )
                } else {
                    match a.parse::<f64>() {
                        Ok(v) if v > 0.0 => (
                            format!("▸ Send {} XRP → {}", a, Self::shorten_display(d, 30)),
                            theme::success_style(),
                        ),
                        Ok(_) => ("Amount must be > 0".to_string(), theme::warning_style()),
                        Err(_) => (
                            "Amount must be a number".to_string(),
                            theme::warning_style(),
                        ),
                    }
                };
                Paragraph::new(vec![
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
                    Line::from(Span::styled(preview_text, preview_st)),
                    Line::from(Span::styled(
                        format!(
                            "[ ] Tab rows · Enter edit/next field · e toggle type · s send · Esc back{net_note}",
                        ),
                        theme::secondary_style(),
                    )),
                ])
            }
        };
        frame.render_widget(body, inner);
    }
}

impl Component for WalletPanel {
    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
        self.seed = config.xrpl.signing.seed.clone();
        self.network = config.xrpl.network;
        self.config = Some(config);
        if let Some(ref s) = self.seed {
            self.seed_address = Some(seed_to_address(s));
        }
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        // Detail overlay scroll takes precedence when visible
        if self.detail.visible {
            match action {
                Action::TxDetailToggle => {
                    self.detail.close();
                    return Ok(None);
                }
                Action::SelectNext | Action::FocusNext => {
                    self.detail.scroll = self.detail.scroll.saturating_add(1);
                    return Ok(None);
                }
                Action::SelectPrev | Action::FocusPrev => {
                    self.detail.scroll = self.detail.scroll.saturating_sub(1);
                    return Ok(None);
                }
                _ => return Ok(None),
            }
        }

        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplWalletOverview(acc, txs, marker) => {
                self.account = acc.clone();
                self.txs = txs.to_vec();
                self.received = true;
                self.marker = marker.clone();
                self.has_more = marker.is_some();
                self.loading_more = false;
                self.reapply_filter();
            }
            Action::XrplTxHistoryAppend(txs, marker) => {
                self.txs.extend(txs.iter().cloned());
                self.marker = marker.clone();
                self.has_more = marker.is_some();
                self.loading_more = false;
                self.reapply_filter();
            }
            Action::SelectNext
                if self.is_focused && self.composer.is_none() && self.row_count() > 0 =>
            {
                self.tx_table.select_next(self.row_count());
            }
            Action::SelectPrev
                if self.is_focused && self.composer.is_none() && self.row_count() > 0 =>
            {
                self.tx_table.select_prev(self.row_count());
            }
            Action::TxDetailToggle
                if self.is_focused && self.composer.is_none() && self.row_count() > 0 =>
            {
                let rows = self.display_rows();
                if let Some(idx) = self.tx_table.selected()
                    && let Some(tx) = rows.get(idx)
                {
                    self.detail.open(tx.tx_json.clone(), tx.meta_json.clone());
                }
            }
            Action::AccountSetSubmitOk(hash) => {
                self.set_submit_flash(SubmitFlash::Success(format!(
                    "AccountSet submitted · {hash}"
                )));
                if matches!(&self.composer, Some(ComposerPhase::AccountSet)) {
                    self.composer = None;
                    self.form_edit = false;
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
                    self.form_edit = false;
                    return Ok(Some(Action::SetKeymapSuppression(false)));
                }
            }
            Action::PaymentSubmitErr(msg) => {
                self.set_submit_flash(SubmitFlash::Error(format!("Payment · {msg}")));
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

        if self.detail.visible {
            return Ok(None);
        }

        if self.filter_mode {
            match key.code {
                KeyCode::Enter => {
                    self.filter_mode = false;
                }
                KeyCode::Esc => {
                    self.filter_mode = false;
                    self.filter_input.clear();
                    self.reapply_filter();
                }
                KeyCode::Char(c) => {
                    self.filter_input.push(c);
                    self.reapply_filter();
                }
                KeyCode::Backspace => {
                    self.filter_input.pop();
                    self.reapply_filter();
                }
                _ => {}
            }
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

        let payment_submit_pairs = match &self.composer {
            Some(ComposerPhase::Payment {
                destination,
                amount_xrp,
                ..
            }) if matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
                && (key.modifiers.contains(KeyModifiers::CONTROL) || !self.form_edit) =>
            {
                Some((destination.clone(), amount_xrp.clone()))
            }
            _ => None,
        };

        if let Some((d, a)) = payment_submit_pairs {
            match Self::payment_validate(&d, &a) {
                Ok(()) => return Ok(Some(Self::queue_submit_payment(d, a, self))),
                Err(m) => {
                    self.set_submit_flash(SubmitFlash::Error(m.to_string()));
                    return Ok(None);
                }
            }
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
            Some(ComposerPhase::Payment { row, .. }) => {
                match key.code {
                    KeyCode::Char('e') | KeyCode::Char('E')
                        if !key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        self.form_edit = !self.form_edit;
                        if self.form_edit {
                            return Ok(Some(Action::SetKeymapSuppression(true)));
                        }
                    }
                    KeyCode::Enter => {
                        if self.form_edit {
                            *row = (*row + 1) % 2;
                        } else {
                            self.form_edit = true;
                            return Ok(Some(Action::SetKeymapSuppression(true)));
                        }
                    }
                    KeyCode::Char('[') | KeyCode::BackTab => {
                        *row = (*row + 2 - 1) % 2;
                    }
                    KeyCode::Char(']') | KeyCode::Tab => {
                        *row = (*row + 1) % 2;
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
            self.form_edit = false;
            self.composer = Some(ComposerPhase::PickKind { selected: 0 });
            return Ok(Some(Action::SetKeymapSuppression(true)));
        }

        if self.composer.is_none() && key.code == KeyCode::Char('f') && !self.filter_mode {
            self.filter_mode = true;
            self.filter_input.clear();
            self.reapply_filter();
            return Ok(None);
        }

        if self.composer.is_none()
            && key.code == KeyCode::Char('m')
            && self.has_more
            && !self.loading_more
        {
            self.loading_more = true;
            return Ok(Some(Action::RefreshTxHistoryMore(self.marker.clone())));
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
                Span::raw(fmt::group_digits_u64(u64::from(account.sequence))),
            ]),
            Line::from(Span::styled("t: composer (AccountSet / Payment)", label)),
        ];
        frame.render_widget(Paragraph::new(lines), top);

        let row_count = self.row_count();
        if row_count == 0 {
            let msg = if self.filter_input.is_empty() {
                "None"
            } else {
                "no matches"
            };
            render_empty(frame, bottom, "Recent Transactions", msg, false);
        } else {
            let [tx_hdr, tx_body] =
                Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(bottom);
            let title_text = if self.filter_input.is_empty() {
                "Recent txs ".to_string()
            } else {
                format!("Recent txs [filter: {}] ", self.filter_input)
            };
            let tx_title = Line::from(vec![
                Span::styled(
                    title_text,
                    theme::secondary_style().add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("({})  ", row_count), theme::dim_style()),
                Span::styled(
                    if self.filter_mode {
                        "Filter: typing…"
                    } else if self.loading_more {
                        "loading more…"
                    } else if self.has_more {
                        "j/k scroll · ▲▼ · f: filter · m: more"
                    } else {
                        "j/k scroll · ▲▼ · f: filter"
                    },
                    theme::dim_style(),
                ),
            ]);
            frame.render_widget(Paragraph::new(tx_title), tx_hdr);

            if let Some(ref filtered) = self.filtered {
                render_tx_scroll_table(
                    frame,
                    tx_body,
                    filtered,
                    &mut self.tx_table,
                    self.is_focused,
                );
            } else {
                render_tx_scroll_table(
                    frame,
                    tx_body,
                    &self.txs,
                    &mut self.tx_table,
                    self.is_focused,
                );
            }
        }

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
        render_tx_detail(frame, area, &self.detail);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xrpl::ArcValue;

    fn dummy_tx_row(hash: &str, tx_type: &str) -> TxRow {
        TxRow {
            hash: hash.to_string(),
            tx_type: tx_type.to_string(),
            ledger_index: 1,
            result: "tesSUCCESS".to_string(),
            direction: "·".to_string(),
            tx_json: ArcValue::new(serde_json::json!({"hash": hash, "TransactionType": tx_type})),
            meta_json: ArcValue::new(serde_json::json!({})),
        }
    }

    #[test]
    fn wallet_filter_by_tx_type() {
        let mut panel = WalletPanel::new(false);
        panel.txs = vec![
            dummy_tx_row("aaa", "Payment"),
            dummy_tx_row("bbb", "OfferCreate"),
            dummy_tx_row("ccc", "Payment"),
        ];
        panel.filter_input = "offer".to_string();
        panel.reapply_filter();
        assert_eq!(panel.row_count(), 1);
        assert_eq!(panel.filtered.as_ref().unwrap()[0].tx_type, "OfferCreate");
    }

    #[test]
    fn wallet_filter_by_hash() {
        let mut panel = WalletPanel::new(false);
        panel.txs = vec![
            dummy_tx_row("deadbeef", "Payment"),
            dummy_tx_row("cafebabe", "AccountSet"),
        ];
        panel.filter_input = "dead".to_string();
        panel.reapply_filter();
        assert_eq!(panel.row_count(), 1);
        assert_eq!(panel.filtered.as_ref().unwrap()[0].hash, "deadbeef");
    }
}
