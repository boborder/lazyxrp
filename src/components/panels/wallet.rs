use std::{ops::ControlFlow, sync::Arc, time::Instant};

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

/// Entries of the composer's action picker. Declaration order is the list order (`as usize` is
/// the row), and [`Self::ALL`] must repeat it; Enter, the shortcut keys, Esc-back and the list
/// drawing all read from here, so a new entry means a variant plus its arms in this `impl`,
/// [`ComposerPhase::picker_kind`] and `WalletPanel::open_composer_kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComposerKind {
    Payment,
    AccountSet,
    SetRegularKey,
    OfferCreate,
    TrustSet,
    FxrpMint,
    FxrpExecute,
    OfferCancel,
    EscrowCreate,
    Escrows,
    AmmDeposit,
    AmmWithdraw,
    AccountDelete,
    SmartAccount,
    Transaction,
}

impl ComposerKind {
    const ALL: [Self; 15] = [
        Self::Payment,
        Self::AccountSet,
        Self::SetRegularKey,
        Self::OfferCreate,
        Self::TrustSet,
        Self::FxrpMint,
        Self::FxrpExecute,
        Self::OfferCancel,
        Self::EscrowCreate,
        Self::Escrows,
        Self::AmmDeposit,
        Self::AmmWithdraw,
        Self::AccountDelete,
        Self::SmartAccount,
        Self::Transaction,
    ];

    fn shortcut(self) -> char {
        match self {
            Self::Payment => '1',
            Self::AccountSet => '2',
            Self::SetRegularKey => '3',
            Self::OfferCreate => '4',
            Self::TrustSet => '5',
            Self::FxrpMint => '6',
            Self::FxrpExecute => '7',
            Self::OfferCancel => '8',
            Self::EscrowCreate => '9',
            Self::Escrows => '0',
            Self::AmmDeposit => 'd',
            Self::AmmWithdraw => 'w',
            Self::AccountDelete => 'x',
            Self::SmartAccount => 'a',
            Self::Transaction => 'm',
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Payment => "Payment",
            Self::AccountSet => "AccountSet",
            Self::SetRegularKey => "SetRegularKey",
            Self::OfferCreate => "OfferCreate",
            Self::TrustSet => "TrustSet",
            Self::FxrpMint => "FXRP Mint",
            Self::FxrpExecute => "FXRP Execute",
            Self::OfferCancel => "OfferCancel",
            Self::EscrowCreate => "EscrowCreate",
            Self::Escrows => "Escrows",
            Self::AmmDeposit => "AMMDeposit",
            Self::AmmWithdraw => "AMMWithdraw",
            Self::AccountDelete => "AccountDelete",
            Self::SmartAccount => "Smart Account",
            Self::Transaction => "Transaction",
        }
    }

    fn blurb(self) -> &'static str {
        match self {
            Self::Payment => "send XRP or IOU",
            Self::AccountSet => "flags, domain, rates",
            Self::SetRegularKey => "assign/clear key ⚠",
            Self::OfferCreate => "place DEX offer",
            Self::TrustSet => "open IOU trust line",
            Self::FxrpMint => "pay Core Vault + memo",
            Self::FxrpExecute => "paste FDC proof → Flare",
            Self::OfferCancel => "cancel own DEX offer",
            Self::EscrowCreate => "lock XRP until a time",
            Self::Escrows => "finish / cancel escrow",
            Self::AmmDeposit => "add both assets to a pool",
            Self::AmmWithdraw => "return LP tokens",
            Self::AccountDelete => "delete account, send rest ⚠",
            Self::SmartAccount => "FXRP transfer / redeem via Flare",
            Self::Transaction => "NFT mint / burn / offers, more types",
        }
    }

    fn from_shortcut(c: char) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.shortcut() == c)
    }
}

