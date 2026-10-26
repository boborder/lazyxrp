//! Composer open/validate/preview/submit + modal rendering for [`super::WalletPanel`].
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
};

use super::{ComposerPhase, DangerResume, SubmitFlash, WalletPanel};
use crate::{
    action::Action,
    components::shared::{fmt, theme},
    xrpl::{
        AccountDeleteSubmitParams, AccountSetSubmitParams, AmmDepositSubmitParams,
        AmmWithdrawSubmitParams, EscrowCloseSubmitParams, EscrowCreateSubmitParams,
        FxrpDirectMintPaymentParams, FxrpExecuteDirectMintParams, GenericTxSubmitParams,
        OfferCancelSubmitParams, OfferCreateSubmitParams, PaymentQuoteParams, PaymentSubmitParams,
        SetRegularKeySubmitParams, SmartAccountInstructionParams, TrustSetSubmitParams,
        txspec::{Fields, TxSpec},
    },
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

/// Flare reads (Core Vault, Smart Account operator/fees) refresh only while Overview is the
/// active tab; older than this, submitting from them is refused until the user revisits it.
const SNAPSHOT_MAX_AGE: Duration = Duration::from_secs(300);
const SNAPSHOT_STALE: &str = "Flare data is stale (>5 min) — open Overview to refresh, then retry";

fn snapshot_stale(at: Option<Instant>) -> bool {
    at.is_some_and(|t| t.elapsed() > SNAPSHOT_MAX_AGE)
}

impl WalletPanel {
    /// Shared `e` toggle for single-row composer phases (no row navigation).
    /// Returns `true` when the composer enters editing mode (caller must set
    /// keymap suppression). Takes the `is_form_editing` field directly so it
    /// can be called while `self.composer` is mutably borrowed.
    pub(super) fn composer_edit_toggle_nav(is_form_editing: &mut bool, key: &KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('e') | KeyCode::Char('E')
                if !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                *is_form_editing = !*is_form_editing;
                return *is_form_editing;
            }
            KeyCode::Enter if !*is_form_editing => {
                *is_form_editing = true;
                return true;
            }
            _ => {}
        }
        false
    }

    /// Shared `e` / Enter / Tab row navigation for multi-row composer phases.
    /// Advances `row` within `rows`; returns `true` when entering editing mode.
    pub(super) fn composer_row_nav(
        is_form_editing: &mut bool,
        key: &KeyEvent,
        row: &mut usize,
        rows: usize,
    ) -> bool {
        match key.code {
            KeyCode::Char('e') | KeyCode::Char('E')
                if !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                *is_form_editing = !*is_form_editing;
                return *is_form_editing;
            }
            KeyCode::Enter => {
                if *is_form_editing {
                    *row = (*row + 1) % rows;
                } else {
                    *is_form_editing = true;
                    return true;
                }
            }
            KeyCode::Char('[') | KeyCode::BackTab => {
                *row = (*row + rows - 1) % rows;
            }
            KeyCode::Char(']') | KeyCode::Tab => {
                *row = (*row + 1) % rows;
            }
            _ => {}
        }
        false
    }

    pub(super) fn open_account_set_composer(&mut self) {
        self.domain = self
            .account
            .as_ref()
            .and_then(|a| a.domain_hex.as_deref())
            .and_then(|hex| crate::xrpl::hex_to_ascii(hex.trim()))
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
            send_max: String::new(),
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

    /// Gate a destructive submit behind a type-to-confirm modal; the caller returns
    /// `None` (deferred) until the user types `expected` in the DangerConfirm phase.
    fn enter_danger_confirm(&mut self, expected: &str, hint: &str, resume: DangerResume) {
        self.composer = Some(ComposerPhase::DangerConfirm {
            expected: expected.into(),
            hint: hint.into(),
            typed: String::new(),
            resume,
        });
        self.is_form_editing = true;
        self.set_submit_flash(SubmitFlash::Error(format!(
            "type {expected} to confirm — Esc cancels"
        )));
    }

    pub(super) fn queue_submit_set_regular_key(&mut self) -> Option<Action> {
        let regular_key = match &self.composer {
            Some(ComposerPhase::SetRegularKey { regular_key }) => regular_key.clone(),
            _ => String::new(),
        };
        if regular_key.trim().is_empty() {
            self.enter_danger_confirm(
                "CLEAR",
                "Clearing regular key is dangerous — type CLEAR then Enter",
                DangerResume::SetRegularKey,
            );
            return None;
        }
        Some(self.queue_submit_set_regular_key_confirmed())
    }

    pub(super) fn queue_submit_set_regular_key_confirmed(&self) -> Action {
        // Callers restore `SetRegularKey` (empty) before confirming; DangerConfirm falls through.
        let regular_key = match &self.composer {
            Some(ComposerPhase::SetRegularKey { regular_key }) => regular_key.clone(),
            _ => String::new(),
        };
        Action::SetRegularKeySubmit(SetRegularKeySubmitParams {
            regular_key,
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            network: self.network,
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
            network: self.network,
        })
    }

    pub(super) fn open_offer_cancel_composer(&mut self) -> Action {
        self.composer = Some(ComposerPhase::OfferCancel {
            offers: None,
            selected: 0,
        });
        self.is_form_editing = false;
        Action::RefreshAccountOffers
    }

    pub(super) fn queue_submit_offer_cancel(&mut self) -> Option<Action> {
        let seq = match &self.composer {
            Some(ComposerPhase::OfferCancel {
                offers: Some(Ok(rows)),
                selected,
            }) => rows.get(*selected).map(|r| r.seq),
            _ => None,
        };
        let Some(offer_sequence) = seq else {
            self.set_submit_flash(SubmitFlash::Error("no open offer selected".into()));
            return None;
        };
        Some(Action::OfferCancelSubmit(OfferCancelSubmitParams {
            offer_sequence,
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            network: self.network,
        }))
    }

    pub(super) fn open_escrow_create_composer(&mut self) {
        self.composer = Some(ComposerPhase::EscrowCreate {
            row: 0,
            destination: String::new(),
            amount_xrp: "1".to_string(),
            finish_after: "1h".to_string(),
            cancel_after: String::new(),
        });
        self.is_form_editing = false;
    }

    /// Same validation as the poll task, so a bad form flashes instead of round-tripping.
    pub(super) fn queue_submit_escrow_create(&mut self) -> Option<Action> {
        let Some(ComposerPhase::EscrowCreate {
            destination,
            amount_xrp,
            finish_after,
            cancel_after,
            ..
        }) = &self.composer
        else {
            return None;
        };
        let params = EscrowCreateSubmitParams {
            destination: destination.trim().to_string(),
            amount_xrp: amount_xrp.trim().to_string(),
            finish_after: finish_after.trim().to_string(),
            cancel_after: cancel_after.trim().to_string(),
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            network: self.network,
        };
        if let Err(e) = crate::signing::validate_escrow_create_input(
            &params.destination,
            &params.amount_xrp,
            &params.finish_after,
            &params.cancel_after,
        ) {
            self.set_submit_flash(SubmitFlash::Error(format!("EscrowCreate · {e}")));
            return None;
        }
        Some(Action::EscrowCreateSubmit(params))
    }

    pub(super) fn open_amm_deposit_composer(&mut self) {
        self.composer = Some(ComposerPhase::AmmDeposit {
            row: 0,
            amount: "XRP:1000000".to_string(),
            amount2: String::new(),
        });
        self.is_form_editing = false;
    }

    pub(super) fn open_amm_withdraw_composer(&mut self) {
        self.composer = Some(ComposerPhase::AmmWithdraw {
            row: 0,
            asset: "XRP".to_string(),
            asset2: String::new(),
            lp_amount: String::new(),
        });
        self.is_form_editing = false;
    }

    /// Queue the open AMM deposit / withdraw after the same validation the poll task runs.
    pub(super) fn queue_submit_amm(&mut self) -> Option<Action> {
        let skip_mainnet_prompt = self.skip_mainnet_prompt;
        let network = self.network;
        let queued = match &self.composer {
            Some(ComposerPhase::AmmDeposit {
                amount, amount2, ..
            }) => crate::signing::validate_amm_deposit_specs(amount, amount2).map(|()| {
                Action::AmmDepositSubmit(AmmDepositSubmitParams {
                    amount: amount.trim().to_string(),
                    amount2: amount2.trim().to_string(),
                    skip_mainnet_prompt,
                    network,
                })
            }),
            Some(ComposerPhase::AmmWithdraw {
                asset,
                asset2,
                lp_amount,
                ..
            }) => crate::signing::validate_amm_withdraw_input(asset, asset2, lp_amount).map(|()| {
                Action::AmmWithdrawSubmit(AmmWithdrawSubmitParams {
                    asset: asset.trim().to_string(),
                    asset2: asset2.trim().to_string(),
                    lp_amount: lp_amount.trim().to_string(),
                    skip_mainnet_prompt,
                    network,
                })
            }),
            _ => return None,
        };
        match queued {
            Ok(action) => Some(action),
            Err(e) => {
                self.set_submit_flash(SubmitFlash::Error(format!("AMM · {e}")));
                None
            }
        }
    }

    pub(super) fn open_account_delete_composer(&mut self) {
        self.composer = Some(ComposerPhase::AccountDelete {
            destination: String::new(),
        });
        self.is_form_editing = false;
    }

    /// Irreversible: a valid destination goes to type-to-confirm (DELETE), never straight out.
    pub(super) fn queue_submit_account_delete(&mut self) -> Option<Action> {
        let Some(ComposerPhase::AccountDelete { destination }) = &self.composer else {
            return None;
        };
        let destination = destination.trim().to_string();
        if let Err(e) = crate::xrpl::resolve_payment_destination(&destination) {
            self.set_submit_flash(SubmitFlash::Error(format!("AccountDelete · {e}")));
            return None;
        }
        let hint =
            format!("Irreversible: deletes this account; all remaining XRP goes to {destination}");
        self.enter_danger_confirm("DELETE", &hint, DangerResume::AccountDelete { destination });
        None
    }

    pub(super) fn account_delete_action(&self, destination: String) -> Action {
        Action::AccountDeleteSubmit(AccountDeleteSubmitParams {
            destination,
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            network: self.network,
        })
    }

    pub(super) fn open_tx_pick_composer(&mut self) {
        self.composer = Some(ComposerPhase::TxPick { selected: 0 });
        self.is_form_editing = false;
    }

    /// Form generated from `spec.fields`: spec default, overridden by a matching `prefill` entry.
    pub(super) fn open_tx_composer(&mut self, spec: &'static TxSpec, prefill: &[(String, String)]) {
        let values = spec
            .fields
            .iter()
            .map(|f| {
                prefill
                    .iter()
                    .find(|(name, _)| name == f.name)
                    .map_or_else(|| f.default.to_string(), |(_, v)| v.clone())
            })
            .collect();
        self.composer = Some(ComposerPhase::Tx {
            spec,
            row: 0,
            values,
        });
        self.is_form_editing = false;
    }

    pub(super) fn tx_fields(spec: &TxSpec, values: &[String]) -> Fields {
        spec.fields
            .iter()
            .zip(values)
            .map(|(f, v)| (f.name.to_string(), v.clone()))
            .collect()
    }

    pub(super) fn tx_action(&self, spec: &TxSpec, values: &[String]) -> Action {
        Action::GenericTxSubmit(GenericTxSubmitParams {
            tx_type: spec.tx_type.to_string(),
            fields: Self::tx_fields(spec, values),
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            network: self.network,
        })
    }

    /// Same validation as the poll task, so a bad form flashes instead of round-tripping.
    /// Types with `TxSpec::confirm` never queue directly: a valid form opens type-to-confirm.
    pub(super) fn queue_submit_tx(&mut self) -> Option<Action> {
        let Some(ComposerPhase::Tx { spec, values, .. }) = &self.composer else {
            return None;
        };
        let (spec, values) = (*spec, values.clone());
        if let Err(e) = spec.validate(&Self::tx_fields(spec, &values)) {
            self.set_submit_flash(SubmitFlash::Error(format!("{} · {e}", spec.tx_type)));
            return None;
        }
        let Some(confirm) = spec.confirm else {
            return Some(self.tx_action(spec, &values));
        };
        let summary = spec.summary_line(&Self::tx_fields(spec, &values));
        let hint = format!("{} — {}", confirm.hint, fmt::truncate_middle(&summary, 120));
        self.enter_danger_confirm(confirm.phrase, &hint, DangerResume::Tx { spec, values });
        None
    }

    pub(super) fn open_escrows_composer(&mut self) -> Action {
        self.composer = Some(ComposerPhase::Escrows {
            escrows: None,
            selected: 0,
        });
        self.is_form_editing = false;
        Action::RefreshAccountEscrows
    }

    /// `cancel == false` → EscrowFinish. Refuses rows this tool cannot close.
    pub(super) fn queue_submit_escrow_close(&mut self, cancel: bool) -> Option<Action> {
        let row = match &self.composer {
            Some(ComposerPhase::Escrows {
                escrows: Some(Ok(rows)),
                selected,
            }) => rows.get(*selected).cloned(),
            _ => None,
        };
        let refuse = match &row {
            None => Some("no escrow selected"),
            Some(r) if r.offer_sequence.is_none() => {
                Some("escrow sequence unknown (creating tx lookup failed) — refresh with Esc → 0")
            }
            Some(r) if r.has_condition && !cancel => {
                Some("crypto-condition escrow: finishing needs a fulfillment (not supported)")
            }
            Some(_) => None,
        };
        if let Some(msg) = refuse {
            self.set_submit_flash(SubmitFlash::Error(msg.into()));
            return None;
        }
        let row = row?;
        Some(Action::EscrowCloseSubmit(EscrowCloseSubmitParams {
            owner: row.owner,
            offer_sequence: row.offer_sequence?,
            cancel,
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            network: self.network,
        }))
    }

    /// Starts empty (invalid until filled): any pre-filled issuer would be a trust line the
    /// user never chose.
    pub(super) fn open_trust_set_composer(&mut self) {
        self.composer = Some(ComposerPhase::TrustSet {
            row: 0,
            currency: String::new(),
            issuer: String::new(),
            limit: String::new(),
        });
        self.is_form_editing = false;
    }

    /// Same shape checks as the poll task, so a bad form flashes instead of round-tripping.
    pub(super) fn queue_submit_trust_set(&mut self) -> Option<Action> {
        let Some(ComposerPhase::TrustSet {
            currency,
            issuer,
            limit,
            ..
        }) = &self.composer
        else {
            return None;
        };
        let (currency, issuer, limit) = (
            currency.trim().to_uppercase(),
            issuer.trim().to_string(),
            limit.trim().to_string(),
        );
        let shape_ok = (currency.len() == 3 && currency.bytes().all(|b| b.is_ascii_alphanumeric()))
            || (currency.len() == 40 && currency.bytes().all(|b| b.is_ascii_hexdigit()));
        let checked = if !shape_ok || currency == "XRP" {
            Err("currency: 3 letters/digits or 40 hex chars (not XRP)".to_string())
        } else if let Err(e) = crate::signing::require_classic_address_shape("issuer", &issuer) {
            Err(e.to_string())
        } else if !limit
            .parse::<f64>()
            .is_ok_and(|v| v.is_finite() && v >= 0.0)
        {
            Err("limit: expected a number ≥ 0".to_string())
        } else {
            Ok(())
        };
        if let Err(e) = checked {
            self.set_submit_flash(SubmitFlash::Error(format!("TrustSet · {e}")));
            return None;
        }
        Some(Action::TrustSetSubmit(TrustSetSubmitParams {
            currency,
            issuer,
            limit,
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            network: self.network,
        }))
    }

    /// The signing account's own Smart Account personal account from the last Flare read. A read
    /// that is stale or for another account (watch / switched) never counts as the signer's.
    fn own_personal_account(&self) -> Option<&str> {
        let info = self.smart_account.as_ref()?;
        match &self.seed_address {
            Some(Ok(signer))
                if *signer == info.xrpl_owner && !snapshot_stale(self.smart_account_at) =>
            {
                Some(&info.personal_account)
            }
            _ => None,
        }
    }

    pub(super) fn open_fxrp_direct_mint_composer(&mut self) {
        if self.fxrp_direct_mint.is_none() {
            self.set_submit_flash(SubmitFlash::Error(
                "FXRP Mint · Core Vault not loaded yet (stay on Overview a moment)".into(),
            ));
            return;
        }
        // XRP-only users have no other Flare address: default to the personal account the
        // Smart Account instructions (composer `a`) spend from. Still editable.
        let flare_recipient = self.own_personal_account().unwrap_or_default().to_string();
        self.composer = Some(ComposerPhase::FxrpDirectMint {
            row: 0,
            flare_recipient,
            amount_xrp: "1".to_string(),
        });
        self.is_form_editing = false;
    }

    pub(super) fn queue_submit_fxrp_direct_mint(&mut self) -> Action {
        let (flare_recipient, amount_xrp) = match &self.composer {
            Some(ComposerPhase::FxrpDirectMint {
                flare_recipient,
                amount_xrp,
                ..
            }) => (flare_recipient.clone(), amount_xrp.clone()),
            _ => (String::new(), String::new()),
        };
        let Some(info) = &self.fxrp_direct_mint else {
            return Action::FxrpDirectMintPaymentSubmitErr("Core Vault not loaded yet".into());
        };
        if snapshot_stale(self.fxrp_direct_mint_at) {
            return Action::FxrpDirectMintPaymentSubmitErr(SNAPSHOT_STALE.into());
        }
        if info.minting_paused == Some(true) || info.emergency_pause_level.is_some_and(|l| l > 0) {
            return Action::FxrpDirectMintPaymentSubmitErr(
                "FXRP minting is paused on Flare (mintingPaused / emergency pause); not sending XRP to the Core Vault".into(),
            );
        }
        if let Some(limits) = &info.limits
            && let Ok(drops) = crate::xrpl::xrp_to_drops(amount_xrp.trim())
            && crate::flare::classify_mint(u128::from(drops), limits)
                == crate::flare::MintClass::CapExceeded
        {
            return Action::FxrpDirectMintPaymentSubmitErr(format!(
                "{}; the XRP would sit in the Core Vault until the cap has room",
                crate::flare::MintClass::CapExceeded
            ));
        }
        if let Err(e) = crate::signing::normalize_eth_address_hex(&flare_recipient) {
            return Action::FxrpDirectMintPaymentSubmitErr(format!("{e}"));
        }
        if amount_xrp.trim().is_empty() {
            return Action::FxrpDirectMintPaymentSubmitErr("amount required".into());
        }
        Action::FxrpDirectMintPaymentSubmit(FxrpDirectMintPaymentParams {
            core_vault_xrpl: info.core_vault_xrpl.clone(),
            min_fee_uba: info.min_fee_uba,
            fee_bips: info.fee_bips,
            executor_fee_uba: info.executor_fee_uba,
            flare_recipient,
            amount_xrp,
            custom_memo: None,
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            network: self.network,
        })
    }

    pub(super) fn open_fxrp_execute_direct_mint_composer(&mut self) {
        self.composer = Some(ComposerPhase::FxrpExecuteDirectMint {
            proof_json: String::new(),
        });
        self.is_form_editing = false;
    }

    pub(super) fn queue_submit_fxrp_execute_direct_mint(&mut self) -> Action {
        let proof_json = match &self.composer {
            Some(ComposerPhase::FxrpExecuteDirectMint { proof_json }) => proof_json.clone(),
            _ => String::new(),
        };
        if proof_json.trim().is_empty() {
            return Action::FxrpExecuteDirectMintSubmitErr("proof_json required".into());
        }
        Action::FxrpExecuteDirectMintSubmit(FxrpExecuteDirectMintParams {
            proof_json,
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            network: self.network,
        })
    }

    pub(super) fn open_smart_account_composer(&mut self) {
        if self.smart_account.is_none() {
            self.set_submit_flash(SubmitFlash::Error(
                "Smart Account · not loaded yet (stay on Overview a moment)".into(),
            ));
            return;
        }
        self.composer = Some(ComposerPhase::SmartAccount {
            row: 0,
            redeem: false,
            recipient: String::new(),
            amount: String::new(),
        });
        self.is_form_editing = false;
    }

    /// Validate the Smart Account form against the last read and queue the instruction Payment.
    /// Failures come back as `SmartAccountInstructionSubmitErr` (flash), never as a Payment.
    pub(super) fn queue_submit_smart_account(&self) -> Action {
        let err = Action::SmartAccountInstructionSubmitErr;
        let Some(ComposerPhase::SmartAccount {
            redeem,
            recipient,
            amount,
            ..
        }) = &self.composer
        else {
            return err("composer closed".into());
        };
        let Some(info) = &self.smart_account else {
            return err("Smart Account not loaded yet".into());
        };
        if snapshot_stale(self.smart_account_at) {
            return err(SNAPSHOT_STALE.into());
        }
        match &self.seed_address {
            Some(Ok(signer)) if *signer == info.xrpl_owner => {}
            _ => {
                return err(format!(
                    "Smart Account read is for {}, not the signing account",
                    crate::xrpl::sanitize_text(&info.xrpl_owner, 64)
                ));
            }
        }
        let Some(operator) = info.operator_wallets.first() else {
            return err("Flare reports no operator wallet".into());
        };
        let fee_drops = if *redeem {
            info.redeem_fee_drops
        } else {
            info.transfer_fee_drops
        };
        if let Err(e) = crate::signing::check_smart_account_fee(fee_drops) {
            return err(e.to_string());
        }
        let amount = amount.trim();
        let value = if *redeem {
            match amount.parse::<u128>() {
                Ok(lots) => lots,
                Err(_) => return err("lots must be a whole number".into()),
            }
        } else {
            match crate::signing::parse_token_amount(amount, info.fxrp_decimals) {
                Ok(units) => units,
                Err(e) => return err(format!("amount: {e}")),
            }
        };
        let encoded = if *redeem {
            crate::signing::encode_fxrp_redeem_instruction(0, value)
        } else {
            crate::signing::encode_fxrp_transfer_instruction(0, value, recipient)
        };
        if let Err(e) = encoded {
            return err(e.to_string());
        }
        if !*redeem && value > info.fxrp_balance_units {
            return err(format!(
                "amount exceeds the personal account FXRP balance ({} FXRP)",
                crate::flare::format_token_units(info.fxrp_balance_units, info.fxrp_decimals)
            ));
        }
        Action::SmartAccountInstructionSubmit(SmartAccountInstructionParams {
            redeem: *redeem,
            value,
            recipient: if *redeem {
                String::new()
            } else {
                recipient.trim().to_string()
            },
            operator_xrpl: operator.clone(),
            fee_drops,
            vault: None,
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            network: self.network,
        })
    }

    pub(super) fn queue_submit_account_set(&mut self) -> Option<Action> {
        let set = Self::label_for_flag(self.set_flag_ix);
        let clr = Self::label_for_flag(self.clear_flag_ix);
        // NoFreeze can never be cleared; DisableMaster only with a regular key. GlobalFreeze /
        // RequireAuth are reversible, so they submit directly.
        let danger = if set == "NoFreeze" {
            Some((
                "NOFREEZE",
                "NoFreeze is permanent and can never be cleared — type NOFREEZE then Enter",
            ))
        } else if set == "DisableMaster" || clr == "DisableMaster" {
            Some((
                "DISABLE",
                "DisableMaster is irreversible without regular key — type DISABLE then Enter",
            ))
        } else {
            None
        };
        if let Some((phrase, hint)) = danger
            && !matches!(
                &self.composer,
                Some(ComposerPhase::DangerConfirm {
                    resume: DangerResume::AccountSet,
                    ..
                })
            )
        {
            self.enter_danger_confirm(phrase, hint, DangerResume::AccountSet);
            return None;
        }
        Some(self.queue_submit_account_set_confirmed())
    }

    pub(super) fn queue_submit_account_set_confirmed(&self) -> Action {
        let set = Self::label_for_flag(self.set_flag_ix);
        let clr = Self::label_for_flag(self.clear_flag_ix);
        Action::AccountSetSubmit(AccountSetSubmitParams {
            set_flag: if set == "(none)" { None } else { Some(set) },
            clear_flag: if clr == "(none)" { None } else { Some(clr) },
            domain_ascii: self.domain.clone(),
            tick_size: self.tick_size.clone(),
            transfer_rate: self.transfer_rate.clone(),
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            network: self.network,
        })
    }

    pub(super) fn queue_submit_payment(
        &self,
        dest: String,
        amt: String,
        iou_currency: String,
        iou_issuer: String,
        send_max: String,
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
            send_max: Some(send_max.trim().to_string()).filter(|s| !s.is_empty()),
            skip_mainnet_prompt: self.skip_mainnet_prompt,
            network: self.network,
        })
    }

    /// Payment composer `p`: validated form → `PaymentQuote` (slippage from `[xrpl] slippage_bps`).
    pub(super) fn queue_payment_quote(&mut self) -> Option<Action> {
        let Some(ComposerPhase::Payment {
            destination,
            amount,
            iou_currency,
            iou_issuer,
            send_max,
            is_iou,
            ..
        }) = &self.composer
        else {
            return None;
        };
        if let Err(m) =
            Self::payment_validate(destination, amount, *is_iou, iou_currency, iou_issuer, None)
        {
            self.set_submit_flash(SubmitFlash::Error(format!("Quote · {m}")));
            return None;
        }
        let (iou_currency, iou_issuer) = if *is_iou {
            (
                Some(iou_currency.trim().to_uppercase()),
                Some(iou_issuer.trim().to_string()),
            )
        } else {
            (None, None)
        };
        Some(Action::PaymentQuote(PaymentQuoteParams {
            destination: destination.trim().to_string(),
            amount: amount.trim().to_string(),
            iou_currency,
            iou_issuer,
            send_max: send_max.trim().to_string(),
            slippage_bps: self.config.as_ref().map_or(100, |c| c.xrpl.slippage_bps),
        }))
    }

    pub(super) fn set_submit_flash(&mut self, flash: SubmitFlash) {
        self.submit_flash = Some(flash);
    }

    /// Shared validation (`signing::validate_payment_input`); `send_max = None` skips the
    /// Send max spec (quote accepts a bare currency prefix there).
    pub(super) fn payment_validate(
        dest: &str,
        amt: &str,
        is_iou: bool,
        currency: &str,
        issuer: &str,
        send_max: Option<&str>,
    ) -> Result<(), String> {
        crate::signing::validate_payment_input(
            dest,
            amt,
            is_iou.then_some((currency, issuer)),
            send_max,
        )
        .map_err(|e| e.to_string())
    }

    pub(super) fn payment_preview(
        destination: &str,
        amount: &str,
        iou_currency: &str,
        iou_issuer: &str,
        send_max: &str,
        is_iou: bool,
    ) -> (String, Style) {
        let destination_trimmed = destination.trim();
        let amount_trimmed = amount.trim();
        let currency_trimmed = iou_currency.trim();
        let issuer_trimmed = iou_issuer.trim();

        if destination_trimmed.is_empty() {
            return (
                "Need destination (classic r… or X-address)".to_string(),
                theme::warning_style(),
            );
        }
        if amount_trimmed.is_empty() {
            return ("Need amount".to_string(), theme::warning_style());
        }
        let iou = is_iou.then_some((currency_trimmed, issuer_trimmed));
        if let Err(e) = crate::signing::validate_payment_input(
            destination_trimmed,
            amount_trimmed,
            iou,
            Some(send_max),
        ) {
            return (e.to_string(), theme::warning_style());
        }
        let spend = if send_max.trim().is_empty() {
            String::new()
        } else {
            format!(" · spend ≤ {}", fmt::truncate_middle(send_max.trim(), 24))
        };
        if is_iou {
            return (
                format!(
                    "▸ Pay {} {currency_trimmed} (issued by {}) → {}{spend}",
                    amount_trimmed,
                    fmt::truncate_middle(issuer_trimmed, 20),
                    fmt::truncate_middle(destination_trimmed, 20)
                ),
                theme::success_style(),
            );
        }
        (
            format!(
                "▸ Send {} XRP → {}{spend}",
                amount_trimmed,
                fmt::truncate_middle(destination_trimmed, 30)
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

    pub(super) fn render_composer(&self, frame: &mut Frame, area: Rect) {
        let Some(phase) = &self.composer else {
            return;
        };

        let max_w = if matches!(phase, ComposerPhase::Tx { .. }) {
            100
        } else {
            74
        };
        let popup_w = area.width.clamp(54, max_w);
        let popup_h = match phase {
            ComposerPhase::PickKind { .. } => 16u16,
            ComposerPhase::AccountSet => 21u16,
            ComposerPhase::Payment { is_iou, .. } => {
                if *is_iou {
                    21u16
                } else {
                    17u16
                }
            }
            ComposerPhase::DangerConfirm { .. } => 12u16,
            ComposerPhase::SetRegularKey { .. } => 14u16,
            ComposerPhase::OfferCreate { .. } => 14u16,
            ComposerPhase::TrustSet { .. } => 16u16,
            ComposerPhase::FxrpDirectMint { .. } => 16u16,
            ComposerPhase::FxrpExecuteDirectMint { .. } => 16u16,
            ComposerPhase::OfferCancel { .. } => 16u16,
            ComposerPhase::EscrowCreate { .. } => 16u16,
            ComposerPhase::Escrows { .. } => 18u16,
            ComposerPhase::AmmDeposit { .. } => 14u16,
            ComposerPhase::AmmWithdraw { .. } => 15u16,
            ComposerPhase::AccountDelete { .. } => 12u16,
            ComposerPhase::SmartAccount { .. } => 16u16,
            ComposerPhase::TxPick { .. } => 14u16,
            ComposerPhase::Tx { spec, .. } => spec.fields.len() as u16 * 2 + 10,
        }
        .min(area.height.saturating_sub(2))
        .max(8);

        let popup_x = area.x + (area.width.saturating_sub(popup_w)) / 2;
        let popup_y = area.y + (area.height.saturating_sub(popup_h)) / 2;
        let popup = Rect::new(popup_x, popup_y, popup_w, popup_h).intersection(area);

        frame.render_widget(Clear, popup);
        let inner_title = match phase {
            ComposerPhase::PickKind { .. } => "Choose action",
            ComposerPhase::AccountSet => "Account settings",
            ComposerPhase::Payment { is_iou, .. } => {
                if *is_iou {
                    "Payment (IOU)"
                } else {
                    "Payment (XRP)"
                }
            }
            ComposerPhase::DangerConfirm { .. } => "Confirm dangerous action",
            ComposerPhase::SetRegularKey { .. } => "Regular key",
            ComposerPhase::OfferCreate { .. } => "Create offer",
            ComposerPhase::TrustSet { .. } => "Trust line",
            ComposerPhase::FxrpDirectMint { .. } => "FXRP Direct Mint",
            ComposerPhase::FxrpExecuteDirectMint { .. } => "FXRP Execute (C3)",
            ComposerPhase::OfferCancel { .. } => "Cancel offer",
            ComposerPhase::EscrowCreate { .. } => "Create escrow",
            ComposerPhase::Escrows { .. } => "Finish / cancel escrow",
            ComposerPhase::AmmDeposit { .. } => "AMM deposit (two-asset)",
            ComposerPhase::AmmWithdraw { .. } => "AMM withdraw",
            ComposerPhase::AccountDelete { .. } => "Delete account",
            ComposerPhase::SmartAccount { .. } => "Smart Account",
            ComposerPhase::TxPick { .. } => "Transaction",
            ComposerPhase::Tx { spec, .. } => spec.tx_type,
        };
        let block = theme::panel_block(inner_title, true);
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let label_style = theme::dim_style();
        let value_style = theme::accent_style();
        let highlight_style = Style::new().bold().underlined();

        let body = match phase {
            ComposerPhase::PickKind { selected } => {
                let hi = |i: usize| {
                    if *selected == i {
                        highlight_style
                    } else {
                        label_style
                    }
                };
                let row = |i: usize, shortcut: &str, title: &str, blurb: &str| {
                    Line::from(vec![
                        Span::raw(if *selected == i { "⟩ " } else { "  " }),
                        Span::styled(format!("[{shortcut}] {title}"), hi(i)),
                        Span::styled(format!(" — {blurb}"), theme::dim_style()),
                    ])
                };
                // Hint first and scroll to keep the selection visible: the wallet panel is often
                // shorter than the 14-row list.
                let first_row = 2u16;
                let scroll = (first_row + *selected as u16 + 1).saturating_sub(inner.height);
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        "1–9, 0, d, w, x, a, m jump · j/k move · Enter open · Esc close",
                        theme::secondary_style(),
                    )),
                    Line::from(""),
                    row(0, "1", "Payment", "send XRP or IOU"),
                    row(1, "2", "AccountSet", "flags, domain, rates"),
                    row(2, "3", "SetRegularKey", "assign/clear key ⚠"),
                    row(3, "4", "OfferCreate", "place DEX offer"),
                    row(4, "5", "TrustSet", "open IOU trust line"),
                    row(5, "6", "FXRP Mint", "pay Core Vault + memo"),
                    row(6, "7", "FXRP Execute", "paste FDC proof → Flare"),
                    row(7, "8", "OfferCancel", "cancel own DEX offer"),
                    row(8, "9", "EscrowCreate", "lock XRP until a time"),
                    row(9, "0", "Escrows", "finish / cancel escrow"),
                    row(10, "d", "AMMDeposit", "add both assets to a pool"),
                    row(11, "w", "AMMWithdraw", "return LP tokens"),
                    row(12, "x", "AccountDelete", "delete account, send rest ⚠"),
                    row(13, "a", "Smart Account", "FXRP transfer / redeem via Flare"),
                    row(
                        14,
                        "m",
                        "Transaction",
                        "NFT mint / burn / offers, more types",
                    ),
                ])
                .scroll((scroll, 0))
            }
            ComposerPhase::DangerConfirm {
                hint,
                typed,
                expected,
                ..
            } => {
                let warn = theme::warning_style();
                let typed_style = if typed.is_empty() {
                    theme::dim_style()
                } else if expected.starts_with(typed.as_str()) {
                    theme::accent_style().bold()
                } else {
                    theme::error_style().bold()
                };
                Paragraph::new(vec![
                    Line::from(Span::styled(hint.clone(), warn.bold())),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Type ", theme::dim_style()),
                        Span::styled(expected.clone(), theme::warning_style().bold()),
                        Span::styled(": ", theme::dim_style()),
                        Span::styled(typed.clone(), typed_style),
                        Span::styled("▌", theme::accent_style()),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(" Enter ", theme::accent_style().bold()),
                        Span::styled("confirm  ", theme::dim_style()),
                        Span::styled(" Esc ", theme::accent_style().bold()),
                        Span::styled("cancel", theme::dim_style()),
                    ]),
                ])
                .wrap(Wrap { trim: false })
            }
            ComposerPhase::AccountSet => {
                let net_note = if self.network.is_production() && !self.skip_mainnet_prompt {
                    " · production writes need --yes"
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
                send_max,
                is_iou,
            } => {
                let net_note = if self.network.is_production() && !self.skip_mainnet_prompt {
                    " · production sends need --yes"
                } else {
                    ""
                };
                let (preview_text, preview_st) = Self::payment_preview(
                    destination,
                    amount,
                    iou_currency,
                    iou_issuer,
                    send_max,
                    *is_iou,
                );

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
                let send_max_row = if *is_iou { 4 } else { 2 };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!(
                            "Send max      [{}]",
                            if *row == send_max_row { "*" } else { " " }
                        ),
                        if *row == send_max_row {
                            highlight_style
                        } else {
                            labels
                        },
                    ),
                    Span::styled(send_max.clone(), value_st),
                ]));
                lines.push(Line::from(Span::styled(
                    "  empty = same currency · XRP:drops or CUR:issuer:value",
                    labels,
                )));

                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(preview_text, preview_st)));
                lines.push(Line::from(Span::styled(
                    format!(
                        "[ ] Tab rows · Enter edit · i XRP/IOU · p quote Send max · e type · s send · Esc back{net_note}",
                    ),
                    theme::secondary_style(),
                )));
                Paragraph::new(lines)
            }
            ComposerPhase::SetRegularKey { regular_key } => {
                let net_note = if self.network.is_production() && !self.skip_mainnet_prompt {
                    " · production writes need --yes"
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
            ComposerPhase::AccountDelete { destination } => Paragraph::new(vec![
                Line::from(Span::styled(
                    format!(
                        "WARNING: deletes this account (irreversible){}",
                        self.production_note()
                    ),
                    theme::error_style(),
                )),
                Line::from(Span::styled(
                    "Remaining XRP → Destination · fee = owner reserve",
                    theme::dim_style(),
                )),
                Line::from(Span::styled(
                    "Needs no owned objects and ≥256 ledgers of age",
                    theme::dim_style(),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Destination ", label_style),
                    Span::styled(
                        if destination.is_empty() {
                            "(classic, or X-address for a tag)"
                        } else {
                            destination.as_str()
                        },
                        value_style,
                    ),
                ]),
                Line::from(""),
                Line::from(Span::styled(
                    "e edit · Ctrl+S / S submit (then type DELETE) · Esc back",
                    theme::secondary_style(),
                )),
            ]),
            ComposerPhase::OfferCreate {
                row,
                taker_gets,
                taker_pays,
            } => {
                let net_note = if self.network.is_production() && !self.skip_mainnet_prompt {
                    " · production writes need --yes"
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
            ComposerPhase::TrustSet {
                row,
                currency,
                issuer,
                limit,
            } => {
                let net_note = if self.network.is_production() && !self.skip_mainnet_prompt {
                    " · production writes need --yes"
                } else {
                    ""
                };
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        format!("v1: LimitAmount only (no rippling flags){net_note}"),
                        theme::secondary_style(),
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(
                            format!("Currency [{}] ", if *row == 0 { "*" } else { " " }),
                            if *row == 0 {
                                highlight_style
                            } else {
                                label_style
                            },
                        ),
                        Span::styled(currency.clone(), value_style),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            format!("Issuer   [{}] ", if *row == 1 { "*" } else { " " }),
                            if *row == 1 {
                                highlight_style
                            } else {
                                label_style
                            },
                        ),
                        Span::styled(issuer.clone(), value_style),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            format!("Limit    [{}] ", if *row == 2 { "*" } else { " " }),
                            if *row == 2 {
                                highlight_style
                            } else {
                                label_style
                            },
                        ),
                        Span::styled(limit.clone(), value_style),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled(
                        "e edit · Tab rows · Ctrl+S / S submit · Esc back",
                        theme::secondary_style(),
                    )),
                ])
            }
            ComposerPhase::FxrpDirectMint {
                row,
                flare_recipient,
                amount_xrp,
            } => {
                let vault = self
                    .fxrp_direct_mint
                    .as_ref()
                    .map(|i| i.core_vault_xrpl.as_str())
                    .unwrap_or("(not loaded)");
                let net_note = if self.network.is_production() && !self.skip_mainnet_prompt {
                    " · production writes need --yes"
                } else {
                    ""
                };
                let own_note = self
                    .own_personal_account()
                    .filter(|p| p.eq_ignore_ascii_case(flare_recipient.trim()))
                    .map(|_| {
                        "= your Smart Account personal account (a Flare RPC answer: check it on an explorer). FXRP then shows as SA FXRP on Overview"
                    })
                    .unwrap_or_default();
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        format!("Core Vault {vault}{net_note}"),
                        theme::secondary_style(),
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(
                            "Flare recipient ",
                            if *row == 0 {
                                highlight_style
                            } else {
                                label_style
                            },
                        ),
                        Span::styled(flare_recipient.clone(), value_style),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            "Amount XRP ",
                            if *row == 1 {
                                highlight_style
                            } else {
                                label_style
                            },
                        ),
                        Span::styled(amount_xrp.clone(), value_style),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Memo = DIRECT_MINTING + recipient (32 bytes)",
                        theme::dim_style(),
                    )),
                    Line::from(Span::styled(own_note, theme::dim_style())),
                    Line::from(Span::styled(
                        format!("e edit · Tab rows · s/Ctrl-S send · Esc back{net_note}"),
                        theme::secondary_style(),
                    )),
                ])
            }
            ComposerPhase::FxrpExecuteDirectMint { proof_json } => {
                let net_note = if self.network.is_production() && !self.skip_mainnet_prompt {
                    " · production writes need --yes"
                } else {
                    ""
                };
                let preview = proof_json_preview(proof_json);
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        format!("Paste FDC proof JSON (gate: flare.fassets.execute){net_note}"),
                        theme::secondary_style(),
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("proof_json ", theme::accent_style()),
                        Span::styled(preview, theme::accent_style()),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Accepts {proof,response} or {merkleProof,data}",
                        theme::dim_style(),
                    )),
                    Line::from(Span::styled(
                        format!("e edit · s/Ctrl-S submit · Esc back{net_note}"),
                        theme::secondary_style(),
                    )),
                ])
            }
            ComposerPhase::SmartAccount {
                row,
                redeem,
                recipient,
                amount,
            } => {
                let net_note = if self.network.is_production() && !self.skip_mainnet_prompt {
                    " · production writes need --yes"
                } else {
                    ""
                };
                let hi = |i: usize| {
                    if *row == i {
                        highlight_style
                    } else {
                        label_style
                    }
                };
                let pinned = self
                    .config
                    .as_ref()
                    .and_then(|c| c.flare.smart_accounts.expected_operator.as_deref());
                let (operator, fee, balance, pin_note) = match &self.smart_account {
                    Some(info) => {
                        let operator = info.operator_wallets.first().map_or_else(
                            || "(none)".to_string(),
                            |o| crate::xrpl::sanitize_text(o, 64),
                        );
                        let fee = if *redeem {
                            info.redeem_fee_drops
                        } else {
                            info.transfer_fee_drops
                        };
                        let pin_note = match crate::flare::check_operator(&operator, pinned) {
                            Ok(()) => "pinned ✓",
                            Err(_) => "NOT pinned — set [flare.smart_accounts] expected_operator",
                        };
                        (
                            operator,
                            crate::flare::uba_to_xrp_display(u128::from(fee)),
                            crate::flare::format_token_units(
                                info.fxrp_balance_units,
                                info.fxrp_decimals,
                            ),
                            pin_note,
                        )
                    }
                    None => (
                        "(not loaded)".into(),
                        String::new(),
                        String::new(),
                        "NOT pinned",
                    ),
                };
                let mut lines = vec![
                    Line::from(Span::styled(
                        if *redeem {
                            "0x02 redeem · amount = lots (FXRP → XRP on XRPL)"
                        } else {
                            "0x01 transfer · FXRP from the personal account"
                        },
                        theme::secondary_style(),
                    )),
                    Line::from(Span::styled(
                        format!(
                            "Operator {} · {pin_note}",
                            fmt::truncate_middle(&operator, 34)
                        ),
                        theme::secondary_style(),
                    )),
                    Line::from(Span::styled(
                        format!("Pays {fee} XRP fee · balance {balance} FXRP"),
                        theme::secondary_style(),
                    )),
                    Line::from(""),
                ];
                if !*redeem {
                    lines.push(Line::from(vec![
                        Span::styled("Flare recipient ", hi(0)),
                        Span::styled(recipient.clone(), value_style),
                    ]));
                }
                lines.push(Line::from(vec![
                    Span::styled(if *redeem { "Lots " } else { "Amount FXRP " }, hi(1)),
                    Span::styled(amount.clone(), value_style),
                ]));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "No DestinationTag · memo = 32-byte instruction",
                    theme::dim_style(),
                )));
                lines.push(Line::from(Span::styled(
                    format!(
                        "i transfer↔redeem · e edit · Tab rows · s/Ctrl-S send · Esc back{net_note}"
                    ),
                    theme::secondary_style(),
                )));
                Paragraph::new(lines).wrap(Wrap { trim: false })
            }
            ComposerPhase::OfferCancel { offers, selected } => {
                let net_note = if self.network.is_production() && !self.skip_mainnet_prompt {
                    " · production writes need --yes"
                } else {
                    ""
                };
                let mut lines = vec![
                    Line::from(Span::styled(
                        format!("Your open offers (seq · TakerGets → TakerPays){net_note}"),
                        theme::secondary_style(),
                    )),
                    Line::from(""),
                ];
                match offers {
                    None => lines.push(Line::from(Span::styled(
                        "loading account_offers…",
                        theme::dim_style(),
                    ))),
                    Some(Err(e)) => {
                        lines.push(Line::from(Span::styled(e.clone(), theme::error_style())))
                    }
                    Some(Ok(rows)) if rows.is_empty() => lines.push(Line::from(Span::styled(
                        "(no open offers)",
                        theme::dim_style(),
                    ))),
                    Some(Ok(rows)) => {
                        let start = selected.saturating_sub(9);
                        for (i, r) in rows.iter().enumerate().skip(start).take(10) {
                            let on = i == *selected;
                            lines.push(Line::from(Span::styled(
                                format!(
                                    "{}#{}  {} → {}",
                                    if on { "⟩ " } else { "  " },
                                    r.seq,
                                    fmt::truncate_middle(&r.taker_gets, 24),
                                    fmt::truncate_middle(&r.taker_pays, 24),
                                ),
                                if on { highlight_style } else { label_style },
                            )));
                        }
                    }
                }
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "j/k select · s cancel offer · Esc back",
                    theme::secondary_style(),
                )));
                Paragraph::new(lines)
            }
            ComposerPhase::EscrowCreate {
                row,
                destination,
                amount_xrp,
                finish_after,
                cancel_after,
            } => {
                let net_note = if self.network.is_production() && !self.skip_mainnet_prompt {
                    " · production writes need --yes"
                } else {
                    ""
                };
                let field = |i: usize, label: &str, value: &str| {
                    Line::from(vec![
                        Span::styled(
                            format!("{label:<12}[{}] ", if *row == i { "*" } else { " " }),
                            if *row == i {
                                highlight_style
                            } else {
                                label_style
                            },
                        ),
                        Span::styled(value.to_string(), value_style),
                    ])
                };
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        format!("XRP only · times from now: 90s / 30m / 2h / 7d{net_note}"),
                        theme::secondary_style(),
                    )),
                    Line::from(""),
                    field(0, "Destination", destination),
                    field(1, "Amount XRP", amount_xrp),
                    field(2, "FinishAfter", finish_after),
                    field(3, "CancelAfter", cancel_after),
                    Line::from(Span::styled(
                        "CancelAfter empty = no refund deadline (only Finish releases it)",
                        theme::dim_style(),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "e edit · Tab rows · Ctrl+S / S submit · Esc back",
                        theme::secondary_style(),
                    )),
                ])
            }
            ComposerPhase::Escrows { escrows, selected } => {
                let net_note = if self.network.is_production() && !self.skip_mainnet_prompt {
                    " · production writes need --yes"
                } else {
                    ""
                };
                let mut lines = vec![
                    Line::from(Span::styled(
                        format!("Escrows you own or receive (seq · amount → dest){net_note}"),
                        theme::secondary_style(),
                    )),
                    Line::from(""),
                ];
                let when = |label: &str, t: Option<u32>| {
                    t.map(|t| {
                        format!(
                            " {label} {}",
                            crate::xrpl::format_ripple_time_utc(u64::from(t))
                        )
                    })
                    .unwrap_or_default()
                };
                match escrows {
                    None => lines.push(Line::from(Span::styled(
                        "loading account_objects…",
                        theme::dim_style(),
                    ))),
                    Some(Err(e)) => {
                        lines.push(Line::from(Span::styled(e.clone(), theme::error_style())))
                    }
                    Some(Ok(rows)) if rows.is_empty() => {
                        lines.push(Line::from(Span::styled("(no escrows)", theme::dim_style())))
                    }
                    Some(Ok(rows)) => {
                        let start = selected.saturating_sub(4);
                        for (i, r) in rows.iter().enumerate().skip(start).take(5) {
                            let on = i == *selected;
                            let style = if on { highlight_style } else { label_style };
                            let seq = r
                                .offer_sequence
                                .map_or_else(|| "?".to_string(), |s| s.to_string());
                            lines.push(Line::from(Span::styled(
                                format!(
                                    "{}#{seq}  {} → {}{}",
                                    if on { "⟩ " } else { "  " },
                                    r.amount,
                                    fmt::truncate_middle(&r.destination, 20),
                                    if r.has_condition { "  [condition]" } else { "" },
                                ),
                                style,
                            )));
                            lines.push(Line::from(Span::styled(
                                format!(
                                    "    {}{}",
                                    when("finish ≥", r.finish_after),
                                    when(" · cancel ≥", r.cancel_after)
                                ),
                                theme::dim_style(),
                            )));
                        }
                    }
                }
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "j/k select · s finish · c cancel · Esc back",
                    theme::secondary_style(),
                )));
                Paragraph::new(lines)
            }
            ComposerPhase::AmmDeposit {
                row,
                amount,
                amount2,
            } => {
                let field = |i: usize, label: &str, value: &str| {
                    form_field(
                        *row == i,
                        label,
                        value,
                        highlight_style,
                        label_style,
                        value_style,
                    )
                };
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        format!(
                            "Maxima in both assets (XRP:drops / CUR:issuer:value){}",
                            self.production_note()
                        ),
                        theme::secondary_style(),
                    )),
                    Line::from(""),
                    field(0, "Amount", amount),
                    field(1, "Amount2", amount2),
                    Line::from(""),
                    Line::from(Span::styled(
                        "e edit · Tab rows · Ctrl+S / S submit · Esc back",
                        theme::secondary_style(),
                    )),
                ])
            }
            ComposerPhase::AmmWithdraw {
                row,
                asset,
                asset2,
                lp_amount,
            } => {
                let field = |i: usize, label: &str, value: &str| {
                    form_field(
                        *row == i,
                        label,
                        value,
                        highlight_style,
                        label_style,
                        value_style,
                    )
                };
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        format!("Pool assets: XRP or CUR:issuer{}", self.production_note()),
                        theme::secondary_style(),
                    )),
                    Line::from(""),
                    field(0, "Asset", asset),
                    field(1, "Asset2", asset2),
                    field(2, "LP amount", lp_amount),
                    Line::from(Span::styled(
                        "LP amount empty = withdraw all",
                        theme::dim_style(),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "e edit · Tab rows · Ctrl+S / S submit · Esc back",
                        theme::secondary_style(),
                    )),
                ])
            }
            ComposerPhase::TxPick { selected } => {
                let mut lines = vec![
                    Line::from(Span::styled(
                        "j/k move · Enter open · Esc back",
                        theme::secondary_style(),
                    )),
                    Line::from(""),
                ];
                for (i, spec) in crate::xrpl::txspec::registry().enumerate() {
                    lines.push(Line::from(vec![
                        Span::raw(if *selected == i { "⟩ " } else { "  " }),
                        Span::styled(
                            format!("[{}] {}", spec.family, spec.tx_type),
                            if *selected == i {
                                highlight_style
                            } else {
                                label_style
                            },
                        ),
                        Span::styled(format!(" — {}", spec.summary), theme::dim_style()),
                    ]));
                }
                let scroll = (2 + *selected as u16 + 1).saturating_sub(inner.height);
                Paragraph::new(lines).scroll((scroll, 0))
            }
            ComposerPhase::Tx { spec, row, values } => {
                let fields = Self::tx_fields(spec, values);
                let mut lines = vec![
                    Line::from(Span::styled(
                        format!("{}{}", spec.summary, self.production_note()),
                        theme::secondary_style(),
                    )),
                    Line::from(""),
                ];
                for (i, (f, v)) in spec.fields.iter().zip(values).enumerate() {
                    let on = *row == i;
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{:<18}[{}] ", f.name, if on { "*" } else { " " }),
                            if on { highlight_style } else { label_style },
                        ),
                        Span::styled(v.clone(), value_style),
                    ]));
                }
                lines.push(Line::from(""));
                if let (Some(f), Some(v)) = (spec.fields.get(*row), values.get(*row)) {
                    lines.push(Line::from(Span::styled(
                        format!("{}{}", f.help, if f.required { " (required)" } else { "" }),
                        theme::dim_style(),
                    )));
                    lines.push(match f.validate_value(v) {
                        Err(e) => Line::from(Span::styled(format!("✗ {e}"), theme::error_style())),
                        Ok(()) => Line::from(""),
                    });
                }
                let (preview, style) = match spec.validate(&fields) {
                    Ok(()) => (
                        format!("▸ {}", spec.summary_line(&fields)),
                        theme::success_style(),
                    ),
                    Err(e) => (e, theme::warning_style()),
                };
                lines.push(Line::from(Span::styled(preview, style)));
                lines.push(Line::from(""));
                let then = spec
                    .confirm
                    .map_or(String::new(), |c| format!(" (then type {})", c.phrase));
                lines.push(Line::from(Span::styled(
                    format!("e edit · Tab rows · Ctrl+S / S submit{then} · Esc back"),
                    theme::secondary_style(),
                )));
                Paragraph::new(lines).wrap(Wrap { trim: false })
            }
        };
        frame.render_widget(body, inner);
    }

    fn production_note(&self) -> &'static str {
        if self.network.is_production() && !self.skip_mainnet_prompt {
            " · production writes need --yes"
        } else {
            ""
        }
    }
}

