use std::{sync::Arc, time::Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    action::Action,
    components::{
        Component,
        shared::{
            fmt, theme,
            widgets::{render_empty, render_error, render_loading},
        },
    },
    config::Config,
    network::Network,
    xrpl::{AccountOfferRow, AccountSummary, EscrowRow, GeneratedWalletKeys, txspec::TxSpec},
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
enum DangerResume {
    AccountSet,
    SetRegularKey,
    /// Destination restored on Esc, submitted on confirm.
    AccountDelete {
        destination: String,
    },
    /// Spec'd transaction with a confirmation phrase (`TxSpec::confirm`); Esc restores the form.
    Tx {
        spec: &'static TxSpec,
        values: Vec<String>,
    },
}

#[derive(Clone, Debug)]
enum ComposerPhase {
    PickKind {
        selected: usize,
    },
    /// Type-to-confirm for destructive AccountSet / SetRegularKey clear / AccountDelete.
    DangerConfirm {
        expected: String,
        hint: String,
        typed: String,
        resume: DangerResume,
    },
    AccountSet,
    Payment {
        row: usize,
        destination: String,
        amount: String,
        iou_currency: String,
        iou_issuer: String,
        /// Optional SendMax spec → cross-currency payment (default paths).
        send_max: String,
        is_iou: bool,
    },
    /// Empty `regular_key` clears the existing regular key (dangerous).
    SetRegularKey {
        regular_key: String,
    },
    OfferCreate {
        row: usize,
        taker_gets: String,
        taker_pays: String,
    },
    TrustSet {
        row: usize,
        currency: String,
        issuer: String,
        limit: String,
    },
    /// Pay Core Vault with 32-byte Direct Mint memo (C2).
    FxrpDirectMint {
        row: usize,
        flare_recipient: String,
        amount_xrp: String,
    },
    /// Paste FDC proof JSON and call Flare executeDirectMinting (C3).
    FxrpExecuteDirectMint {
        proof_json: String,
    },
    /// Cancel one of the signing account's open offers (`offers`: `None` = loading).
    OfferCancel {
        offers: Option<Result<Vec<AccountOfferRow>, String>>,
        selected: usize,
    },
    /// Lock XRP until a relative time (`FinishAfter`), optional refund deadline (`CancelAfter`).
    EscrowCreate {
        row: usize,
        destination: String,
        amount_xrp: String,
        finish_after: String,
        cancel_after: String,
    },
    /// Finish or cancel an escrow the signing account owns or receives (`None` = loading).
    Escrows {
        escrows: Option<Result<Vec<EscrowRow>, String>>,
        selected: usize,
    },
    /// Two-asset AMMDeposit: both amount specs are maxima and name the pool.
    AmmDeposit {
        row: usize,
        amount: String,
        amount2: String,
    },
    /// AMMWithdraw: pool assets; empty `lp_amount` = withdraw all.
    AmmWithdraw {
        row: usize,
        asset: String,
        asset2: String,
        lp_amount: String,
    },
    /// AccountDelete: classic or X-address; submit goes through DangerConfirm (DELETE).
    AccountDelete {
        destination: String,
    },
    /// Smart Account instruction: `0x01` FXRP transfer (`redeem = false`) / `0x02` redeem.
    /// `amount` is a decimal FXRP amount (converted with the read `decimals()`) for transfer,
    /// whole lots for redeem.
    SmartAccount {
        row: usize,
        redeem: bool,
        recipient: String,
        amount: String,
    },
    /// Pick a spec'd transaction type (`xrpl::txspec` registry).
    TxPick {
        selected: usize,
    },
    /// Generic form generated from `spec.fields` (`values[i]` ↔ `spec.fields[i]`).
    Tx {
        spec: &'static TxSpec,
        row: usize,
        values: Vec<String>,
    },
}

const COMPOSER_KIND_COUNT: usize = 15;

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
    /// Debug form of the last queued write; an identical one is refused until a result arrives.
    pending_submit: Option<String>,
    /// The open composer queued `pending_submit`: its Ok result may close it. Cleared when the
    /// user backs out, so a later composer is never closed by an earlier submit's result.
    submit_in_composer: bool,
    /// When the Flare snapshots below last arrived (stale ones refuse to submit).
    fxrp_direct_mint_at: Option<Instant>,
    smart_account_at: Option<Instant>,
    /// Last AssetManager C1 snapshot (Core Vault address for Direct Mint).
    fxrp_direct_mint: Option<crate::xrpl::FxrpDirectMintInfo>,
    /// Last Smart Account read (personal account, balance, operator, fees).
    smart_account: Option<crate::xrpl::SmartAccountInfo>,
    /// Key generation result overlay (GenerateWalletKeysOk → show, Esc to dismiss).
    keygen_result: Option<GeneratedWalletKeys>,
}