impl ComposerPhase {
    /// The picker row a form opened from, for Esc to return to. `None` for the picker itself and
    /// for the phases whose Esc goes elsewhere (`DangerConfirm` resumes its form, `Tx` returns to
    /// the type list).
    fn picker_kind(&self) -> Option<ComposerKind> {
        Some(match self {
            Self::Payment { .. } => ComposerKind::Payment,
            Self::AccountSet => ComposerKind::AccountSet,
            Self::SetRegularKey { .. } => ComposerKind::SetRegularKey,
            Self::OfferCreate { .. } => ComposerKind::OfferCreate,
            Self::TrustSet { .. } => ComposerKind::TrustSet,
            Self::FxrpDirectMint { .. } => ComposerKind::FxrpMint,
            Self::FxrpExecuteDirectMint { .. } => ComposerKind::FxrpExecute,
            Self::OfferCancel { .. } => ComposerKind::OfferCancel,
            Self::EscrowCreate { .. } => ComposerKind::EscrowCreate,
            Self::Escrows { .. } => ComposerKind::Escrows,
            Self::AmmDeposit { .. } => ComposerKind::AmmDeposit,
            Self::AmmWithdraw { .. } => ComposerKind::AmmWithdraw,
            Self::AccountDelete { .. } => ComposerKind::AccountDelete,
            Self::SmartAccount { .. } => ComposerKind::SmartAccount,
            Self::TxPick { .. } => ComposerKind::Transaction,
            Self::PickKind { .. } | Self::DangerConfirm { .. } | Self::Tx { .. } => return None,
        })
    }
}

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
/// `Break(action)`: the key is consumed and `action` (if any) is emitted; `Continue`: the next
/// handler gets the key.
type KeyFlow = ControlFlow<Option<Action>>;