/// First 72 chars of the pasted proof (char-based: byte slicing panics on multibyte JSON).
pub(super) fn proof_json_preview(proof_json: &str) -> String {
    if proof_json.chars().count() > 72 {
        format!("{}…", proof_json.chars().take(72).collect::<String>())
    } else {
        proof_json.to_string()
    }
}

/// `Label       [*] value` row of a composer form.
fn form_field(
    on: bool,
    label: &str,
    value: &str,
    highlight: Style,
    label_style: Style,
    value_style: Style,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<12}[{}] ", if on { "*" } else { " " }),
            if on { highlight } else { label_style },
        ),
        Span::styled(value.to_string(), value_style),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
    }

    /// TC-132: composer_row_nav — `]` wraps back to row 0 after a full cycle,
    /// Enter while editing advances the row, Ctrl+E is never a toggle.
    #[test]
    fn composer_row_nav_wraps_and_advances() {
        let mut editing = false;
        let mut row = 0;

        // `e` enters editing (true); second `e` exits (false).
        assert!(WalletPanel::composer_row_nav(
            &mut editing,
            &key('e'),
            &mut row,
            3
        ));
        assert!(editing);
        assert_eq!(row, 0);
        assert!(!WalletPanel::composer_row_nav(
            &mut editing,
            &key('e'),
            &mut row,
            3
        ));
        assert!(!editing);

        // Enter while editing advances the row and stays in editing mode.
        editing = true;
        assert!(!WalletPanel::composer_row_nav(
            &mut editing,
            &KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
            &mut row,
            3
        ));
        assert!(editing);
        assert_eq!(row, 1);

        // `]` from row 1 wraps: 2 → 0 → 1; three presses from 0 cycle back to 0.
        let mut row = 0;
        for expected in [1usize, 2, 0, 1] {
            WalletPanel::composer_row_nav(&mut editing, &key(']'), &mut row, 3);
            assert_eq!(row, expected);
        }

        // `[` from row 0 wraps to the last row.
        let mut row = 0;
        WalletPanel::composer_row_nav(&mut editing, &key('['), &mut row, 3);
        assert_eq!(row, 2);

        // Ctrl+E must not toggle editing.
        editing = false;
        assert!(!WalletPanel::composer_row_nav(
            &mut editing,
            &KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
            &mut row,
            3
        ));
        assert!(!editing);
        assert_eq!(row, 2);
    }

    /// TC-132: composer_edit_toggle_nav for single-row phases — `e` toggles,
    /// Enter enters editing when not editing and is a no-op while editing.
    #[test]
    fn composer_edit_toggle_nav_toggles_and_enters() {
        let mut editing = false;
        assert!(WalletPanel::composer_edit_toggle_nav(
            &mut editing,
            &key('e')
        ));
        assert!(editing);
        assert!(!WalletPanel::composer_edit_toggle_nav(
            &mut editing,
            &key('e')
        ));
        assert!(!editing);

        assert!(WalletPanel::composer_edit_toggle_nav(
            &mut editing,
            &KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())
        ));
        assert!(editing);
        assert!(!WalletPanel::composer_edit_toggle_nav(
            &mut editing,
            &KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())
        ));
        assert!(editing);
    }
}