impl Default for WalletPanel {
    fn default() -> Self {
        Self {
            account: None,
            tick: 0,
            has_received_wallet_data: false,
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
            pending_submit: None,
            submit_in_composer: false,
            fxrp_direct_mint_at: None,
            smart_account_at: None,
            fxrp_direct_mint: None,
            smart_account: None,
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

    /// The signing wallet's address (or why it has none); `None` = no wallet configured.
    pub fn with_seed_address(mut self, seed_address: Option<Result<String, String>>) -> Self {
        self.seed_address = seed_address;
        self
    }

    fn label_for_flag(ix: usize) -> String {
        FLAG_OPTIONS.get(ix).unwrap_or(&"(none)").to_string()
    }
}

/// Ctrl+S (or bare S outside form editing) submits the open composer.
fn submit_shortcut(key: &KeyEvent, is_form_editing: bool) -> bool {
    matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
        && (key.modifiers.contains(KeyModifiers::CONTROL) || !is_form_editing)
}

fn submit_err(action: &Action) -> Option<(&'static str, &String)> {
    Some(match action {
        Action::AccountSetSubmitErr(m) => ("AccountSet", m),
        Action::PaymentSubmitErr(m) => ("Payment", m),
        Action::SetRegularKeySubmitErr(m) => ("SetRegularKey", m),
        Action::AccountDeleteSubmitErr(m) => ("AccountDelete", m),
        Action::GenericTxSubmitErr(m) => ("Transaction", m),
        Action::OfferCreateSubmitErr(m) => ("OfferCreate", m),
        Action::OfferCancelSubmitErr(m) => ("OfferCancel", m),
        Action::EscrowCreateSubmitErr(m) => ("EscrowCreate", m),
        Action::EscrowCloseSubmitErr(m) => ("Escrow", m),
        Action::AmmSubmitErr(m) => ("AMM", m),
        Action::TrustSetSubmitErr(m) => ("TrustSet", m),
        Action::FxrpDirectMintPaymentSubmitErr(m) => ("FXRP Mint", m),
        Action::FxrpExecuteDirectMintSubmitErr(m) => ("FXRP Execute", m),
        Action::SmartAccountInstructionSubmitErr(m) => ("Smart Account", m),
        Action::GenerateWalletKeysErr(m) => ("Keygen", m),
        _ => return None,
    })
}

impl Component for WalletPanel {
    fn register_config_handler(&mut self, config: Arc<Config>) -> color_eyre::Result<()> {
        self.network = config.xrpl.network;
        self.config = Some(config);
        Ok(())
    }

    fn update(&mut self, action: &Action) -> color_eyre::Result<Option<Action>> {
        if is_write_result(action) {
            self.pending_submit = None;
        }
        if let Some((label, msg)) = submit_err(action) {
            self.set_submit_flash(SubmitFlash::Error(format!("{label} · {msg}")));
            return Ok(None);
        }
        match action {
            Action::Tick => self.tick = self.tick.wrapping_add(1),
            Action::XrplWalletOverview(acc) => {
                self.account = acc.clone();
                self.has_received_wallet_data = true;
            }
            Action::AccountSetSubmitOk(hash) => {
                return self.submit_ok(
                    format!("AccountSet submitted · {hash}"),
                    matches!(&self.composer, Some(ComposerPhase::AccountSet)),
                );
            }
            Action::PaymentSubmitOk(hash) => {
                return self.submit_ok(
                    format!("Payment sent · {hash} (see Recent Transactions below)"),
                    matches!(&self.composer, Some(ComposerPhase::Payment { .. })),
                );
            }
            Action::PaymentQuoteOk(spec) => {
                if let Some(ComposerPhase::Payment { send_max, .. }) = &mut self.composer {
                    *send_max = spec.clone();
                    self.set_submit_flash(SubmitFlash::Success(format!(
                        "Quote · Send max {spec} (incl. slippage) — review, then s"
                    )));
                }
            }
            Action::PaymentQuoteErr(msg) => {
                self.set_submit_flash(SubmitFlash::Error(format!("Quote · {msg}")));
            }
            Action::SetRegularKeySubmitOk(hash) => {
                return self.submit_ok(
                    format!("SetRegularKey submitted · {hash}"),
                    matches!(&self.composer, Some(ComposerPhase::SetRegularKey { .. })),
                );
            }
            Action::AccountDeleteSubmitOk(hash) => {
                return self.submit_ok(
                    format!("AccountDelete submitted · {hash}"),
                    matches!(&self.composer, Some(ComposerPhase::AccountDelete { .. })),
                );
            }
            Action::OfferCreateSubmitOk(hash) => {
                return self.submit_ok(
                    format!("OfferCreate submitted · {hash}"),
                    matches!(&self.composer, Some(ComposerPhase::OfferCreate { .. })),
                );
            }
            Action::XrplAccountOffers(rows) => {
                if let Some(ComposerPhase::OfferCancel { offers, selected }) = &mut self.composer {
                    *selected = (*selected).min(rows.len().saturating_sub(1));
                    *offers = Some(Ok(rows.clone()));
                }
            }
            Action::XrplError(e) if e.starts_with("account_offers") => {
                if let Some(ComposerPhase::OfferCancel { offers, .. }) = &mut self.composer {
                    *offers = Some(Err(e.clone()));
                }
            }
            Action::OfferCancelSubmitOk(hash) => {
                return self.submit_ok(
                    format!("OfferCancel submitted · {hash}"),
                    matches!(&self.composer, Some(ComposerPhase::OfferCancel { .. })),
                );
            }
            Action::XrplAccountEscrows(rows) => {
                if let Some(ComposerPhase::Escrows { escrows, selected }) = &mut self.composer {
                    *selected = (*selected).min(rows.len().saturating_sub(1));
                    *escrows = Some(Ok(rows.clone()));
                }
            }
            Action::XrplError(e) if e.starts_with("account_escrows") => {
                if let Some(ComposerPhase::Escrows { escrows, .. }) = &mut self.composer {
                    *escrows = Some(Err(e.clone()));
                }
            }
            Action::EscrowCreateSubmitOk(hash) => {
                return self.submit_ok(
                    format!("EscrowCreate submitted · {hash}"),
                    matches!(&self.composer, Some(ComposerPhase::EscrowCreate { .. })),
                );
            }
            Action::EscrowCloseSubmitOk(hash) => {
                return self.submit_ok(
                    format!("Escrow closed · {hash}"),
                    matches!(&self.composer, Some(ComposerPhase::Escrows { .. })),
                );
            }
            Action::AmmSubmitOk(msg) => {
                return self.submit_ok(
                    format!("{msg} submitted"),
                    matches!(
                        &self.composer,
                        Some(ComposerPhase::AmmDeposit { .. } | ComposerPhase::AmmWithdraw { .. })
                    ),
                );
            }
            Action::TrustSetSubmitOk(hash) => {
                return self.submit_ok(
                    format!("TrustSet submitted · {hash}"),
                    matches!(&self.composer, Some(ComposerPhase::TrustSet { .. })),
                );
            }
            Action::FxrpDirectMintInfo(info) => {
                self.fxrp_direct_mint = Some((**info).clone());
                self.fxrp_direct_mint_at = Some(Instant::now());
            }
            Action::FxrpDirectMintPaymentSubmitOk(hash) => {
                return self.submit_ok(
                    format!(
                        "FXRP Direct Mint Payment · {hash} · next: composer [7] Execute if flare.fassets.execute=true"
                    ),
                    matches!(&self.composer, Some(ComposerPhase::FxrpDirectMint { .. })),
                );
            }
            Action::FxrpExecuteDirectMintSubmitOk(hash) => {
                return self.submit_ok(
                    format!("FXRP executeDirectMinting · {hash}"),
                    matches!(
                        &self.composer,
                        Some(ComposerPhase::FxrpExecuteDirectMint { .. })
                    ),
                );
            }
            Action::SmartAccountInfo(info) => {
                self.smart_account = Some((**info).clone());
                self.smart_account_at = Some(Instant::now());
            }
            Action::SmartAccountInstructionSubmitOk(hash) => {
                return self.submit_ok(
                    format!(
                        "Smart Account instruction sent · {hash} · the operator executes it on Flare; check the personal-account FXRP balance on Overview"
                    ),
                    matches!(&self.composer, Some(ComposerPhase::SmartAccount { .. })),
                );
            }
            Action::GenerateWalletKeysOk(result) => {
                self.keygen_result = Some(result.clone());
            }
            Action::GenericTxSubmitOk(msg) => {
                return self.submit_ok(
                    format!("{msg} submitted"),
                    matches!(&self.composer, Some(ComposerPhase::Tx { .. })),
                );
            }
            Action::TxComposerOpen { tx_type, fields } => {
                if matches!(self.seed_address, Some(Ok(_)))
                    && let Some(spec) = crate::xrpl::txspec::lookup(tx_type)
                {
                    self.open_tx_composer(spec, fields);
                    self.submit_in_composer = false;
                    return Ok(Some(Action::SetKeymapSuppression(true)));
                }
            }
            Action::NetworkChange(net) => {
                self.network = *net;
                self.keygen_result = None;
                // Flare reads belong to the previous network's session; never submit from them.
                self.fxrp_direct_mint = None;
                self.fxrp_direct_mint_at = None;
                self.smart_account = None;
                self.smart_account_at = None;
            }
            _ => {}
        }
        Ok(None)
    }

    fn on_key_event(&mut self, key: KeyEvent) -> color_eyre::Result<Option<Action>> {
        let out = self.key_action(key)?;
        Ok(self.guard_double_submit(out))
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if self.seed_address.is_none() {
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

        // Modals (keygen result, an already-open composer) must stay visible while the wallet
        // body is still loading or the account is unfunded: `g` works for an unfunded seed.
        if !self.has_received_wallet_data {
            render_loading(
                frame,
                area,
                "Wallet",
                self.tick,
                "loading...",
                self.is_focused,
            );
            self.render_composer(frame, area);
            self.render_keygen_popup(frame, area);
            return Ok(());
        }

        let Some(account) = self.account.as_ref() else {
            let msg = "account not found on this network (unfunded or deleted) — g new keys";
            render_empty(frame, area, "Wallet", msg, self.is_focused);
            self.render_composer(frame, area);
            self.render_keygen_popup(frame, area);
            return Ok(());
        };

        let block = theme::panel_block("Wallet", self.is_focused);
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
                Span::styled(fmt::truncate_middle(&account.account, 42), value_style),
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
                    "  ·  t send/configure  g new keys",
                    theme::dim_style(),
                ));
                Line::from(spans)
            },
        ];
        if let Some(ref rk) = account.regular_key {
            summary.push(Line::from(vec![
                Span::styled("RegKey ", label_style),
                Span::styled(fmt::truncate_middle(rk, 36), theme::warning_style()),
            ]));
        }
        if let Some(ref dh) = account.domain_hex {
            let domain_str = crate::xrpl::hex_to_ascii(dh.trim()).unwrap_or_else(|| dh.clone());
            summary.push(Line::from(vec![
                Span::styled("Domain ", label_style),
                Span::styled(domain_str, theme::dim_style()),
            ]));
        }
        summary.push(Line::from(Span::styled(
            "Recent txs below · j/k select · f filter · Enter detail",
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

impl WalletPanel {
    /// Flash a submit success; when `closes_composer`, also dismiss it and release keymap suppression.
    /// Only a composer that queued the in-flight submit closes: a composer the user re-opened
    /// after backing out of the submitted one stays put.
    fn submit_ok(
        &mut self,
        text: String,
        closes_composer: bool,
    ) -> color_eyre::Result<Option<Action>> {
        self.set_submit_flash(SubmitFlash::Success(text));
        if !closes_composer || !self.submit_in_composer {
            return Ok(None);
        }
        self.submit_in_composer = false;
        self.composer = None;
        self.is_form_editing = false;
        Ok(Some(Action::SetKeymapSuppression(false)))
    }

    /// Text field the cursor is in for every phase that types plain characters. Payment and
    /// AccountSet filter per row (`payment_edit_keys` / `account_set_edit_keys`); DangerConfirm
    /// is type-to-confirm and never receives pasted text.
    fn edit_target(&mut self) -> Option<&mut String> {
        Some(match self.composer.as_mut()? {
            ComposerPhase::SetRegularKey { regular_key } => regular_key,
            ComposerPhase::OfferCreate {
                row,
                taker_gets,
                taker_pays,
            } => {
                if *row == 0 {
                    taker_gets
                } else {
                    taker_pays
                }
            }
            ComposerPhase::TrustSet {
                row,
                currency,
                issuer,
                limit,
            } => match *row {
                0 => currency,
                1 => issuer,
                _ => limit,
            },
            ComposerPhase::FxrpDirectMint {
                row,
                flare_recipient,
                amount_xrp,
            } => {
                if *row == 0 {
                    flare_recipient
                } else {
                    amount_xrp
                }
            }
            ComposerPhase::FxrpExecuteDirectMint { proof_json } => proof_json,
            ComposerPhase::SmartAccount {
                row,
                redeem,
                recipient,
                amount,
            } => {
                if *redeem || *row == 1 {
                    amount
                } else {
                    recipient
                }
            }
            ComposerPhase::EscrowCreate {
                row,
                destination,
                amount_xrp,
                finish_after,
                cancel_after,
            } => match *row {
                0 => destination,
                1 => amount_xrp,
                2 => finish_after,
                _ => cancel_after,
            },
            ComposerPhase::AmmDeposit {
                row,
                amount,
                amount2,
            } => {
                if *row == 0 {
                    amount
                } else {
                    amount2
                }
            }
            ComposerPhase::AmmWithdraw {
                row,
                asset,
                asset2,
                lp_amount,
            } => match *row {
                0 => asset,
                1 => asset2,
                _ => lp_amount,
            },
            ComposerPhase::AccountDelete { destination } => destination,
            ComposerPhase::Tx { row, values, .. } => values.get_mut(*row)?,
            _ => return None,
        })
    }

    /// Bracketed paste: inserted into the field being edited only. Characters go straight to
    /// the field (same per-row filters as typing), never through the key handlers, so pasted
    /// text cannot trigger hotkeys; newlines and other control characters are dropped.
    pub fn paste(&mut self, text: &str) {
        if !self.is_focused || !self.is_form_editing {
            return;
        }
        let chars = text.chars().filter(|c| !c.is_control());
        if matches!(self.composer, Some(ComposerPhase::AccountSet)) {
            for c in chars {
                self.account_set_edit_keys(&KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
            }
            return;
        }
        if let Some(ComposerPhase::Payment {
            row,
            destination,
            amount,
            iou_currency,
            iou_issuer,
            send_max,
            is_iou,
        }) = &mut self.composer
        {
            for c in chars {
                let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
                Self::payment_edit_keys(
                    destination,
                    amount,
                    iou_currency,
                    iou_issuer,
                    send_max,
                    *is_iou,
                    *row,
                    &key,
                );
            }
            return;
        }
        if let Some(target) = self.edit_target() {
            target.extend(chars);
        }
    }

    fn key_action(&mut self, key: KeyEvent) -> color_eyre::Result<Option<Action>> {
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
            self.submit_in_composer = false;
            return Ok(match &self.composer {
                None => None,
                Some(ComposerPhase::PickKind { .. }) => {
                    self.composer = None;
                    Some(Action::SetKeymapSuppression(false))
                }
                Some(ComposerPhase::Payment { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 0 });
                    None
                }
                Some(ComposerPhase::DangerConfirm { resume, .. }) => {
                    let resume = resume.clone();
                    self.composer = Some(match resume {
                        DangerResume::AccountSet => ComposerPhase::AccountSet,
                        DangerResume::SetRegularKey => ComposerPhase::SetRegularKey {
                            regular_key: String::new(),
                        },
                        DangerResume::AccountDelete { destination } => {
                            ComposerPhase::AccountDelete { destination }
                        }
                        DangerResume::Tx { spec, values } => ComposerPhase::Tx {
                            spec,
                            row: 0,
                            values,
                        },
                    });
                    self.is_form_editing = false;
                    None
                }
                Some(ComposerPhase::AccountSet) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 1 });
                    None
                }
                Some(ComposerPhase::SetRegularKey { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 2 });
                    None
                }
                Some(ComposerPhase::OfferCreate { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 3 });
                    None
                }
                Some(ComposerPhase::TrustSet { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 4 });
                    None
                }
                Some(ComposerPhase::FxrpDirectMint { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 5 });
                    None
                }
                Some(ComposerPhase::FxrpExecuteDirectMint { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 6 });
                    None
                }
                Some(ComposerPhase::OfferCancel { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 7 });
                    None
                }
                Some(ComposerPhase::EscrowCreate { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 8 });
                    None
                }
                Some(ComposerPhase::Escrows { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 9 });
                    None
                }
                Some(ComposerPhase::AmmDeposit { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 10 });
                    None
                }
                Some(ComposerPhase::AmmWithdraw { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 11 });
                    None
                }
                Some(ComposerPhase::AccountDelete { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 12 });
                    None
                }
                Some(ComposerPhase::SmartAccount { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 13 });
                    None
                }
                Some(ComposerPhase::TxPick { .. }) => {
                    self.composer = Some(ComposerPhase::PickKind { selected: 14 });
                    None
                }
                Some(ComposerPhase::Tx { spec, .. }) => {
                    let selected = crate::xrpl::txspec::registry()
                        .position(|s| s.tx_type == spec.tx_type)
                        .unwrap_or(0);
                    self.composer = Some(ComposerPhase::TxPick { selected });
                    None
                }
            });
        }

        // 'i' toggles between XRP and IOU payment mode (disabled while typing)
        if let Some(ComposerPhase::Payment {
            ref mut row,
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
            // Send-max row index differs between modes.
            *row = 0;
            if !*is_iou {
                iou_currency.clear();
                iou_issuer.clear();
            }
            return Ok(None);
        }

        // 'p' quotes Send max via ripple_path_find (disabled while typing).
        if matches!(self.composer, Some(ComposerPhase::Payment { .. }))
            && !self.is_form_editing
            && matches!(key.code, KeyCode::Char('p') | KeyCode::Char('P'))
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            return Ok(self.queue_payment_quote());
        }

        if let Some(ComposerPhase::Payment {
            row,
            ref mut destination,
            ref mut amount,
            ref mut iou_currency,
            ref mut iou_issuer,
            ref mut send_max,
            is_iou,
        }) = self.composer
            && self.is_form_editing
            && Self::payment_edit_keys(
                destination,
                amount,
                iou_currency,
                iou_issuer,
                send_max,
                is_iou,
                row,
                &key,
            )
        {
            return Ok(None);
        }

        if let Some(ComposerPhase::DangerConfirm {
            expected,
            typed,
            resume,
            ..
        }) = &mut self.composer
        {
            match key.code {
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    typed.push(c);
                }
                KeyCode::Backspace => {
                    typed.pop();
                }
                KeyCode::Enter => {
                    if typed.as_str() == expected {
                        let resume = resume.clone();
                        self.is_form_editing = false;
                        let action = match resume {
                            DangerResume::AccountSet => {
                                self.composer = Some(ComposerPhase::AccountSet);
                                Some(self.queue_submit_account_set_confirmed())
                            }
                            DangerResume::SetRegularKey => {
                                self.composer = Some(ComposerPhase::SetRegularKey {
                                    regular_key: String::new(),
                                });
                                Some(self.queue_submit_set_regular_key_confirmed())
                            }
                            DangerResume::AccountDelete { destination } => {
                                let action = self.account_delete_action(destination.clone());
                                self.composer = Some(ComposerPhase::AccountDelete { destination });
                                Some(action)
                            }
                            DangerResume::Tx { spec, values } => {
                                let action = self.tx_action(spec, &values);
                                self.composer = Some(ComposerPhase::Tx {
                                    spec,
                                    row: 0,
                                    values,
                                });
                                Some(action)
                            }
                        };
                        return Ok(action);
                    }
                    let expected = expected.clone();
                    self.set_submit_flash(SubmitFlash::Error(format!(
                        "confirmation mismatch — type {expected} exactly"
                    )));
                }
                _ => {}
            }
            return Ok(None);
        }

        if matches!(&self.composer, Some(ComposerPhase::SetRegularKey { .. }))
            && submit_shortcut(&key, self.is_form_editing)
        {
            return Ok(self.queue_submit_set_regular_key());
        }

        if matches!(&self.composer, Some(ComposerPhase::AccountDelete { .. }))
            && submit_shortcut(&key, self.is_form_editing)
        {
            return Ok(self.queue_submit_account_delete());
        }

        if matches!(&self.composer, Some(ComposerPhase::Tx { .. }))
            && submit_shortcut(&key, self.is_form_editing)
        {
            return Ok(self.queue_submit_tx());
        }

        if matches!(&self.composer, Some(ComposerPhase::OfferCreate { .. }))
            && submit_shortcut(&key, self.is_form_editing)
        {
            return Ok(Some(self.queue_submit_offer_create()));
        }

        if matches!(&self.composer, Some(ComposerPhase::OfferCancel { .. }))
            && submit_shortcut(&key, self.is_form_editing)
        {
            return Ok(self.queue_submit_offer_cancel());
        }

        if matches!(&self.composer, Some(ComposerPhase::EscrowCreate { .. }))
            && submit_shortcut(&key, self.is_form_editing)
        {
            return Ok(self.queue_submit_escrow_create());
        }

        if matches!(
            &self.composer,
            Some(ComposerPhase::AmmDeposit { .. } | ComposerPhase::AmmWithdraw { .. })
        ) && submit_shortcut(&key, self.is_form_editing)
        {
            return Ok(self.queue_submit_amm());
        }

        if matches!(&self.composer, Some(ComposerPhase::Escrows { .. })) {
            if submit_shortcut(&key, false) {
                return Ok(self.queue_submit_escrow_close(false));
            }
            if matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
                && !key.modifiers.contains(KeyModifiers::CONTROL)
            {
                return Ok(self.queue_submit_escrow_close(true));
            }
        }

        if matches!(&self.composer, Some(ComposerPhase::TrustSet { .. }))
            && submit_shortcut(&key, self.is_form_editing)
        {
            return Ok(self.queue_submit_trust_set());
        }

        if matches!(&self.composer, Some(ComposerPhase::FxrpDirectMint { .. }))
            && submit_shortcut(&key, self.is_form_editing)
        {
            return Ok(Some(self.queue_submit_fxrp_direct_mint()));
        }

        if matches!(
            &self.composer,
            Some(ComposerPhase::FxrpExecuteDirectMint { .. })
        ) && submit_shortcut(&key, self.is_form_editing)
        {
            return Ok(Some(self.queue_submit_fxrp_execute_direct_mint()));
        }

        if matches!(&self.composer, Some(ComposerPhase::SmartAccount { .. }))
            && submit_shortcut(&key, self.is_form_editing)
        {
            return Ok(Some(self.queue_submit_smart_account()));
        }

        if self.is_form_editing
            && let Some(target) = self.edit_target()
        {
            match key.code {
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    target.push(c);
                    return Ok(None);
                }
                KeyCode::Backspace => {
                    target.pop();
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
                send_max,
                is_iou,
                ..
            }) if submit_shortcut(&key, self.is_form_editing) => Some((
                destination.clone(),
                amount.clone(),
                iou_currency.clone(),
                iou_issuer.clone(),
                send_max.clone(),
                *is_iou,
            )),
            _ => None,
        };

        if let Some((d, a, cur, iss, smax, iou)) = payment_submit_pairs {
            if let Err(m) = Self::payment_validate(&d, &a, iou, &cur, &iss, Some(&smax)) {
                self.set_submit_flash(SubmitFlash::Error(m));
                return Ok(None);
            }
            // XRP mode must not leak leftover IOU fields into PaymentSubmitParams.
            let (cur, iss) = if iou {
                (cur, iss)
            } else {
                (String::new(), String::new())
            };
            return Ok(Some(self.queue_submit_payment(d, a, cur, iss, smax)));
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
                        0 => self.open_payment_composer(),
                        1 => self.open_account_set_composer(),
                        2 => self.open_set_regular_key_composer(),
                        3 => self.open_offer_create_composer(),
                        4 => self.open_trust_set_composer(),
                        5 => self.open_fxrp_direct_mint_composer(),
                        6 => self.open_fxrp_execute_direct_mint_composer(),
                        7 => return Ok(Some(self.open_offer_cancel_composer())),
                        8 => self.open_escrow_create_composer(),
                        9 => return Ok(Some(self.open_escrows_composer())),
                        10 => self.open_amm_deposit_composer(),
                        11 => self.open_amm_withdraw_composer(),
                        12 => self.open_account_delete_composer(),
                        13 => self.open_smart_account_composer(),
                        14 => self.open_tx_pick_composer(),
                        _ => {}
                    },
                    KeyCode::Char('1') => self.open_payment_composer(),
                    KeyCode::Char('2') => self.open_account_set_composer(),
                    KeyCode::Char('3') => self.open_set_regular_key_composer(),
                    KeyCode::Char('4') => self.open_offer_create_composer(),
                    KeyCode::Char('5') => self.open_trust_set_composer(),
                    KeyCode::Char('6') => self.open_fxrp_direct_mint_composer(),
                    KeyCode::Char('7') => self.open_fxrp_execute_direct_mint_composer(),
                    KeyCode::Char('8') => return Ok(Some(self.open_offer_cancel_composer())),
                    KeyCode::Char('9') => self.open_escrow_create_composer(),
                    KeyCode::Char('0') => return Ok(Some(self.open_escrows_composer())),
                    KeyCode::Char('d') => self.open_amm_deposit_composer(),
                    KeyCode::Char('w') => self.open_amm_withdraw_composer(),
                    KeyCode::Char('x') => self.open_account_delete_composer(),
                    KeyCode::Char('a') => self.open_smart_account_composer(),
                    KeyCode::Char('m') => self.open_tx_pick_composer(),
                    _ => {}
                }
                return Ok(None);
            }
            Some(ComposerPhase::AccountSet) => {
                return Ok(self.account_set_modal_key_to_action(key));
            }
            Some(ComposerPhase::Payment { row, is_iou, .. }) => {
                let row_count: usize = if *is_iou { 5 } else { 3 };
                if Self::composer_row_nav(&mut self.is_form_editing, &key, row, row_count) {
                    return Ok(Some(Action::SetKeymapSuppression(true)));
                }
                return Ok(None);
            }
            Some(ComposerPhase::SetRegularKey { .. } | ComposerPhase::AccountDelete { .. }) => {
                if Self::composer_edit_toggle_nav(&mut self.is_form_editing, &key) {
                    return Ok(Some(Action::SetKeymapSuppression(true)));
                }
                return Ok(None);
            }
            Some(ComposerPhase::OfferCreate { row, .. }) => {
                if Self::composer_row_nav(&mut self.is_form_editing, &key, row, 2) {
                    return Ok(Some(Action::SetKeymapSuppression(true)));
                }
                return Ok(None);
            }
            Some(ComposerPhase::TrustSet { row, .. }) => {
                if Self::composer_row_nav(&mut self.is_form_editing, &key, row, 3) {
                    return Ok(Some(Action::SetKeymapSuppression(true)));
                }
                return Ok(None);
            }
            Some(ComposerPhase::FxrpDirectMint { row, .. }) => {
                if Self::composer_row_nav(&mut self.is_form_editing, &key, row, 2) {
                    return Ok(Some(Action::SetKeymapSuppression(true)));
                }
                return Ok(None);
            }
            Some(ComposerPhase::FxrpExecuteDirectMint { .. }) => {
                if Self::composer_edit_toggle_nav(&mut self.is_form_editing, &key) {
                    return Ok(Some(Action::SetKeymapSuppression(true)));
                }
                return Ok(None);
            }
            Some(ComposerPhase::SmartAccount { row, redeem, .. }) => {
                // `i` flips transfer ↔ redeem (not while typing, where it is a character).
                if !self.is_form_editing
                    && matches!(key.code, KeyCode::Char('i') | KeyCode::Char('I'))
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    *redeem = !*redeem;
                    *row = 0;
                    return Ok(None);
                }
                let rows = if *redeem { 1 } else { 2 };
                if Self::composer_row_nav(&mut self.is_form_editing, &key, row, rows) {
                    return Ok(Some(Action::SetKeymapSuppression(true)));
                }
                return Ok(None);
            }
            Some(ComposerPhase::OfferCancel { offers, selected }) => {
                let last = match offers {
                    Some(Ok(rows)) => rows.len().saturating_sub(1),
                    _ => 0,
                };
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => *selected = (*selected + 1).min(last),
                    KeyCode::Char('k') | KeyCode::Up => *selected = selected.saturating_sub(1),
                    _ => {}
                }
                return Ok(None);
            }
            Some(ComposerPhase::AmmDeposit { row, .. }) => {
                if Self::composer_row_nav(&mut self.is_form_editing, &key, row, 2) {
                    return Ok(Some(Action::SetKeymapSuppression(true)));
                }
                return Ok(None);
            }
            Some(ComposerPhase::AmmWithdraw { row, .. }) => {
                if Self::composer_row_nav(&mut self.is_form_editing, &key, row, 3) {
                    return Ok(Some(Action::SetKeymapSuppression(true)));
                }
                return Ok(None);
            }
            Some(ComposerPhase::EscrowCreate { row, .. }) => {
                if Self::composer_row_nav(&mut self.is_form_editing, &key, row, 4) {
                    return Ok(Some(Action::SetKeymapSuppression(true)));
                }
                return Ok(None);
            }
            Some(ComposerPhase::Escrows { escrows, selected }) => {
                let last = match escrows {
                    Some(Ok(rows)) => rows.len().saturating_sub(1),
                    _ => 0,
                };
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => *selected = (*selected + 1).min(last),
                    KeyCode::Char('k') | KeyCode::Up => *selected = selected.saturating_sub(1),
                    _ => {}
                }
                return Ok(None);
            }
            Some(ComposerPhase::TxPick { selected }) => {
                let n = crate::xrpl::txspec::registry().count();
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => {
                        *selected = (*selected + 1) % n;
                    }
                    KeyCode::Char('k') | KeyCode::Up | KeyCode::BackTab => {
                        *selected = (*selected + n - 1) % n;
                    }
                    KeyCode::Enter => {
                        if let Some(spec) = crate::xrpl::txspec::registry().nth(*selected) {
                            self.open_tx_composer(spec, &[]);
                        }
                    }
                    _ => {}
                }
                return Ok(None);
            }
            Some(ComposerPhase::Tx { row, values, .. }) => {
                let rows = values.len();
                if Self::composer_row_nav(&mut self.is_form_editing, &key, row, rows) {
                    return Ok(Some(Action::SetKeymapSuppression(true)));
                }
                return Ok(None);
            }
            Some(ComposerPhase::DangerConfirm { .. }) => {
                // Handled above in the dedicated DangerConfirm key branch.
            }
            None => {}
        }

        // Composer needs a funded account (modals draw in every state, but a send from an
        // unfunded/unknown account is pointless); keygen only needs a valid wallet config.
        if self.composer.is_none()
            && self.account.is_some()
            && matches!(self.seed_address, Some(Ok(_)))
            && matches!(key.code, KeyCode::Char('t') | KeyCode::Char('T'))
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.is_form_editing = false;
            self.composer = Some(ComposerPhase::PickKind { selected: 0 });
            return Ok(Some(Action::SetKeymapSuppression(true)));
        }

        // Key generation (g — only when no composer open)
        if self.composer.is_none()
            && matches!(self.seed_address, Some(Ok(_)))
            && key.code == KeyCode::Char('g')
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.keygen_result = None;
            return Ok(Some(Action::GenerateWalletKeys));
        }

        Ok(None)
    }

    /// Refuse a write identical to one still awaiting its Ok/Err (double `s`, repeated confirm).
    /// Local validation failures (`*SubmitErr` built by the composer) only flash: they are not
    /// results of an in-flight write, so they must not release its guard.
    fn guard_double_submit(&mut self, out: Option<Action>) -> Option<Action> {
        let action = out?;
        if let Some((label, msg)) = submit_err(&action) {
            self.set_submit_flash(SubmitFlash::Error(format!("{label} · {msg}")));
            return None;
        }
        if !is_write_submit(&action) {
            return Some(action);
        }
        let key = format!("{action:?}");
        if self.pending_submit.as_deref() == Some(key.as_str()) {
            self.set_submit_flash(SubmitFlash::Error(
                "already submitted · waiting for its result".into(),
            ));
            return None;
        }
        self.pending_submit = Some(key);
        self.submit_in_composer = true;
        Some(action)
    }
}