/// A navigation key that began a text edit tells the app to stop treating keys as hotkeys.
fn keymap_suppression(started_edit: bool) -> Option<Action> {
    started_edit.then_some(Action::SetKeymapSuppression(true))
}

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
                        "FXRP Direct Mint Payment · {hash} · a public executor usually finalizes it within minutes (not guaranteed; no fee floor is known). Check the recipient's FXRP on Overview, or finalize it yourself: composer [7] / `-x fxrp-execute` (needs flare.fassets.execute=true, an EVM key and FLR)"
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
    /// AccountSet filter per row (`payment_edit_key` / `account_set_edit_keys`); DangerConfirm
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
        if let Some(phase @ ComposerPhase::Payment { .. }) = &mut self.composer {
            for c in chars {
                phase.payment_edit_key(&KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
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
        Ok(match self.route_key(&key) {
            ControlFlow::Break(action) => action,
            ControlFlow::Continue(()) => self.idle_key(&key),
        })
    }

    /// Offers the key to each handler in turn until one consumes it.
    fn route_key(&mut self, key: &KeyEvent) -> KeyFlow {
        if key.code == KeyCode::Esc {
            return KeyFlow::Break(self.composer_esc());
        }
        self.payment_hotkeys(key)?;
        self.danger_confirm_key(key)?;
        self.submit_key(key)?;
        self.text_edit_key(key)?;
        self.payment_submit_key(key)?;
        self.phase_nav_key(key)?;
        KeyFlow::Continue(())
    }

    /// Esc: leave text editing, then the keygen result, then step the composer back one level.
    fn composer_esc(&mut self) -> Option<Action> {
        if self.is_form_editing {
            self.is_form_editing = false;
            return None;
        }
        if self.keygen_result.is_some() {
            self.keygen_result = None;
            return None;
        }
        self.submit_in_composer = false;
        match &self.composer {
            None => None,
            Some(ComposerPhase::PickKind { .. }) => {
                self.composer = None;
                Some(Action::SetKeymapSuppression(false))
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
            Some(ComposerPhase::Tx { spec, .. }) => {
                let selected = crate::xrpl::txspec::registry()
                    .position(|s| s.tx_type == spec.tx_type)
                    .unwrap_or(0);
                self.composer = Some(ComposerPhase::TxPick { selected });
                None
            }
            Some(phase) => {
                // Every other form returns to its own picker row (`None`: handled above).
                let selected = phase.picker_kind().map_or(0, |kind| kind as usize);
                self.composer = Some(ComposerPhase::PickKind { selected });
                None
            }
        }
    }

    /// Payment-only keys: `i` XRP/IOU, `p` quote, and typing into its fields.
    fn payment_hotkeys(&mut self, key: &KeyEvent) -> KeyFlow {
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
            return KeyFlow::Break(None);
        }

        // 'p' quotes Send max via ripple_path_find (disabled while typing).
        if matches!(self.composer, Some(ComposerPhase::Payment { .. }))
            && !self.is_form_editing
            && matches!(key.code, KeyCode::Char('p') | KeyCode::Char('P'))
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            return KeyFlow::Break(self.queue_payment_quote());
        }

        if self.is_form_editing
            && let Some(phase) = &mut self.composer
            && phase.payment_edit_key(key)
        {
            return KeyFlow::Break(None);
        }
        KeyFlow::Continue(())
    }

    /// Type-to-confirm phase: it consumes every key.
    fn danger_confirm_key(&mut self, key: &KeyEvent) -> KeyFlow {
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
                        return KeyFlow::Break(action);
                    }
                    let expected = expected.clone();
                    self.set_submit_flash(SubmitFlash::Error(format!(
                        "confirmation mismatch — type {expected} exactly"
                    )));
                }
                _ => {}
            }
            return KeyFlow::Break(None);
        }
        KeyFlow::Continue(())
    }

    /// The submit shortcut of whichever form is open (Escrows: `s` finish / `c` cancel).
    fn submit_key(&mut self, key: &KeyEvent) -> KeyFlow {
        let submit = submit_shortcut(key, self.is_form_editing);
        let action = match &self.composer {
            Some(ComposerPhase::SetRegularKey { .. }) if submit => {
                self.queue_submit_set_regular_key()
            }
            Some(ComposerPhase::AccountDelete { .. }) if submit => {
                self.queue_submit_account_delete()
            }
            Some(ComposerPhase::Tx { .. }) if submit => self.queue_submit_tx(),
            Some(ComposerPhase::OfferCreate { .. }) if submit => {
                Some(self.queue_submit_offer_create())
            }
            Some(ComposerPhase::OfferCancel { .. }) if submit => self.queue_submit_offer_cancel(),
            Some(ComposerPhase::EscrowCreate { .. }) if submit => self.queue_submit_escrow_create(),
            Some(ComposerPhase::AmmDeposit { .. } | ComposerPhase::AmmWithdraw { .. })
                if submit =>
            {
                self.queue_submit_amm()
            }
            Some(ComposerPhase::Escrows { .. }) => {
                if submit_shortcut(key, false) {
                    self.queue_submit_escrow_close(false)
                } else if matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    self.queue_submit_escrow_close(true)
                } else {
                    return KeyFlow::Continue(());
                }
            }
            Some(ComposerPhase::TrustSet { .. }) if submit => self.queue_submit_trust_set(),
            Some(ComposerPhase::FxrpDirectMint { .. }) if submit => {
                Some(self.queue_submit_fxrp_direct_mint())
            }
            Some(ComposerPhase::FxrpExecuteDirectMint { .. }) if submit => {
                Some(self.queue_submit_fxrp_execute_direct_mint())
            }
            Some(ComposerPhase::SmartAccount { .. }) if submit => {
                Some(self.queue_submit_smart_account())
            }
            _ => return KeyFlow::Continue(()),
        };
        KeyFlow::Break(action)
    }

    /// Typing and Backspace into the focused text field.
    fn text_edit_key(&mut self, key: &KeyEvent) -> KeyFlow {
        if self.is_form_editing
            && let Some(target) = self.edit_target()
        {
            match key.code {
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    target.push(c);
                    return KeyFlow::Break(None);
                }
                KeyCode::Backspace => {
                    target.pop();
                    return KeyFlow::Break(None);
                }
                _ => {}
            }
        }
        KeyFlow::Continue(())
    }

    /// Validate and queue the Payment form (XRP mode never carries leftover IOU fields).
    fn payment_submit_key(&mut self, key: &KeyEvent) -> KeyFlow {
        let payment_submit_pairs = match &self.composer {
            Some(ComposerPhase::Payment {
                destination,
                amount,
                iou_currency,
                iou_issuer,
                send_max,
                is_iou,
                ..
            }) if submit_shortcut(key, self.is_form_editing) => Some((
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
                return KeyFlow::Break(None);
            }
            // XRP mode must not leak leftover IOU fields into PaymentSubmitParams.
            let (cur, iss) = if iou {
                (cur, iss)
            } else {
                (String::new(), String::new())
            };
            return KeyFlow::Break(Some(self.queue_submit_payment(d, a, cur, iss, smax)));
        }
        KeyFlow::Continue(())
    }

    /// The key for whichever form or picker is open: row navigation, edit toggles, list moves.
    fn phase_nav_key(&mut self, key: &KeyEvent) -> KeyFlow {
        match &mut self.composer {
            Some(ComposerPhase::PickKind { selected }) => {
                let picked = match key.code {
                    KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => {
                        *selected = (*selected + 1) % ComposerKind::ALL.len();
                        None
                    }
                    KeyCode::Char('k') | KeyCode::Up | KeyCode::BackTab => {
                        *selected =
                            (*selected + ComposerKind::ALL.len() - 1) % ComposerKind::ALL.len();
                        None
                    }
                    KeyCode::Enter => ComposerKind::ALL.get(*selected).copied(),
                    KeyCode::Char(c) => ComposerKind::from_shortcut(c),
                    _ => None,
                };
                KeyFlow::Break(picked.and_then(|kind| self.open_composer_kind(kind)))
            }
            Some(ComposerPhase::AccountSet) => {
                KeyFlow::Break(self.account_set_modal_key_to_action(*key))
            }
            Some(ComposerPhase::Payment { row, is_iou, .. }) => {
                let row_count: usize = if *is_iou { 5 } else { 3 };
                KeyFlow::Break(keymap_suppression(Self::composer_row_nav(
                    &mut self.is_form_editing,
                    key,
                    row,
                    row_count,
                )))
            }
            Some(ComposerPhase::SetRegularKey { .. } | ComposerPhase::AccountDelete { .. }) => {
                KeyFlow::Break(keymap_suppression(Self::composer_edit_toggle_nav(
                    &mut self.is_form_editing,
                    key,
                )))
            }
            Some(ComposerPhase::OfferCreate { row, .. }) => KeyFlow::Break(keymap_suppression(
                Self::composer_row_nav(&mut self.is_form_editing, key, row, 2),
            )),
            Some(ComposerPhase::TrustSet { row, .. }) => KeyFlow::Break(keymap_suppression(
                Self::composer_row_nav(&mut self.is_form_editing, key, row, 3),
            )),
            Some(ComposerPhase::FxrpDirectMint { row, .. }) => KeyFlow::Break(keymap_suppression(
                Self::composer_row_nav(&mut self.is_form_editing, key, row, 2),
            )),
            Some(ComposerPhase::FxrpExecuteDirectMint { .. }) => {
                KeyFlow::Break(keymap_suppression(Self::composer_edit_toggle_nav(
                    &mut self.is_form_editing,
                    key,
                )))
            }
            Some(ComposerPhase::SmartAccount { row, redeem, .. }) => {
                // `i` flips transfer ↔ redeem (not while typing, where it is a character).
                if !self.is_form_editing
                    && matches!(key.code, KeyCode::Char('i') | KeyCode::Char('I'))
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    *redeem = !*redeem;
                    *row = 0;
                    return KeyFlow::Break(None);
                }
                let rows = if *redeem { 1 } else { 2 };
                KeyFlow::Break(keymap_suppression(Self::composer_row_nav(
                    &mut self.is_form_editing,
                    key,
                    row,
                    rows,
                )))
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
                KeyFlow::Break(None)
            }
            Some(ComposerPhase::AmmDeposit { row, .. }) => KeyFlow::Break(keymap_suppression(
                Self::composer_row_nav(&mut self.is_form_editing, key, row, 2),
            )),
            Some(ComposerPhase::AmmWithdraw { row, .. }) => KeyFlow::Break(keymap_suppression(
                Self::composer_row_nav(&mut self.is_form_editing, key, row, 3),
            )),
            Some(ComposerPhase::EscrowCreate { row, .. }) => KeyFlow::Break(keymap_suppression(
                Self::composer_row_nav(&mut self.is_form_editing, key, row, 4),
            )),
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
                KeyFlow::Break(None)
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
                KeyFlow::Break(None)
            }
            Some(ComposerPhase::Tx { row, values, .. }) => {
                let rows = values.len();
                KeyFlow::Break(keymap_suppression(Self::composer_row_nav(
                    &mut self.is_form_editing,
                    key,
                    row,
                    rows,
                )))
            }
            // DangerConfirm has its own key handler; nothing is open on the idle panel.
            Some(ComposerPhase::DangerConfirm { .. }) | None => KeyFlow::Continue(()),
        }
    }

    /// Keys that open something from the idle panel: `t` composer, `g` keygen.
    fn idle_key(&mut self, key: &KeyEvent) -> Option<Action> {
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
            return Some(Action::SetKeymapSuppression(true));
        }

        if self.composer.is_none()
            && matches!(self.seed_address, Some(Ok(_)))
            && key.code == KeyCode::Char('g')
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.keygen_result = None;
            return Some(Action::GenerateWalletKeys);
        }
        None
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

    /// `i` toggles XRP / IOU (clearing IOU fields back on XRP) but is a plain character while
    /// editing.
    #[test]
    fn payment_composer_i_key() {
        // composer i toggles iou and clears iou fields back on xrp
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

        // i while editing types into destination
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

    /// Key generation works for an unfunded seed, its result clears on Esc / regenerate, and the
    /// composer is gated on a funded account.
    ///
    /// Dismissing the keygen popup (Esc) and regenerating (`g`) both drop the shown seed.
    ///
    /// `g` works for an unfunded seed and its popup draws; `t` (composer) needs an account.
    #[test]
    fn keygen_flow() {
        // result cleared on esc and on regenerate
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

        // works unfunded and composer is gated
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

    /// Payment composer: the quote key, preview and submit share one validation, XRP mode drops IOU
    /// fields, currency is uppercased and empty options are omitted.
    ///
    /// TC-194: Payment `p` requests a path quote (form fields + default 100 bps slippage);
    /// incomplete form flashes instead; QuoteOk fills Send max, QuoteErr flashes.
    ///
    /// TC-198: preview + submit use the shared validation — bad destination checksum or
    /// bad Send max stays orange and `s` refuses with a flash; valid input is green.
    #[test]
    fn payment_composer_quote_preview_and_submit() {
        // queue submit payment uppercases currency and omits empty
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

        // payment quote key requests quote and fills send max
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

        // payment submit key queues action and xrp mode drops iou fields
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

        // payment preview and submit share validation
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

    /// Identical writes are refused until their result arrives (local errors and stale results keep
    /// the guard), and stale or foreign-network snapshots refuse to submit.
    ///
    /// TC-228: double-submit guard — the same write with identical params is refused (flash)
    /// until its Ok/Err arrives; different params, or the same after the result, go through.
    ///
    /// A local validation error must not release the guard of a write still in flight, and a
    /// result only closes the composer that queued the write.
    ///
    /// Flare snapshots older than the limit, or from another network, never submit.
    #[test]
    fn submit_guards() {
        // identical submit refused until result
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

        // submit guard survives local errors and stale results
        {
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

        // stale or foreign network snapshots refuse to submit
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

    /// EscrowCreate validates then queues; the escrow list finishes or cancels the selected row.
    ///
    /// TC-219: EscrowCreate composer — `9` opens it; invalid input flashes instead of queuing;
    /// valid fields queue `EscrowCreateSubmit` (trimmed, relative durations kept for the poll task).
    ///
    /// TC-220: Escrow finish/cancel composer — `0` fetches escrows; `s` finishes, `c` cancels the
    /// selected row; condition escrows cannot be finished and unresolved sequences are refused.
    #[test]
    fn escrow_composers_contract() {
        // create composer validates then queues
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

        // list composer finishes and cancels selected
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

    /// Generic TX composer: form generation, validation and queuing; irreversible types need a
    /// typed phrase; fields, help and inline errors render.
    ///
    /// TC-255: the Transaction composer is generated from the spec — `m` opens the picker, Enter a
    /// form with one row per field (spec defaults prefilled); an invalid value flashes
    /// `<Type> · <Field>: …` and never queues; a valid form queues one `GenericTxSubmit`
    /// with every field and the `--yes` flag; an identical resubmit is refused until a result.
    ///
    /// TC-255: irreversible types (burn, accept) never queue from `s`: a valid form asks for the
    /// typed phrase (`BURN`); Esc keeps the form; a partial phrase is refused. `TxComposerOpen`
    /// (Assets `x`) opens the burn form prefilled, but only with a signing credential.
    ///
    /// TC-255: the form renders one labelled row per spec field with the focused row's help and
    /// inline validation error, and an orange/green preview line.
    #[test]
    fn tx_composer_contract() {
        // generates form validates and queues
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

        // Esc walks back: form → picker (same type selected) → action list (row 14).
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

        // burn needs typed phrase and opens prefilled
        {
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

        // renders fields help and inline errors
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

    /// The TrustSet form uppercases the currency when queuing, and its default (empty) form cannot
    /// be submitted.
    ///
    /// The TrustSet form starts empty: pressing `s` straight away must not queue a trust
    /// line to a pre-filled issuer, and a malformed form flashes instead of queueing.
    #[test]
    fn trust_set_composer_queue_and_validation() {
        // queue uppercases currency
        let mut panel = WalletPanel::new(true);
        panel.open_trust_set_composer();
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

        // default form cannot submit
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

    /// SetRegularKey (clear) and AccountSet (disable master, no-freeze) queue only after the
    /// matching typed confirmation; a plain set-key needs none.
    ///
    /// NoFreeze can never be cleared: it needs type-to-confirm; reversible flags do not.
    #[test]
    fn dangerous_account_changes_need_typed_confirmation() {
        // nonempty set regular key queues without clear confirm
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

        // empty set regular key requires clear confirm
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

        // disable master requires disable confirm
        let mut panel = WalletPanel::new(true);
        panel.is_focused = true;
        panel.open_account_set_composer();
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

        // account set no freeze requires confirm
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

    /// Smart Account composer: validation before queuing, its keys, and the not-loaded guard.
    ///
    /// TC-244: Smart Account form validation — bad / zero recipient, zero / empty / over-balance
    /// amount, bad lots and a read for another account are refused with a flash message; a valid
    /// form queues exact params (FXRP decimal → base units via the read `decimals()`).
    ///
    /// TC-244: the picker opens the composer with `a`, `i` flips transfer ↔ redeem, typing fills
    /// the amount, `s` queues; without a Smart Account read the composer does not open.
    #[test]
    fn smart_account_composer_validation_keys_and_guard() {
        // validates before queueing
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

        // keys and not loaded guard
        {
            let mut panel = smart_account_panel();
            panel.composer = Some(ComposerPhase::PickKind { selected: 0 });
            panel.on_key_event(key('a')).expect("open");
            assert!(matches!(
                &panel.composer,
                Some(ComposerPhase::SmartAccount { redeem: false, .. })
            ));
            let area = ratatui::layout::Rect::new(0, 0, 80, 24);
            let out =
                crate::test_support::render_to_string(80, 24, |f| panel.render_composer(f, area));
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

    /// FXRP Mint composer: carries the read fees, refuses while paused or over the cap, and
    /// prefills only the signer's own personal account.
    ///
    /// TC-333: the FXRP Mint composer carries the read fee views into the queued params and
    /// refuses to queue while FXRP minting is paused (mint pause or any emergency level).
    ///
    /// TC-335: the FXRP Mint composer refuses an amount over the global minting cap headroom
    /// (executeDirectMinting would revert and the XRP would wait in the Core Vault); a delayed
    /// amount is still queued.
    ///
    /// TC-367: the FXRP Mint composer prefills the signer's own personal account (so an XRP-only
    /// user needs no other Flare address) and shows it in full; a read for another account or a
    /// stale read leaves the recipient empty.
    #[test]
    fn fxrp_mint_composer_contract() {
        // refuses when paused and carries fees
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

        // refuses over cap only
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

        // prefills own personal account only
        {
            let mut panel = smart_account_panel();
            panel.fxrp_direct_mint = Some(crate::xrpl::FxrpDirectMintInfo {
                core_vault_xrpl: "rVault".into(),
                asset_manager: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                min_fee_uba: 100_000,
                fee_bips: 10,
                executor_fee_uba: 200_000,
                minting_paused: Some(false),
                emergency_pause_level: Some(0),
                limits: None,
            });
            let recipient = |p: &WalletPanel| match &p.composer {
                Some(ComposerPhase::FxrpDirectMint {
                    flare_recipient, ..
                }) => flare_recipient.clone(),
                other => panic!("composer not open: {other:?}"),
            };
            panel.open_fxrp_direct_mint_composer();
            assert_eq!(
                recipient(&panel),
                "0x1234567890abcdef1234567890abcdef12345678"
            );
            let out = drawn(&mut panel, 160, 24);
            assert!(
                out.contains("0x1234567890abcdef1234567890abcdef12345678"),
                "full address shown: {out}"
            );
            assert!(out.contains("your Smart Account personal account"), "{out}");

            panel.seed_address = Some(Ok("rSomeoneElse".into()));
            panel.open_fxrp_direct_mint_composer();
            assert_eq!(
                recipient(&panel),
                "",
                "read for another account never prefills"
            );

            panel.seed_address = Some(Ok("rOwner".into()));
            panel.smart_account_at =
                Instant::now().checked_sub(std::time::Duration::from_secs(301));
            panel.open_fxrp_direct_mint_composer();
            assert_eq!(recipient(&panel), "", "stale read never prefills");
        }
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

    /// TC-371: `ComposerKind::ALL` repeats the declaration order (`as usize` is the picker row),
    /// and no two entries share a shortcut key or a title.
    #[test]
    fn composer_kind_all_follows_declaration_order_with_unique_keys() {
        for (row, kind) in ComposerKind::ALL.into_iter().enumerate() {
            assert_eq!(kind as usize, row, "{kind:?} is out of order in ALL");
            assert_eq!(ComposerKind::from_shortcut(kind.shortcut()), Some(kind));
        }
        let keys: std::collections::HashSet<_> =
            ComposerKind::ALL.iter().map(|k| k.shortcut()).collect();
        let titles: std::collections::HashSet<_> =
            ComposerKind::ALL.iter().map(|k| k.title()).collect();
        assert_eq!(keys.len(), ComposerKind::ALL.len());
        assert_eq!(titles.len(), ComposerKind::ALL.len());
    }

    /// TC-371: the composer picker is one table — for each of its 15 entries the shortcut key and
    /// Enter on its row open the same form, Esc goes back to that same row, and the selected row
    /// is drawn as `⟩ [shortcut] title`.
    #[test]
    fn composer_picker_shortcut_enter_esc_and_list_agree_for_every_entry() {
        // (shortcut, listed title, form it opens)
        const ENTRIES: [(char, &str, &str); 15] = [
            ('1', "Payment", "Payment"),
            ('2', "AccountSet", "AccountSet"),
            ('3', "SetRegularKey", "SetRegularKey"),
            ('4', "OfferCreate", "OfferCreate"),
            ('5', "TrustSet", "TrustSet"),
            ('6', "FXRP Mint", "FxrpDirectMint"),
            ('7', "FXRP Execute", "FxrpExecuteDirectMint"),
            ('8', "OfferCancel", "OfferCancel"),
            ('9', "EscrowCreate", "EscrowCreate"),
            ('0', "Escrows", "Escrows"),
            ('d', "AMMDeposit", "AmmDeposit"),
            ('w', "AMMWithdraw", "AmmWithdraw"),
            ('x', "AccountDelete", "AccountDelete"),
            ('a', "Smart Account", "SmartAccount"),
            ('m', "Transaction", "TxPick"),
        ];
        let opened = |panel: &WalletPanel| {
            let debug = format!("{:?}", panel.composer);
            // `Some(Payment { .. })` / `Some(AccountSet)` → the variant name.
            debug
                .trim_start_matches("Some(")
                .split([' ', '(', ')'])
                .next()
                .unwrap_or_default()
                .to_string()
        };

        let mut panel = smart_account_panel();
        panel.fxrp_direct_mint = Some(crate::xrpl::FxrpDirectMintInfo {
            core_vault_xrpl: "rVault".into(),
            asset_manager: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            min_fee_uba: 100_000,
            fee_bips: 10,
            executor_fee_uba: 200_000,
            minting_paused: Some(false),
            emergency_pause_level: Some(0),
            limits: None,
        });

        for (row, (shortcut, _, form)) in ENTRIES.iter().enumerate() {
            panel.composer = Some(ComposerPhase::PickKind { selected: 0 });
            panel.on_key_event(key(*shortcut)).expect("shortcut");
            assert_eq!(opened(&panel), *form, "shortcut {shortcut}");
            panel
                .on_key_event(special(KeyCode::Esc))
                .expect("esc from form");
            assert!(
                matches!(
                    panel.composer,
                    Some(ComposerPhase::PickKind { selected }) if selected == row
                ),
                "Esc from {form} must return to row {row}: {:?}",
                panel.composer
            );

            panel.on_key_event(special(KeyCode::Enter)).expect("enter");
            assert_eq!(opened(&panel), *form, "Enter on row {row}");
        }

        // The popup is shorter than the list and scrolls with the selection, so check each row
        // while it is the selected (marked) one.
        let area = ratatui::layout::Rect::new(0, 0, 100, 30);
        for (row, (shortcut, title, _)) in ENTRIES.iter().enumerate() {
            panel.composer = Some(ComposerPhase::PickKind { selected: row });
            let out =
                crate::test_support::render_to_string(100, 30, |f| panel.render_composer(f, area));
            let marked = format!("⟩ [{shortcut}] {title}");
            assert!(
                out.contains(&marked),
                "row {row} not marked as `{marked}`:\n{out}"
            );
        }

        panel.on_key_event(special(KeyCode::Esc)).expect("close");
        assert!(panel.composer.is_none(), "Esc on the picker closes it");
    }
}