fn is_write_submit(action: &Action) -> bool {
    matches!(
        action,
        Action::AccountSetSubmit(_)
            | Action::PaymentSubmit(_)
            | Action::FxrpDirectMintPaymentSubmit(_)
            | Action::SmartAccountInstructionSubmit(_)
            | Action::FxrpExecuteDirectMintSubmit(_)
            | Action::SetRegularKeySubmit(_)
            | Action::EscrowCreateSubmit(_)
            | Action::EscrowCloseSubmit(_)
            | Action::AmmDepositSubmit(_)
            | Action::AmmWithdrawSubmit(_)
            | Action::OfferCreateSubmit(_)
            | Action::OfferCancelSubmit(_)
            | Action::TrustSetSubmit(_)
            | Action::AccountDeleteSubmit(_)
            | Action::GenericTxSubmit(_)
    )
}

fn is_write_result(action: &Action) -> bool {
    matches!(
        action,
        Action::AccountSetSubmitOk(_)
            | Action::AccountSetSubmitErr(_)
            | Action::PaymentSubmitOk(_)
            | Action::PaymentSubmitErr(_)
            | Action::FxrpDirectMintPaymentSubmitOk(_)
            | Action::FxrpDirectMintPaymentSubmitErr(_)
            | Action::SmartAccountInstructionSubmitOk(_)
            | Action::SmartAccountInstructionSubmitErr(_)
            | Action::FxrpExecuteDirectMintSubmitOk(_)
            | Action::FxrpExecuteDirectMintSubmitErr(_)
            | Action::SetRegularKeySubmitOk(_)
            | Action::SetRegularKeySubmitErr(_)
            | Action::EscrowCreateSubmitOk(_)
            | Action::EscrowCreateSubmitErr(_)
            | Action::EscrowCloseSubmitOk(_)
            | Action::EscrowCloseSubmitErr(_)
            | Action::AmmSubmitOk(_)
            | Action::AmmSubmitErr(_)
            | Action::OfferCreateSubmitOk(_)
            | Action::OfferCreateSubmitErr(_)
            | Action::OfferCancelSubmitOk(_)
            | Action::OfferCancelSubmitErr(_)
            | Action::TrustSetSubmitOk(_)
            | Action::TrustSetSubmitErr(_)
            | Action::AccountDeleteSubmitOk(_)
            | Action::AccountDeleteSubmitErr(_)
            | Action::GenericTxSubmitOk(_)
            | Action::GenericTxSubmitErr(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
    }

    #[test]
    fn payment_composer_defaults_amount_1_and_i_toggles_iou_then_clears_fields_on_xrp() {
        let mut panel = WalletPanel::new(false);
        panel.is_focused = true;
        panel.open_payment_composer();
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::Payment { is_iou: false, .. })
        ));
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

    /// `WalletPanel::new(skip)` is what reaches the queued params: `false` must not skip the
    /// mainnet prompt for any write (a hardcoded `true` would pass the `new(true)` asserts).
    #[test]
    fn skip_mainnet_prompt_false_is_forwarded_to_queued_writes() {
        const DEST: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        const ISS: &str = "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN";
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        for skip in [false, true] {
            let mut panel = WalletPanel::new(skip);
            panel.is_focused = true;
            match panel.queue_submit_payment(
                DEST.into(),
                "1".into(),
                "".into(),
                "".into(),
                "".into(),
            ) {
                Action::PaymentSubmit(p) => assert_eq!(p.skip_mainnet_prompt, skip, "payment"),
                other => panic!("expected PaymentSubmit, got {other:?}"),
            }

            panel.composer = Some(ComposerPhase::PickKind { selected: 0 });
            panel.on_key_event(key('x')).expect("open delete");
            if let Some(ComposerPhase::AccountDelete { destination }) = &mut panel.composer {
                *destination = DEST.into();
            }
            panel.on_key_event(key('s')).expect("s");
            for c in "DELETE".chars() {
                panel.on_key_event(key(c)).expect("type");
            }
            match panel.on_key_event(enter).expect("confirm") {
                Some(Action::AccountDeleteSubmit(p)) => {
                    assert_eq!(p.skip_mainnet_prompt, skip, "account delete");
                }
                other => panic!("expected AccountDeleteSubmit, got {other:?}"),
            }

            panel.composer = Some(ComposerPhase::PickKind { selected: 0 });
            panel.on_key_event(key('d')).expect("open deposit");
            if let Some(ComposerPhase::AmmDeposit { amount2, .. }) = &mut panel.composer {
                *amount2 = format!("USD:{ISS}:10");
            }
            match panel.on_key_event(key('s')).expect("deposit") {
                Some(Action::AmmDepositSubmit(p)) => {
                    assert_eq!(p.skip_mainnet_prompt, skip, "amm deposit");
                }
                other => panic!("expected AmmDepositSubmit, got {other:?}"),
            }
        }
    }

    /// Dismissing the keygen popup (Esc) and regenerating (`g`) both drop the shown seed.
    #[test]
    fn keygen_result_cleared_on_esc_and_on_regenerate() {
        let result = || crate::xrpl::GeneratedWalletKeys {
            account_id: "rKeygenAcct".into(),
            key_type: "ed25519".into(),
            master_seed: "sEdSECRETSEED".into(),
            master_seed_hex: "00".repeat(16),
            public_key: "ED00".into(),
        };
        let mut panel = WalletPanel::new(false);
        panel.is_focused = true;
        panel.seed_address = Some(Ok("rOwner".into()));
        panel
            .update(&Action::GenerateWalletKeysOk(result()))
            .expect("ok");
        assert!(panel.keygen_result.is_some());
        panel
            .on_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()))
            .expect("esc");
        assert!(panel.keygen_result.is_none(), "Esc dismisses");

        panel
            .update(&Action::GenerateWalletKeysOk(result()))
            .expect("ok");
        let act = panel.on_key_event(key('g')).expect("g");
        assert!(matches!(act, Some(Action::GenerateWalletKeys)));
        assert!(panel.keygen_result.is_none(), "g clears the previous seed");
    }

    #[test]
    fn queue_submit_payment_uppercases_currency_and_omits_empty() {
        let panel = WalletPanel::new(true);
        match panel.queue_submit_payment(
            "rDest".into(),
            "2.5".into(),
            "usd".into(),
            "rIssuer".into(),
            String::new(),
        ) {
            Action::PaymentSubmit(p) => {
                assert_eq!(p.iou_currency.as_deref(), Some("USD"));
            }
            other => panic!("expected PaymentSubmit, got {other:?}"),
        }
        match panel.queue_submit_payment(
            "rDest".into(),
            "1".into(),
            String::new(),
            String::new(),
            String::new(),
        ) {
            Action::PaymentSubmit(p) => {
                assert!(p.iou_currency.is_none());
                assert!(p.iou_issuer.is_none());
                assert!(p.send_max.is_none());
            }
            other => panic!("expected PaymentSubmit, got {other:?}"),
        }
        match panel.queue_submit_payment(
            "rDest".into(),
            "1".into(),
            String::new(),
            String::new(),
            "  XRP:5  ".into(),
        ) {
            Action::PaymentSubmit(p) => assert_eq!(p.send_max.as_deref(), Some("XRP:5")),
            other => panic!("expected PaymentSubmit, got {other:?}"),
        }
    }

    /// TC-185: OfferCancel composer — key `8` fetches offers, `j`/`s` queue the selected seq.
    #[test]
    fn offer_cancel_composer_lists_offers_and_queues_selected_seq() {
        let row = |seq| AccountOfferRow {
            seq,
            taker_gets: "1000000".into(),
            taker_pays: "2 USD".into(),
        };
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let mut panel = WalletPanel::new(true);
        panel.is_focused = true;
        panel.composer = Some(ComposerPhase::PickKind { selected: 0 });

        let open = panel.on_key_event(key('8')).expect("open");
        assert!(matches!(open, Some(Action::RefreshAccountOffers)));

        assert!(panel.on_key_event(key('s')).expect("early s").is_none());
        assert!(matches!(
            &panel.submit_flash,
            Some(SubmitFlash::Error(m)) if m.contains("no open offer selected")
        ));

        panel
            .update(&Action::XrplAccountOffers(vec![row(11), row(22)]))
            .expect("offers");
        panel.on_key_event(key('j')).expect("down");
        let out = crate::test_support::render_to_string(80, 24, |f| panel.render_composer(f, area));
        assert!(out.contains("#22"), "{out}");
        match panel.on_key_event(key('s')).expect("submit") {
            Some(Action::OfferCancelSubmit(p)) => {
                assert_eq!(p.offer_sequence, 22);
                assert!(p.skip_mainnet_prompt);
            }
            other => panic!("expected OfferCancelSubmit, got {other:?}"),
        }

        panel
            .update(&Action::XrplError("account_offers: timeout".into()))
            .expect("err");
        let out = crate::test_support::render_to_string(80, 24, |f| panel.render_composer(f, area));
        assert!(out.contains("account_offers: timeout"), "{out}");
    }

    /// TC-225: AMM composers — picker `d` / `w`; invalid input flashes; deposit queues both
    /// amount specs; withdraw with empty LP = withdraw all; Esc returns to picker rows 10 / 11.
    #[test]
    fn amm_composers_validate_then_queue() {
        const ISS: &str = "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN";
        let mut panel = WalletPanel::new(true);
        panel.is_focused = true;
        panel.composer = Some(ComposerPhase::PickKind { selected: 0 });
        assert!(
            panel
                .on_key_event(key('d'))
                .expect("open deposit")
                .is_none()
        );
        assert!(panel.on_key_event(key('s')).expect("bad").is_none());
        assert!(matches!(&panel.submit_flash, Some(SubmitFlash::Error(m)) if m.contains("AMM")));
        if let Some(ComposerPhase::AmmDeposit { amount2, .. }) = &mut panel.composer {
            *amount2 = format!("USD:{ISS}:10");
        }
        match panel.on_key_event(key('s')).expect("deposit") {
            Some(Action::AmmDepositSubmit(p)) => {
                assert_eq!(
                    (p.amount.as_str(), p.amount2.as_str()),
                    ("XRP:1000000", &*format!("USD:{ISS}:10"))
                );
                assert!(p.skip_mainnet_prompt);
            }
            other => panic!("expected AmmDepositSubmit, got {other:?}"),
        }
        panel
            .on_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()))
            .expect("esc");
        assert!(matches!(
            panel.composer,
            Some(ComposerPhase::PickKind { selected: 10 })
        ));

        assert!(
            panel
                .on_key_event(key('w'))
                .expect("open withdraw")
                .is_none()
        );
        if let Some(ComposerPhase::AmmWithdraw { asset2, .. }) = &mut panel.composer {
            *asset2 = format!("USD:{ISS}");
        }
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let out = crate::test_support::render_to_string(80, 24, |f| panel.render_composer(f, area));
        assert!(out.contains("empty = withdraw all"), "{out}");
        match panel.on_key_event(key('s')).expect("withdraw") {
            Some(Action::AmmWithdrawSubmit(p)) => {
                assert_eq!((p.asset.as_str(), p.lp_amount.as_str()), ("XRP", ""));
            }
            other => panic!("expected AmmWithdrawSubmit, got {other:?}"),
        }
        panel
            .on_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()))
            .expect("esc");
        assert!(matches!(
            panel.composer,
            Some(ComposerPhase::PickKind { selected: 11 })
        ));
    }

    /// TC-228: double-submit guard — the same write with identical params is refused (flash)
    /// until its Ok/Err arrives; different params, or the same after the result, go through.
    #[test]
    fn identical_submit_refused_until_result() {
        const ISS: &str = "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN";
        let mut panel = WalletPanel::new(true);
        panel.is_focused = true;
        panel.composer = Some(ComposerPhase::PickKind { selected: 0 });
        panel.on_key_event(key('d')).expect("open");
        if let Some(ComposerPhase::AmmDeposit { amount2, .. }) = &mut panel.composer {
            *amount2 = format!("USD:{ISS}:10");
        }
        let submit = |p: &mut WalletPanel| p.on_key_event(key('s')).expect("s");
        assert!(matches!(
            submit(&mut panel),
            Some(Action::AmmDepositSubmit(_))
        ));
        assert!(submit(&mut panel).is_none(), "identical while pending");
        assert!(matches!(
            &panel.submit_flash,
            Some(SubmitFlash::Error(m)) if m.contains("already submitted")
        ));
        if let Some(ComposerPhase::AmmDeposit { amount2, .. }) = &mut panel.composer {
            *amount2 = format!("USD:{ISS}:11");
        }
        assert!(
            matches!(submit(&mut panel), Some(Action::AmmDepositSubmit(_))),
            "different params"
        );
        panel
            .update(&Action::AmmSubmitErr("tecX".into()))
            .expect("result");
        assert!(
            matches!(submit(&mut panel), Some(Action::AmmDepositSubmit(_))),
            "retry after result"
        );
    }

    /// TC-219: EscrowCreate composer — `9` opens it; invalid input flashes instead of queuing;
    /// valid fields queue `EscrowCreateSubmit` (trimmed, relative durations kept for the poll task).
    #[test]
    fn escrow_create_composer_validates_then_queues() {
        const DEST: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let mut panel = WalletPanel::new(true);
        panel.is_focused = true;
        panel.composer = Some(ComposerPhase::PickKind { selected: 0 });
        assert!(panel.on_key_event(key('9')).expect("open").is_none());

        assert!(panel.on_key_event(key('s')).expect("s").is_none());
        assert!(matches!(
            &panel.submit_flash,
            Some(SubmitFlash::Error(m)) if m.contains("Destination")
        ));

        if let Some(ComposerPhase::EscrowCreate {
            destination,
            cancel_after,
            ..
        }) = &mut panel.composer
        {
            *destination = format!(" {DEST} ");
            *cancel_after = "2d".into();
        }
        let out = crate::test_support::render_to_string(80, 24, |f| panel.render_composer(f, area));
        assert!(out.contains("FinishAfter") && out.contains("2d"), "{out}");
        match panel.on_key_event(key('s')).expect("submit") {
            Some(Action::EscrowCreateSubmit(p)) => {
                assert_eq!(p.destination, DEST);
                assert_eq!(
                    (p.finish_after.as_str(), p.cancel_after.as_str()),
                    ("1h", "2d")
                );
                assert!(p.skip_mainnet_prompt);
            }
            other => panic!("expected EscrowCreateSubmit, got {other:?}"),
        }
        panel
            .on_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()))
            .expect("esc");
        assert!(matches!(
            panel.composer,
            Some(ComposerPhase::PickKind { selected: 8 })
        ));
    }

    /// TC-233: AccountDelete is irreversible — `x` opens it; `s` never queues directly but asks
    /// for DELETE (bad destination: flash only). Esc keeps the destination; a partial word is
    /// refused; DELETE + Enter queues the trimmed destination.
    #[test]
    fn account_delete_requires_typed_delete_and_keeps_destination() {
        const DEST: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        let mut panel = WalletPanel::new(true);
        panel.is_focused = true;
        panel.composer = Some(ComposerPhase::PickKind { selected: 0 });
        assert!(panel.on_key_event(key('x')).expect("open").is_none());

        assert!(panel.on_key_event(key('s')).expect("s").is_none());
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::AccountDelete { .. })
        ));
        if let Some(ComposerPhase::AccountDelete { destination }) = &mut panel.composer {
            *destination = format!(" {DEST} ");
        }
        assert!(panel.on_key_event(key('s')).expect("s").is_none());
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::DangerConfirm { expected, .. }) if expected == "DELETE"
        ));
        // DangerConfirm opens in typing mode: first Esc leaves typing, second leaves the modal.
        for _ in 0..2 {
            panel
                .on_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()))
                .expect("esc");
        }
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::AccountDelete { destination }) if destination.trim() == DEST
        ));

        panel.on_key_event(key('s')).expect("s");
        for c in "DELET".chars() {
            panel.on_key_event(key(c)).expect("type");
        }
        assert!(panel.on_key_event(enter).expect("partial").is_none());
        panel.on_key_event(key('E')).expect("type");
        match panel.on_key_event(enter).expect("confirm") {
            Some(Action::AccountDeleteSubmit(p)) => {
                assert_eq!(p.destination, DEST);
                assert!(p.skip_mainnet_prompt);
            }
            other => panic!("expected AccountDeleteSubmit, got {other:?}"),
        }
    }

    const TEST_NFT_ID: &str = "000B013A95F14B0044F78A264E41713C64B5F89242540EE208C3098E00000D65";

    fn special(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn set_tx_field(panel: &mut WalletPanel, field: usize, value: &str) {
        if let Some(ComposerPhase::Tx { values, .. }) = &mut panel.composer {
            values[field] = value.to_string();
        } else {
            panic!("not in a Tx form: {:?}", panel.composer);
        }
    }

    /// TC-255: the Transaction composer is generated from the spec — `m` opens the picker, Enter a
    /// form with one row per field (spec defaults prefilled); an invalid value flashes
    /// `<Type> · <Field>: …` and never queues; a valid form queues one `GenericTxSubmit`
    /// with every field and the `--yes` flag; an identical resubmit is refused until a result.
    #[test]
    fn tx_composer_generates_form_validates_and_queues() {
        let mut panel = WalletPanel::new(true);
        panel.is_focused = true;
        panel.composer = Some(ComposerPhase::PickKind { selected: 0 });
        assert!(panel.on_key_event(key('m')).expect("m").is_none());
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::TxPick { selected: 0 })
        ));
        panel.on_key_event(special(KeyCode::Enter)).expect("open");
        match &panel.composer {
            Some(ComposerPhase::Tx {
                spec,
                row: 0,
                values,
            }) => {
                assert_eq!(spec.tx_type, "NFTokenMint");
                assert_eq!(values.len(), spec.fields.len());
                assert_eq!(values[0], "0", "spec default prefilled");
            }
            other => panic!("expected Tx form, got {other:?}"),
        }
        // Row navigation + edit keys are the shared composer ones.
        panel.on_key_event(special(KeyCode::Tab)).expect("tab");
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::Tx { row: 1, .. })
        ));

        // Invalid: fee > 50000 (with transferable) is flashed with the field name, nothing queued.
        set_tx_field(&mut panel, 2, "transferable");
        set_tx_field(&mut panel, 3, "50001");
        assert!(panel.on_key_event(key('s')).expect("s").is_none());
        assert!(matches!(
            &panel.submit_flash,
            Some(SubmitFlash::Error(m)) if m.starts_with("NFTokenMint · TransferFee")
        ));
        set_tx_field(&mut panel, 3, "50000");
        let queued = panel.on_key_event(key('s')).expect("s");
        match &queued {
            Some(Action::GenericTxSubmit(p)) => {
                assert_eq!(p.tx_type, "NFTokenMint");
                assert!(p.skip_mainnet_prompt);
                assert_eq!(p.fields["NFTokenTaxon"], "0");
                assert_eq!(p.fields["Flags"], "transferable");
                assert_eq!(p.fields["TransferFee"], "50000");
                assert_eq!(p.fields.len(), 5, "every spec field is carried");
            }
            other => panic!("expected GenericTxSubmit, got {other:?}"),
        }
        // Same params again: refused until the result arrives (TC-228 guard covers specs).
        assert!(panel.on_key_event(key('s')).expect("again").is_none());
        assert!(matches!(
            &panel.submit_flash,
            Some(SubmitFlash::Error(m)) if m.contains("already submitted")
        ));
        panel
            .update(&Action::GenericTxSubmitErr(
                "simulate tecNO_PERMISSION".into(),
            ))
            .expect("err");
        assert!(matches!(
            &panel.submit_flash,
            Some(SubmitFlash::Error(m)) if m.starts_with("Transaction · simulate tec")
        ));
        assert!(panel.on_key_event(key('s')).expect("retry").is_some());
        let closed = panel
            .update(&Action::GenericTxSubmitOk("NFTokenMint · ABC".into()))
            .expect("ok");
        assert!(matches!(closed, Some(Action::SetKeymapSuppression(false))));
        assert!(panel.composer.is_none());

        // Esc walks back: form → picker (same type selected) → action list (row 14) → closed.
        panel.composer = Some(ComposerPhase::PickKind { selected: 0 });
        panel.on_key_event(key('m')).expect("m");
        panel.on_key_event(key('j')).expect("down");
        panel.on_key_event(special(KeyCode::Enter)).expect("open");
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::Tx { spec, .. }) if spec.tx_type == "NFTokenBurn"
        ));
        for want in [1_usize, 14] {
            panel.on_key_event(special(KeyCode::Esc)).expect("esc");
            match (&panel.composer, want) {
                (Some(ComposerPhase::TxPick { selected }), 1) => assert_eq!(*selected, want),
                (Some(ComposerPhase::PickKind { selected }), 14) => assert_eq!(*selected, want),
                other => panic!("unexpected Esc target: {other:?}"),
            }
        }
    }

    /// TC-255: irreversible types (burn, accept) never queue from `s`: a valid form asks for the
    /// typed phrase (`BURN`); Esc keeps the form; a partial phrase is refused. `TxComposerOpen`
    /// (Assets `x`) opens the burn form prefilled, but only with a signing credential.
    #[test]
    fn tx_composer_burn_needs_typed_phrase_and_opens_prefilled() {
        let mut panel = WalletPanel::new(false);
        panel.is_focused = true;
        let open = Action::TxComposerOpen {
            tx_type: "NFTokenBurn".into(),
            fields: vec![("NFTokenID".into(), TEST_NFT_ID.into())],
        };
        assert!(panel.update(&open).expect("no key").is_none());
        assert!(
            panel.composer.is_none(),
            "no signing credential → stays closed"
        );

        panel.seed_address = Some(Ok("rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into()));
        let suppress = panel.update(&open).expect("open");
        assert!(matches!(suppress, Some(Action::SetKeymapSuppression(true))));
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::Tx { spec, values, .. })
                if spec.tx_type == "NFTokenBurn" && values[0] == TEST_NFT_ID && values[1].is_empty()
        ));
        let unknown = Action::TxComposerOpen {
            tx_type: "Nope".into(),
            fields: vec![],
        };
        panel.update(&unknown).expect("unknown type ignored");
        assert!(matches!(&panel.composer, Some(ComposerPhase::Tx { .. })));

        // Bad id: flash only; valid: DangerConfirm(BURN), not a queued action.
        set_tx_field(&mut panel, 0, "abc");
        assert!(panel.on_key_event(key('s')).expect("s").is_none());
        assert!(matches!(&panel.composer, Some(ComposerPhase::Tx { .. })));
        assert!(matches!(
            &panel.submit_flash,
            Some(SubmitFlash::Error(m)) if m.contains("NFTokenID")
        ));
        set_tx_field(&mut panel, 0, TEST_NFT_ID);
        assert!(panel.on_key_event(key('s')).expect("s").is_none());
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::DangerConfirm { expected, .. }) if expected == "BURN"
        ));
        // Typing mode first (Esc leaves it), then Esc restores the form with its values.
        for _ in 0..2 {
            panel.on_key_event(special(KeyCode::Esc)).expect("esc");
        }
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::Tx { values, .. }) if values[0] == TEST_NFT_ID
        ));

        panel.on_key_event(key('s')).expect("s");
        for c in "BUR".chars() {
            panel.on_key_event(key(c)).expect("type");
        }
        assert!(
            panel
                .on_key_event(special(KeyCode::Enter))
                .expect("partial")
                .is_none()
        );
        panel.on_key_event(key('N')).expect("type");
        match panel
            .on_key_event(special(KeyCode::Enter))
            .expect("confirm")
        {
            Some(Action::GenericTxSubmit(p)) => {
                assert_eq!(p.tx_type, "NFTokenBurn");
                assert_eq!(p.fields["NFTokenID"], TEST_NFT_ID);
                assert!(!p.skip_mainnet_prompt);
            }
            other => panic!("expected GenericTxSubmit, got {other:?}"),
        }
    }

    /// TC-255: the form renders one labelled row per spec field with the focused row's help and
    /// inline validation error, and an orange/green preview line.
    #[test]
    fn tx_composer_renders_fields_help_and_inline_errors() {
        let mut panel = WalletPanel::new(true);
        panel.is_focused = true;
        panel.composer = Some(ComposerPhase::PickKind { selected: 0 });
        panel.on_key_event(key('m')).expect("m");
        panel.on_key_event(key('j')).expect("down");
        panel.on_key_event(special(KeyCode::Enter)).expect("open");
        let area = ratatui::layout::Rect::new(0, 0, 110, 24);
        let draw = |panel: &WalletPanel| {
            crate::test_support::render_to_string(110, 24, |f| panel.render_composer(f, area))
        };
        let screen = draw(&panel);
        for needle in [
            "NFTokenBurn",
            "NFTokenID",
            "Owner",
            "64 hex characters",
            "(required)",
        ] {
            assert!(screen.contains(needle), "missing {needle:?}:\n{screen}");
        }
        set_tx_field(&mut panel, 0, "abc");
        let screen = draw(&panel);
        assert!(
            screen.contains("✗ expected 64 hex characters (got 3)"),
            "inline error missing:\n{screen}"
        );
        set_tx_field(&mut panel, 0, TEST_NFT_ID);
        let screen = draw(&panel);
        assert!(
            screen.contains("▸ NFTokenBurn NFTokenID="),
            "valid preview:\n{screen}"
        );
        assert!(screen.contains("then type BURN"), "confirm hint:\n{screen}");
    }

    /// TC-220: Escrow finish/cancel composer — `0` fetches escrows; `s` finishes, `c` cancels the
    /// selected row; condition escrows cannot be finished and unresolved sequences are refused.
    #[test]
    fn escrow_list_composer_finishes_and_cancels_selected() {
        let row = |seq: Option<u32>, has_condition: bool| crate::xrpl::EscrowRow {
            owner: "rOwner".into(),
            destination: "rDest".into(),
            amount: "1.5 XRP".into(),
            finish_after: Some(800),
            cancel_after: None,
            has_condition,
            offer_sequence: seq,
            previous_txn_id: "AB".into(),
        };
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let mut panel = WalletPanel::new(true);
        panel.is_focused = true;
        panel.composer = Some(ComposerPhase::PickKind { selected: 0 });
        let open = panel.on_key_event(key('0')).expect("open");
        assert!(matches!(open, Some(Action::RefreshAccountEscrows)));
        assert!(panel.on_key_event(key('s')).expect("early s").is_none());
        assert!(matches!(
            &panel.submit_flash,
            Some(SubmitFlash::Error(m)) if m.contains("no escrow selected")
        ));

        panel
            .update(&Action::XrplAccountEscrows(vec![
                row(Some(5), true),
                row(None, false),
                row(Some(9), false),
            ]))
            .expect("escrows");
        assert!(panel.on_key_event(key('s')).expect("finish cond").is_none());
        assert!(matches!(
            &panel.submit_flash,
            Some(SubmitFlash::Error(m)) if m.contains("condition")
        ));
        match panel.on_key_event(key('c')).expect("cancel") {
            Some(Action::EscrowCloseSubmit(p)) => {
                assert_eq!(
                    (p.owner.as_str(), p.offer_sequence, p.cancel),
                    ("rOwner", 5, true)
                );
            }
            other => panic!("expected EscrowCloseSubmit, got {other:?}"),
        }
        panel.on_key_event(key('j')).expect("down");
        assert!(panel.on_key_event(key('s')).expect("no seq").is_none());
        assert!(matches!(
            &panel.submit_flash,
            Some(SubmitFlash::Error(m)) if m.contains("sequence")
        ));
        panel.on_key_event(key('j')).expect("down");
        let out = crate::test_support::render_to_string(80, 24, |f| panel.render_composer(f, area));
        assert!(out.contains("#9") && out.contains("1.5 XRP"), "{out}");
        match panel.on_key_event(key('s')).expect("finish") {
            Some(Action::EscrowCloseSubmit(p)) => {
                assert_eq!((p.offer_sequence, p.cancel), (9, false));
            }
            other => panic!("expected EscrowCloseSubmit, got {other:?}"),
        }
    }

    /// TC-194: Payment `p` requests a path quote (form fields + default 100 bps slippage);
    /// incomplete form flashes instead; QuoteOk fills Send max, QuoteErr flashes.
    #[test]
    fn payment_quote_key_requests_quote_and_fills_send_max() {
        const GENESIS: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        let mut panel = WalletPanel::new(false);
        panel.is_focused = true;
        panel.open_payment_composer();
        if let Some(ComposerPhase::Payment { amount, .. }) = &mut panel.composer {
            amount.clear();
        }
        assert!(panel.on_key_event(key('p')).expect("p").is_none());
        assert!(matches!(&panel.submit_flash, Some(SubmitFlash::Error(_))));

        if let Some(ComposerPhase::Payment {
            destination,
            amount,
            iou_currency,
            iou_issuer,
            send_max,
            is_iou,
            ..
        }) = &mut panel.composer
        {
            *destination = GENESIS.into();
            *amount = "10".into();
            *iou_currency = "USD".into();
            *iou_issuer = GENESIS.into();
            *send_max = "XRP".into();
            *is_iou = true;
        }
        match panel.on_key_event(key('p')).expect("p") {
            Some(Action::PaymentQuote(q)) => {
                assert_eq!(q.destination, GENESIS);
                assert_eq!(q.amount, "10");
                assert_eq!(q.iou_currency.as_deref(), Some("USD"));
                assert_eq!(q.send_max, "XRP");
                assert_eq!(q.slippage_bps, 100);
            }
            other => panic!("expected PaymentQuote, got {other:?}"),
        }

        panel
            .update(&Action::PaymentQuoteOk("XRP:1010000".into()))
            .expect("ok");
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::Payment { send_max, .. }) if send_max == "XRP:1010000"
        ));
        panel
            .update(&Action::PaymentQuoteErr("no path".into()))
            .expect("err");
        assert!(matches!(
            &panel.submit_flash,
            Some(SubmitFlash::Error(m)) if m.contains("no path")
        ));
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
                assert!(
                    !p.skip_mainnet_prompt,
                    "WalletPanel::new(false) keeps the prompt"
                );
            }
            other => panic!("expected PaymentSubmit, got {other:?}"),
        }
        // Double-submit guard on the money path: identical refused until the result arrives.
        assert!(panel.on_key_event(key('s')).expect("again").is_none());
        assert!(matches!(
            &panel.submit_flash,
            Some(SubmitFlash::Error(m)) if m.contains("already submitted")
        ));
        panel
            .update(&Action::PaymentSubmitErr("tecX".into()))
            .expect("result");
        assert!(matches!(
            panel.on_key_event(key('s')).expect("retry"),
            Some(Action::PaymentSubmit(_))
        ));
        panel
            .update(&Action::PaymentSubmitOk("HASH".into()))
            .expect("ok");
        // Ok closes the composer; a reopened identical payment must go through.
        panel.open_payment_composer();
        if let Some(ComposerPhase::Payment {
            destination,
            amount,
            ..
        }) = &mut panel.composer
        {
            *destination = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into();
            *amount = "1.25".into();
        }
        assert!(
            matches!(
                panel.on_key_event(key('s')).expect("after ok"),
                Some(Action::PaymentSubmit(_))
            ),
            "Ok clears the guard"
        );
    }

    /// TC-198: preview + submit use the shared validation — bad destination checksum or
    /// bad Send max stays orange and `s` refuses with a flash; valid input is green.
    #[test]
    fn payment_preview_and_submit_share_validation() {
        const DEST: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        const ISS: &str = "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN";
        let (xrp, st) = WalletPanel::payment_preview(DEST, "1.5", "", "", "", false);
        assert!(xrp.contains("Send 1.5 XRP"), "{xrp}");
        assert_eq!(st, theme::success_style());
        assert!(!xrp.contains("spend"), "{xrp}");

        let (iou, st) = WalletPanel::payment_preview(DEST, "10", "USD", ISS, "XRP:5000000", true);
        assert!(iou.contains("Pay 10 USD"), "{iou}");
        assert!(iou.contains("spend ≤ XRP:5000000"), "{iou}");
        assert_eq!(st, theme::success_style());

        let (_, st) =
            WalletPanel::payment_preview("rDestAddressLongEnough", "1", "", "", "", false);
        assert_eq!(
            st,
            theme::warning_style(),
            "bad checksum must not look ready"
        );
        let (bad, st) = WalletPanel::payment_preview(DEST, "1", "", "", "XRP:5", false);
        assert_eq!(st, theme::warning_style());
        assert!(bad.contains("SendMax"), "{bad}");

        let mut panel = WalletPanel::new(false);
        panel.is_focused = true;
        panel.open_payment_composer();
        if let Some(ComposerPhase::Payment {
            destination,
            send_max,
            ..
        }) = &mut panel.composer
        {
            *destination = DEST.into();
            *send_max = "XRP:5".into();
        }
        assert!(panel.on_key_event(key('s')).expect("s").is_none());
        assert!(matches!(
            &panel.submit_flash,
            Some(SubmitFlash::Error(m)) if m.contains("SendMax")
        ));
    }

    #[test]
    fn trust_set_queue_uppercases_currency() {
        let mut panel = WalletPanel::new(true);
        panel.open_trust_set_composer();
        // Rewrite the composer fields, then observe the queue conversion.
        if let Some(ComposerPhase::TrustSet {
            currency,
            issuer,
            limit,
            ..
        }) = &mut panel.composer
        {
            *currency = "usd".into();
            *issuer = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into();
            *limit = "42".into();
        }
        match panel.queue_submit_trust_set() {
            Some(Action::TrustSetSubmit(p)) => {
                assert_eq!(p.currency, "USD", "currency must be uppercased");
            }
            other => panic!("expected TrustSetSubmit, got {other:?}"),
        }
    }

    /// The TrustSet form starts empty: pressing `s` straight away must not queue a trust
    /// line to a pre-filled issuer, and a malformed form flashes instead of queueing.
    #[test]
    fn trust_set_default_form_cannot_submit() {
        let mut panel = WalletPanel::new(true);
        panel.is_focused = true;
        panel.open_trust_set_composer();
        assert!(panel.on_key_event(key('s')).expect("s").is_none());
        assert!(matches!(
            &panel.submit_flash,
            Some(SubmitFlash::Error(m)) if m.contains("TrustSet")
        ));
        if let Some(ComposerPhase::TrustSet {
            currency,
            issuer,
            limit,
            ..
        }) = &mut panel.composer
        {
            *currency = "USD".into();
            *issuer = "rNotAnAddress".into();
            *limit = "1".into();
        }
        assert!(panel.on_key_event(key('s')).expect("s").is_none());
    }

    #[test]
    fn nonempty_set_regular_key_queues_without_clear_confirm() {
        let mut panel = WalletPanel::new(true);
        panel.open_set_regular_key_composer();
        if let Some(ComposerPhase::SetRegularKey { regular_key }) = &mut panel.composer {
            *regular_key = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into();
        }
        match panel.queue_submit_set_regular_key() {
            Some(Action::SetRegularKeySubmit(p)) => {
                assert_eq!(p.regular_key, "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh");
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

    #[test]
    fn empty_set_regular_key_requires_clear_confirm() {
        let mut panel = WalletPanel::new(true);
        panel.is_focused = true;
        panel.open_set_regular_key_composer();
        assert!(panel.queue_submit_set_regular_key().is_none());
        match &panel.composer {
            Some(ComposerPhase::DangerConfirm {
                expected,
                resume: DangerResume::SetRegularKey,
                ..
            }) => assert_eq!(expected, "CLEAR"),
            other => panic!("expected DangerConfirm CLEAR, got {other:?}"),
        }
        // Type CLEAR through the real key path; a wrong word must not submit.
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        for c in "CLEA".chars() {
            panel.on_key_event(key(c)).expect("type");
        }
        assert!(panel.on_key_event(enter).expect("partial").is_none());
        panel.on_key_event(key('R')).expect("type");
        match panel.on_key_event(enter).expect("confirm") {
            Some(Action::SetRegularKeySubmit(p)) => assert!(p.regular_key.is_empty()),
            other => panic!("expected SetRegularKeySubmit, got {other:?}"),
        }
    }

    #[test]
    fn disable_master_requires_disable_confirm() {
        let mut panel = WalletPanel::new(true);
        panel.is_focused = true;
        panel.open_account_set_composer();
        // FLAG_OPTIONS index of DisableMaster
        panel.set_flag_ix = FLAG_OPTIONS
            .iter()
            .position(|f| *f == "DisableMaster")
            .expect("DisableMaster flag");
        assert!(panel.queue_submit_account_set().is_none());
        match &panel.composer {
            Some(ComposerPhase::DangerConfirm {
                expected,
                resume: DangerResume::AccountSet,
                ..
            }) => assert_eq!(expected, "DISABLE"),
            other => panic!("expected DangerConfirm DISABLE, got {other:?}"),
        }
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        for c in "DISABL".chars() {
            panel.on_key_event(key(c)).expect("type");
        }
        assert!(panel.on_key_event(enter).expect("partial").is_none());
        panel.on_key_event(key('E')).expect("type");
        match panel.on_key_event(enter).expect("confirm") {
            Some(Action::AccountSetSubmit(p)) => {
                assert_eq!(p.set_flag.as_deref(), Some("DisableMaster"));
            }
            other => panic!("expected AccountSetSubmit, got {other:?}"),
        }
    }

    /// TC-129: proof_json preview must truncate by chars — byte slicing panicked on multibyte JSON.
    #[test]
    fn fxrp_proof_preview_truncates_by_chars_on_multibyte_proof_json() {
        use super::composer::proof_json_preview;
        // 'a' + 100×'é': byte offset 72 falls inside a char, so byte slicing would panic.
        let preview = proof_json_preview(&format!("a{}", "é".repeat(100)));
        assert_eq!(preview, format!("a{}…", "é".repeat(71)));
        let short = format!("a{}", "é".repeat(71));
        assert_eq!(proof_json_preview(&short), short, "72 chars are kept whole");
    }

    fn smart_account_panel() -> WalletPanel {
        let mut panel = WalletPanel::new(false);
        panel.is_focused = true;
        panel.seed_address = Some(Ok("rOwner".into()));
        panel.smart_account = Some(crate::xrpl::SmartAccountInfo {
            xrpl_owner: "rOwner".into(),
            controller: "0xcccccccccccccccccccccccccccccccccccccccc".into(),
            personal_account: "0x1234567890abcdef1234567890abcdef12345678".into(),
            deployed: true,
            nonce: 1,
            operator_wallets: vec!["rOperator".into()],
            transfer_fee_drops: 200_000,
            redeem_fee_drops: 1_500_000,
            fxrp_decimals: 6,
            fxrp_balance_units: 12_500_000,
            vaults: vec![],
        });
        panel
    }

    /// Fill the open Smart Account form and queue it (the Err/Ok action is the validation result).
    fn smart_account_submit(
        panel: &mut WalletPanel,
        redeem: bool,
        recipient: &str,
        amount: &str,
    ) -> Action {
        panel.open_smart_account_composer();
        if let Some(ComposerPhase::SmartAccount {
            redeem: r,
            recipient: to,
            amount: a,
            ..
        }) = &mut panel.composer
        {
            (*r, *to, *a) = (redeem, recipient.into(), amount.into());
        }
        panel.queue_submit_smart_account()
    }

    /// TC-244: Smart Account form validation — bad / zero recipient, zero / empty / over-balance
    /// amount, bad lots and a read for another account are refused with a flash message; a valid
    /// form queues exact params (FXRP decimal → base units via the read `decimals()`).
    #[test]
    fn smart_account_composer_validates_before_queueing() {
        const TO: &str = "0xf5488132432118596fa13800b68df4c0ff25131d";
        let mut panel = smart_account_panel();
        let err = |a: Action| match a {
            Action::SmartAccountInstructionSubmitErr(m) => m,
            other => panic!("expected refusal, got {other:?}"),
        };

        assert!(err(smart_account_submit(&mut panel, false, "0xabc", "1")).contains("recipient"));
        assert!(
            err(smart_account_submit(
                &mut panel,
                false,
                "0x0000000000000000000000000000000000000000",
                "1"
            ))
            .contains("zero address")
        );
        assert!(
            err(smart_account_submit(&mut panel, false, TO, "0")).contains("greater than zero")
        );
        assert!(err(smart_account_submit(&mut panel, false, TO, "")).contains("amount"));
        // Balance is 12.5 FXRP (6 decimals): one base unit above is refused, the exact balance is fine.
        assert!(err(smart_account_submit(&mut panel, false, TO, "12.500001")).contains("exceeds"));
        assert!(err(smart_account_submit(&mut panel, true, "", "abc")).contains("whole number"));
        assert!(err(smart_account_submit(&mut panel, true, "", "0")).contains("greater than zero"));

        let Action::SmartAccountInstructionSubmit(p) =
            smart_account_submit(&mut panel, false, &format!(" {TO} "), "12.5")
        else {
            panic!("valid transfer must queue");
        };
        assert_eq!(
            p,
            crate::xrpl::SmartAccountInstructionParams {
                redeem: false,
                value: 12_500_000,
                recipient: TO.into(),
                operator_xrpl: "rOperator".into(),
                fee_drops: 200_000,
                vault: None,
                skip_mainnet_prompt: false,
                network: panel.network,
            }
        );
        let Action::SmartAccountInstructionSubmit(p) =
            smart_account_submit(&mut panel, true, TO, "3")
        else {
            panic!("valid redeem must queue");
        };
        assert_eq!(
            (p.redeem, p.value, p.recipient.as_str(), p.fee_drops),
            (true, 3, "", 1_500_000)
        );

        // A read for another XRPL account is never signed under this one.
        panel.seed_address = Some(Ok("rSomeoneElse".into()));
        assert!(
            err(smart_account_submit(&mut panel, false, TO, "1"))
                .contains("not the signing account")
        );
    }

    /// TC-247: the decimal → base-units conversion, over-balance check and refusal to queue
    /// follow the `decimals()` read from the token (8 and 0 here, not an assumed 6): the exact
    /// balance is accepted, one base unit more is refused, extra fraction digits are refused.
    #[test]
    fn smart_account_composer_follows_read_decimals() {
        const TO: &str = "0xf5488132432118596fa13800b68df4c0ff25131d";
        let mut panel = smart_account_panel();
        let value = |panel: &mut WalletPanel, amount: &str| match smart_account_submit(
            panel, false, TO, amount,
        ) {
            Action::SmartAccountInstructionSubmit(p) => Ok(p.value),
            Action::SmartAccountInstructionSubmitErr(m) => Err(m),
            other => panic!("unexpected {other:?}"),
        };

        // 8 decimals, balance 12.5 FXRP = 1_250_000_000 units.
        if let Some(info) = &mut panel.smart_account {
            (info.fxrp_decimals, info.fxrp_balance_units) = (8, 1_250_000_000);
        }
        assert_eq!(value(&mut panel, "12.5"), Ok(1_250_000_000));
        assert_eq!(value(&mut panel, "0.00000001"), Ok(1));
        assert!(
            value(&mut panel, "12.50000001")
                .unwrap_err()
                .contains("exceeds")
        );
        assert!(
            value(&mut panel, "0.000000001")
                .unwrap_err()
                .contains("decimal places")
        );

        // 0 decimals: whole units only, balance 1_000 units.
        if let Some(info) = &mut panel.smart_account {
            (info.fxrp_decimals, info.fxrp_balance_units) = (0, 1_000);
        }
        assert_eq!(value(&mut panel, "1000"), Ok(1_000));
        assert!(value(&mut panel, "1001").unwrap_err().contains("exceeds"));
        assert!(
            value(&mut panel, "1.5")
                .unwrap_err()
                .contains("decimal places")
        );

        // Decimals above 18 never queue; no read at all never queues.
        if let Some(info) = &mut panel.smart_account {
            info.fxrp_decimals = 19;
        }
        assert!(value(&mut panel, "1").is_err());
        panel.smart_account = None;
        assert!(value(&mut panel, "1").is_err());
    }

    /// TC-244: the picker opens the composer with `a`, `i` flips transfer ↔ redeem, typing fills
    /// the amount, `s` queues; without a Smart Account read the composer does not open.
    #[test]
    fn smart_account_composer_keys_and_not_loaded_guard() {
        let mut panel = smart_account_panel();
        panel.composer = Some(ComposerPhase::PickKind { selected: 0 });
        panel.on_key_event(key('a')).expect("open");
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::SmartAccount { redeem: false, .. })
        ));
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let out = crate::test_support::render_to_string(80, 24, |f| panel.render_composer(f, area));
        // Operator, fee and balance come from the read; no pin configured → flagged.
        assert!(
            out.contains("rOperator") && out.contains("NOT pinned"),
            "{out}"
        );
        assert!(
            out.contains("0.2 XRP fee") && out.contains("12.5 FXRP"),
            "{out}"
        );
        panel.on_key_event(key('i')).expect("toggle");
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::SmartAccount { redeem: true, .. })
        ));
        panel.on_key_event(key('e')).expect("edit");
        panel.on_key_event(key('4')).expect("type");
        panel
            .on_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()))
            .expect("stop editing");
        match panel.on_key_event(key('s')).expect("submit") {
            Some(Action::SmartAccountInstructionSubmit(p)) => {
                assert_eq!((p.redeem, p.value), (true, 4));
            }
            other => panic!("expected SmartAccountInstructionSubmit, got {other:?}"),
        }
        // Esc returns to the picker row of this action.
        panel
            .on_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()))
            .expect("back");
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::PickKind { selected: 13 })
        ));

        let mut panel = smart_account_panel();
        panel.smart_account = None;
        panel.open_smart_account_composer();
        assert!(panel.composer.is_none());
    }

    /// TC-333: the FXRP Mint composer carries the read fee views into the queued params and
    /// refuses to queue while FXRP minting is paused (mint pause or any emergency level).
    #[test]
    fn fxrp_mint_composer_refuses_when_paused_and_carries_fees() {
        let info = |paused, level| crate::xrpl::FxrpDirectMintInfo {
            core_vault_xrpl: "rVault".into(),
            asset_manager: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            min_fee_uba: 100_000,
            fee_bips: 10,
            executor_fee_uba: 200_000,
            minting_paused: paused,
            emergency_pause_level: level,
            limits: None,
        };
        let mut panel = WalletPanel::new(true);
        panel.fxrp_direct_mint = Some(info(None, None));
        panel.open_fxrp_direct_mint_composer();
        if let Some(ComposerPhase::FxrpDirectMint {
            flare_recipient, ..
        }) = &mut panel.composer
        {
            *flare_recipient = "0xabcdef0123456789abcdef0123456789abcdef01".into();
        }
        let Action::FxrpDirectMintPaymentSubmit(p) = panel.queue_submit_fxrp_direct_mint() else {
            panic!("unpaused unknown state must queue");
        };
        assert_eq!(
            (p.min_fee_uba, p.fee_bips, p.executor_fee_uba),
            (100_000, 10, 200_000)
        );
        for (paused, level) in [
            (Some(true), Some(0)),
            (Some(false), Some(1)),
            (None, Some(3)),
        ] {
            panel.fxrp_direct_mint = Some(info(paused, level));
            assert!(
                matches!(
                    panel.queue_submit_fxrp_direct_mint(),
                    Action::FxrpDirectMintPaymentSubmitErr(m) if m.contains("paused")
                ),
                "{paused:?}/{level:?}"
            );
        }
    }

    /// TC-335: the FXRP Mint composer refuses an amount over the global minting cap headroom
    /// (executeDirectMinting would revert and the XRP would wait in the Core Vault); a delayed
    /// amount is still queued.
    #[test]
    fn fxrp_mint_composer_refuses_over_cap_only() {
        let mut panel = WalletPanel::new(true);
        panel.fxrp_direct_mint = Some(crate::xrpl::FxrpDirectMintInfo {
            core_vault_xrpl: "rVault".into(),
            asset_manager: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            min_fee_uba: 100_000,
            fee_bips: 10,
            executor_fee_uba: 200_000,
            minting_paused: Some(false),
            emergency_pause_level: Some(0),
            limits: Some(crate::xrpl::MintLimits {
                cap_uba: 1_000_000_000,
                supply_uba: 990_000_000,
                cap_headroom_uba: Some(10_000_000),
                window_max_now_uba: 1_000_000,
                large_threshold_uba: 4_000_000_000,
                large_delay_secs: 7200,
                granularity_uba: 1,
            }),
        });
        panel.open_fxrp_direct_mint_composer();
        let set_amount = |p: &mut WalletPanel, v: &str| {
            if let Some(ComposerPhase::FxrpDirectMint {
                flare_recipient,
                amount_xrp,
                ..
            }) = &mut p.composer
            {
                *flare_recipient = "0xabcdef0123456789abcdef0123456789abcdef01".into();
                *amount_xrp = v.into();
            }
        };
        set_amount(&mut panel, "5"); // over the window max (delayed), under the cap headroom
        assert!(matches!(
            panel.queue_submit_fxrp_direct_mint(),
            Action::FxrpDirectMintPaymentSubmit(_)
        ));
        set_amount(&mut panel, "10.000001");
        assert!(matches!(
            panel.queue_submit_fxrp_direct_mint(),
            Action::FxrpDirectMintPaymentSubmitErr(m) if m.contains("cap")
        ));
    }

    fn drawn(panel: &mut WalletPanel, w: u16, h: u16) -> String {
        crate::test_support::render_to_string(w, h, |f| panel.draw(f, f.area()).unwrap())
    }

    fn unfunded_panel() -> WalletPanel {
        let mut panel = WalletPanel::new(true);
        panel.is_focused = true;
        panel.seed_address = Some(Ok("rOwner".into()));
        panel.has_received_wallet_data = true;
        panel
    }

    /// `g` works for an unfunded seed and its popup draws; `t` (composer) needs an account.
    #[test]
    fn keygen_works_unfunded_and_composer_is_gated() {
        let mut panel = unfunded_panel();
        assert!(panel.on_key_event(key('t')).unwrap().is_none());
        assert!(
            panel.composer.is_none(),
            "no composer for an unfunded account"
        );
        assert_eq!(
            panel.on_key_event(key('g')).unwrap(),
            Some(Action::GenerateWalletKeys)
        );
        panel
            .update(&Action::GenerateWalletKeysOk(GeneratedWalletKeys {
                master_seed: "sEdTestOnlySeedValue".into(),
                master_seed_hex: "DEADBEEF01".into(),
                account_id: "rTestAccountAddress123".into(),
                public_key: "aTestPublicKey99".into(),
                key_type: "ed25519".into(),
            }))
            .unwrap();
        let out = drawn(&mut panel, 100, 24);
        assert!(out.contains("New Key (local)") && out.contains("sEdTestOnlySeedValue"));

        // A composer opened earlier stays visible while the body is loading.
        let mut loading = unfunded_panel();
        loading.has_received_wallet_data = false;
        loading.composer = Some(ComposerPhase::PickKind { selected: 0 });
        assert!(drawn(&mut loading, 100, 24).contains("Choose action"));
    }

    /// A terminal narrower than the popup clips instead of panicking.
    #[test]
    fn composer_popup_narrow_terminal_does_not_panic() {
        let mut panel = unfunded_panel();
        panel.open_payment_composer();
        for (w, h) in [(40, 24), (40, 6), (5, 3)] {
            crate::test_support::render_to_string(w, h, |f| panel.render_composer(f, f.area()));
        }
        panel.composer = Some(ComposerPhase::PickKind { selected: 0 });
        crate::test_support::render_to_string(40, 24, |f| panel.render_composer(f, f.area()));
        panel.open_tx_pick_composer();
        crate::test_support::render_to_string(40, 24, |f| panel.render_composer(f, f.area()));
    }

    /// NoFreeze can never be cleared: it needs type-to-confirm; reversible flags do not.
    #[test]
    fn account_set_no_freeze_requires_confirm() {
        let mut panel = unfunded_panel();
        panel.open_account_set_composer();
        let ix = |name: &str| FLAG_OPTIONS.iter().position(|f| *f == name).unwrap();
        panel.set_flag_ix = ix("GlobalFreeze");
        assert!(matches!(
            panel.queue_submit_account_set(),
            Some(Action::AccountSetSubmit(_))
        ));
        panel.set_flag_ix = ix("NoFreeze");
        assert!(panel.queue_submit_account_set().is_none());
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::DangerConfirm { expected, .. }) if expected == "NOFREEZE"
        ));
    }

    /// A local validation error must not release the guard of a write still in flight, and a
    /// result only closes the composer that queued the write.
    #[test]
    fn submit_guard_survives_local_errors_and_stale_results() {
        let mut panel = smart_account_panel();
        panel.pending_submit = Some("in-flight".into());
        panel.open_smart_account_composer();
        assert!(
            panel.on_key_event(key('s')).unwrap().is_none(),
            "empty form"
        );
        assert_eq!(panel.pending_submit.as_deref(), Some("in-flight"));
        assert!(matches!(&panel.submit_flash, Some(SubmitFlash::Error(_))));

        let mut panel = unfunded_panel();
        panel.open_offer_create_composer();
        assert!(matches!(
            panel.on_key_event(key('s')).unwrap(),
            Some(Action::OfferCreateSubmit(_))
        ));
        panel.on_key_event(KeyEvent::from(KeyCode::Esc)).unwrap();
        panel.open_offer_create_composer();
        panel
            .update(&Action::OfferCreateSubmitOk("H".into()))
            .unwrap();
        assert!(
            panel.composer.is_some(),
            "earlier submit must not close a new composer"
        );
        assert!(panel.on_key_event(key('s')).unwrap().is_some());
        panel
            .update(&Action::OfferCreateSubmitOk("H".into()))
            .unwrap();
        assert!(
            panel.composer.is_none(),
            "its own result closes the composer"
        );
    }

    /// Flare snapshots older than the limit, or from another network, never submit.
    #[test]
    fn stale_or_foreign_network_snapshots_refuse_to_submit() {
        use std::time::Duration;
        const TO: &str = "0x1234567890abcdef1234567890abcdef12345678";
        let mut panel = smart_account_panel();
        assert!(matches!(
            smart_account_submit(&mut panel, false, TO, "1"),
            Action::SmartAccountInstructionSubmit(_)
        ));
        panel.smart_account_at = Instant::now().checked_sub(Duration::from_secs(3600));
        assert!(matches!(
            smart_account_submit(&mut panel, false, TO, "1"),
            Action::SmartAccountInstructionSubmitErr(m) if m.contains("stale")
        ));
        panel
            .update(&Action::NetworkChange(Network::Testnet))
            .unwrap();
        assert!(panel.smart_account.is_none() && panel.fxrp_direct_mint.is_none());
        assert!(matches!(
            smart_account_submit(&mut panel, false, TO, "1"),
            Action::SmartAccountInstructionSubmitErr(_)
        ));
    }

    /// Paste lands in the edited field only (same per-row filters as typing) and cannot
    /// toggle edit mode or trigger hotkeys.
    #[test]
    fn paste_inserts_only_while_editing_without_hotkeys() {
        let mut panel = unfunded_panel();
        panel.open_payment_composer();
        panel.paste("rAbc");
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::Payment { destination, .. }) if destination.is_empty()
        ));
        panel.is_form_editing = true;
        panel.paste("rAbc\n def");
        if let Some(ComposerPhase::Payment { row, .. }) = &mut panel.composer {
            *row = 1;
        }
        panel.paste("1e5");
        assert!(panel.is_form_editing, "`e` in pasted text is not a hotkey");
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::Payment { destination, amount, .. })
                if destination == "rAbcdef" && amount == "115"
        ));

        panel.open_trust_set_composer();
        panel.is_form_editing = true;
        panel.paste("USD");
        assert!(matches!(
            &panel.composer,
            Some(ComposerPhase::TrustSet { currency, .. }) if currency == "USD"
        ));
    }
}
