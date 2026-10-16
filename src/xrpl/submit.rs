//! Write path for the poll task: mainnet guard → account_info → simulate (`tesSUCCESS` +
//! request binding) → sign → submit, one `submit_*` per transaction type.

use tokio::sync::mpsc::UnboundedSender;

use super::address::{ensure_xaddress_matches_network, resolve_payment_destination};
use crate::action::Action;
use crate::network::Network;
use crate::signing;

use super::client::{RPC_TIMEOUT, RpcClient, xrp_to_drops};
use super::types::{
    AccountDeleteSubmitParams, AccountSetSubmitParams, AmmDepositSubmitParams,
    AmmWithdrawSubmitParams, EscrowCloseSubmitParams, EscrowCreateSubmitParams,
    FxrpDirectMintPaymentParams, FxrpExecuteDirectMintParams, GenericTxSubmitParams,
    OfferCancelSubmitParams, OfferCreateSubmitParams, PaymentQuoteParams, PaymentSubmitParams,
    SetRegularKeySubmitParams, SimulateResult, SmartAccountInstructionParams, TrustSetSubmitParams,
};
use serde_json::Value;
use tracing::warn;

pub(crate) async fn simulate_tx_requiring_tes_success(
    rpc: &RpcClient,
    tx_json: Value,
) -> Result<SimulateResult, String> {
    let request = tx_json.clone();
    match tokio::time::timeout(RPC_TIMEOUT, rpc.simulate_tx(tx_json)).await {
        Ok(Ok(sim)) => {
            if sim.engine_result != "tesSUCCESS" {
                Err(format!(
                    "simulate {}: {}",
                    sim.engine_result, sim.engine_result_message
                ))
            } else {
                super::parse::check_simulate_bound_to_request(&request, &sim).map(|()| sim)
            }
        }
        Ok(Err(e)) => Err(format!("simulate: {e}")),
        Err(_) => Err("simulate: timeout".to_string()),
    }
}

/// Mainnet guard, then load seed + wallet for AccountSet / Payment submit.
pub(super) fn resolve_submit_wallet<E>(
    network: &Network,
    skip_mainnet_prompt: bool,
    signing_credential: Option<&crate::signing::SigningCredential>,
    mainnet_err: &str,
    submit_err: E,
    action_tx: &UnboundedSender<Action>,
) -> Option<xrpl::wallet::Wallet>
where
    E: Fn(String) -> Action,
{
    if network.is_production() && !skip_mainnet_prompt {
        send_action(action_tx, submit_err(mainnet_err.into()));
        return None;
    }
    let Some(credential) = signing_credential else {
        send_action(
            action_tx,
            submit_err(
                "no signing credential — set XRPL_SEED, XRPL_MNEMONIC, or config [xrpl.signing]"
                    .into(),
            ),
        );
        return None;
    };
    match credential.wallet() {
        Ok(wallet) => Some(wallet),
        Err(e) => {
            send_action(action_tx, submit_err(format!("wallet: {e:?}")));
            None
        }
    }
}

pub(super) fn send_action(action_tx: &UnboundedSender<Action>, action: Action) -> bool {
    match action_tx.send(action) {
        Ok(()) => true,
        Err(e) => {
            warn!(?e, "action channel closed");
            false
        }
    }
}

pub(super) fn send_submit_failure<E>(
    action_tx: &UnboundedSender<Action>,
    submit_err: E,
    message: String,
) where
    E: Fn(String) -> Action,
{
    send_action(action_tx, submit_err(message));
    send_action(action_tx, Action::RefreshAccount);
}

/// Endpoint network check (TC-203) then `account_info` — the shared entry of every XRPL write.
pub(super) async fn fetch_account_summary_for_submit<E>(
    rpc: &RpcClient,
    network: &Network,
    account: &str,
    submit_err: E,
    action_tx: &UnboundedSender<Action>,
) -> Option<crate::xrpl::types::AccountSummary>
where
    E: Fn(String) -> Action,
{
    match tokio::time::timeout(RPC_TIMEOUT, rpc.ensure_network(network)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            send_action(action_tx, submit_err(format!("{e}")));
            return None;
        }
        Err(_) => {
            send_action(action_tx, submit_err("server_info: timeout".into()));
            return None;
        }
    }
    match tokio::time::timeout(RPC_TIMEOUT, rpc.account_info(account)).await {
        Ok(Ok(summary)) => Some(summary),
        Ok(Err(e)) => {
            send_action(action_tx, submit_err(format!("account_info: {e}")));
            None
        }
        Err(_) => {
            send_action(action_tx, submit_err("account_info: timeout".into()));
            None
        }
    }
}

pub(super) async fn finalize_simulate_sign_submit<E, FO, FS>(
    rpc: &RpcClient,
    action_tx: &UnboundedSender<Action>,
    sim: SimulateResult,
    sign_blob: FS,
    submit_err: E,
    on_ok: FO,
) where
    E: Fn(String) -> Action,
    FS: FnOnce(u32, u32, u32) -> color_eyre::Result<String>,
    FO: FnOnce(String) -> Vec<Action>,
{
    let (sequence, fee_drops, last_ledger_sequence) =
        match signing::sequence_fee_ledger_from_simulate(&sim.tx_json) {
            Ok(v) => v,
            Err(e) => {
                send_action(action_tx, submit_err(format!("{e}")));
                return;
            }
        };

    let blob = match sign_blob(sequence, fee_drops, last_ledger_sequence) {
        Ok(b) => b,
        Err(e) => {
            send_action(action_tx, submit_err(format!("sign: {e}")));
            return;
        }
    };

    match tokio::time::timeout(RPC_TIMEOUT, rpc.submit_signed_tx(&blob)).await {
        Ok(Ok(tx)) => {
            // Show the node-chosen (simulate) fee that was signed next to the hash.
            for action in on_ok(format!("{} · fee {fee_drops} drops", tx.hash)) {
                send_action(action_tx, action);
            }
        }
        Ok(Err(e)) => send_submit_failure(action_tx, submit_err, format!("submit: {e}")),
        Err(_) => send_submit_failure(action_tx, submit_err, "submit: timeout".into()),
    }
}

/// Unwrap a validation Result; on error, send `submit_err` and give the caller
/// a `None` to return on.
pub(super) fn unwrap_or_submit_err<T, E>(
    result: Result<T, impl std::fmt::Display>,
    submit_err: E,
    action_tx: &UnboundedSender<Action>,
) -> Option<T>
where
    E: Fn(String) -> Action,
{
    match result {
        Ok(v) => Some(v),
        Err(e) => {
            send_action(action_tx, submit_err(format!("{e}")));
            None
        }
    }
}

/// Shared tail of every XRPL submit: build tx_json, simulate for tesSUCCESS,
/// then sign + submit via [`finalize_simulate_sign_submit`].
pub(super) async fn simulate_then_finalize<E, FS, FO>(
    rpc: &RpcClient,
    action_tx: &UnboundedSender<Action>,
    submit_err: E,
    tx_json: color_eyre::Result<serde_json::Value>,
    sign_blob: FS,
    on_ok: FO,
) where
    E: Fn(String) -> Action + Copy,
    FS: FnOnce(u32, u32, u32) -> color_eyre::Result<String>,
    FO: FnOnce(String) -> Vec<Action>,
{
    let tx_json = match tx_json {
        Ok(j) => j,
        Err(e) => {
            send_action(action_tx, submit_err(format!("tx_json: {e}")));
            return;
        }
    };
    let sim = match simulate_tx_requiring_tes_success(rpc, tx_json).await {
        Ok(s) => s,
        Err(e) => {
            send_action(action_tx, submit_err(e));
            return;
        }
    };
    finalize_simulate_sign_submit(rpc, action_tx, sim, sign_blob, submit_err, on_ok).await;
}

/// Account fetch → build tx_json → simulate → sign → submit for an already-resolved wallet.
/// `build(account, sequence)`, `sign(wallet, account, sequence, fee, last_ledger_sequence)`.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_submit_with_wallet<E, B, S, O>(
    rpc: &RpcClient,
    network: &Network,
    wallet: &xrpl::wallet::Wallet,
    submit_err: E,
    action_tx: &UnboundedSender<Action>,
    build: B,
    sign: S,
    on_ok: O,
) where
    E: Fn(String) -> Action + Copy,
    B: FnOnce(&str, u32) -> color_eyre::Result<Value>,
    S: FnOnce(&xrpl::wallet::Wallet, &str, u32, u32, u32) -> color_eyre::Result<String>,
    O: FnOnce(String) -> Vec<Action>,
{
    let account: &str = &wallet.classic_address;
    let Some(account_info) =
        fetch_account_summary_for_submit(rpc, network, account, submit_err, action_tx).await
    else {
        return;
    };
    simulate_then_finalize(
        rpc,
        action_tx,
        submit_err,
        build(account, account_info.sequence),
        |sequence, fee_drops, last_ledger_sequence| {
            sign(wallet, account, sequence, fee_drops, last_ledger_sequence)
        },
        on_ok,
    )
    .await;
}

/// Mainnet guard + wallet, then [`run_submit_with_wallet`]. Callers validate inputs first.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_submit<E, B, S, O>(
    rpc: &RpcClient,
    network: &Network,
    skip_mainnet_prompt: bool,
    signing_credential: Option<&crate::signing::SigningCredential>,
    mainnet_err: &str,
    submit_err: E,
    action_tx: &UnboundedSender<Action>,
    build: B,
    sign: S,
    on_ok: O,
) where
    E: Fn(String) -> Action + Copy,
    B: FnOnce(&str, u32) -> color_eyre::Result<Value>,
    S: FnOnce(&xrpl::wallet::Wallet, &str, u32, u32, u32) -> color_eyre::Result<String>,
    O: FnOnce(String) -> Vec<Action>,
{
    let Some(wallet) = resolve_submit_wallet(
        network,
        skip_mainnet_prompt,
        signing_credential,
        mainnet_err,
        submit_err,
        action_tx,
    ) else {
        return;
    };
    run_submit_with_wallet(
        rpc, network, &wallet, submit_err, action_tx, build, sign, on_ok,
    )
    .await;
}

pub(super) fn account_set_params_nonempty(p: &AccountSetSubmitParams) -> bool {
    let ds = p.domain_ascii.trim();
    let ts = p.tick_size.trim();
    let tr = p.transfer_rate.trim();
    signing::resolved_account_set_flag(&p.set_flag)
        || signing::resolved_account_set_flag(&p.clear_flag)
        || !ds.is_empty()
        || !ts.is_empty()
        || !tr.is_empty()
}

pub(super) async fn submit_account_set_transaction(
    rpc: &RpcClient,
    network: &Network,
    params: AccountSetSubmitParams,
    action_tx: &UnboundedSender<Action>,
    signing_credential: Option<&crate::signing::SigningCredential>,
) {
    let submit_err = Action::AccountSetSubmitErr;
    if !account_set_params_nonempty(&params) {
        send_action(
            action_tx,
            submit_err(
                "nothing to change — pick a flag and/or fill domain, tick size, transfer rate"
                    .into(),
            ),
        );
        return;
    }
    let Some(wallet) = resolve_submit_wallet(
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        "production network: restart lazyxrp with --yes to allow AccountSet writes",
        submit_err,
        action_tx,
    ) else {
        return;
    };

    let tick_size = if params.tick_size.trim().is_empty() {
        None
    } else {
        match params.tick_size.trim().parse::<u32>() {
            Ok(n) => Some(n),
            Err(_) => {
                send_action(
                    action_tx,
                    submit_err("tick size: invalid number (use 0 or 3–15)".into()),
                );
                return;
            }
        }
    };

    let transfer_rate = if params.transfer_rate.trim().is_empty() {
        None
    } else {
        match params.transfer_rate.trim().parse::<u32>() {
            Ok(n) => Some(n),
            Err(_) => {
                send_action(
                    action_tx,
                    submit_err("transfer rate: invalid number".into()),
                );
                return;
            }
        }
    };

    let domain_trim = params.domain_ascii.trim();
    let domain_hex = if domain_trim.is_empty() {
        None
    } else {
        Some(signing::domain_ascii_to_hex(domain_trim))
    };

    let set_flag = signing::parse_account_set_flag_choice(params.set_flag.as_deref());
    let clear_flag = signing::parse_account_set_flag_choice(params.clear_flag.as_deref());

    run_submit_with_wallet(
        rpc,
        network,
        &wallet,
        submit_err,
        action_tx,
        |account, sequence| {
            signing::build_account_set_tx_json_for_simulate(
                account,
                sequence,
                set_flag,
                clear_flag,
                domain_hex.as_deref(),
                tick_size,
                transfer_rate,
            )
        },
        |wallet, account, sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_account_set(
                wallet,
                account,
                sequence,
                fee_drops,
                last_ledger_sequence,
                set_flag,
                clear_flag,
                domain_hex.as_deref(),
                tick_size,
                transfer_rate,
            )
        },
        |hash| {
            vec![
                Action::AccountSetSubmitOk(hash),
                Action::RefreshAccount,
                Action::RefreshTxHistory,
            ]
        },
    )
    .await;
}

pub(super) async fn submit_trust_set_transaction(
    rpc: &RpcClient,
    network: &Network,
    params: TrustSetSubmitParams,
    action_tx: &UnboundedSender<Action>,
    signing_credential: Option<&crate::signing::SigningCredential>,
) {
    let submit_err = Action::TrustSetSubmitErr;
    let Some(currency) = unwrap_or_submit_err(
        signing::require_nonempty_field("currency", &params.currency).map(str::to_string),
        submit_err,
        action_tx,
    ) else {
        return;
    };
    let Some(issuer) = unwrap_or_submit_err(
        signing::require_classic_address_shape("issuer", &params.issuer).map(str::to_string),
        submit_err,
        action_tx,
    ) else {
        return;
    };
    let Some(limit) = unwrap_or_submit_err(
        signing::require_nonempty_field("limit", &params.limit).map(str::to_string),
        submit_err,
        action_tx,
    ) else {
        return;
    };

    run_submit(
        rpc,
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        "production network: restart lazyxrp with --yes to allow TrustSet writes",
        submit_err,
        action_tx,
        |account, sequence| {
            signing::build_trust_set_tx_json_for_simulate(
                account, &currency, &issuer, &limit, sequence,
            )
        },
        |wallet, account, sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_trust_set(
                wallet,
                account,
                &currency,
                &issuer,
                &limit,
                sequence,
                fee_drops,
                last_ledger_sequence,
            )
        },
        |hash| {
            vec![
                Action::TrustSetSubmitOk(hash),
                Action::RefreshAccount,
                Action::RefreshLines,
                Action::RefreshTxHistory,
            ]
        },
    )
    .await;
}

pub(super) async fn submit_offer_create_transaction(
    rpc: &RpcClient,
    network: &Network,
    params: OfferCreateSubmitParams,
    action_tx: &UnboundedSender<Action>,
    signing_credential: Option<&crate::signing::SigningCredential>,
) {
    let submit_err = Action::OfferCreateSubmitErr;
    let Some(taker_gets) = unwrap_or_submit_err(
        signing::require_nonempty_field("taker_gets", &params.taker_gets).map(str::to_string),
        submit_err,
        action_tx,
    ) else {
        return;
    };
    let Some(taker_pays) = unwrap_or_submit_err(
        signing::require_nonempty_field("taker_pays", &params.taker_pays).map(str::to_string),
        submit_err,
        action_tx,
    ) else {
        return;
    };

    run_submit(
        rpc,
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        "production network: restart lazyxrp with --yes to allow OfferCreate writes",
        submit_err,
        action_tx,
        |account, sequence| {
            signing::build_offer_create_tx_json_for_simulate(
                account,
                &taker_gets,
                &taker_pays,
                sequence,
            )
        },
        |wallet, account, sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_offer_create(
                wallet,
                account,
                &taker_gets,
                &taker_pays,
                sequence,
                fee_drops,
                last_ledger_sequence,
            )
        },
        |hash| {
            vec![
                Action::OfferCreateSubmitOk(hash),
                Action::RefreshAccount,
                Action::RefreshTxHistory,
            ]
        },
    )
    .await;
}

pub(super) async fn submit_offer_cancel_transaction(
    rpc: &RpcClient,
    network: &Network,
    params: OfferCancelSubmitParams,
    action_tx: &UnboundedSender<Action>,
    signing_credential: Option<&crate::signing::SigningCredential>,
) {
    let submit_err = Action::OfferCancelSubmitErr;
    run_submit(
        rpc,
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        "production network: restart lazyxrp with --yes to allow OfferCancel writes",
        submit_err,
        action_tx,
        |account, sequence| {
            signing::build_offer_cancel_tx_json_for_simulate(
                account,
                params.offer_sequence,
                sequence,
            )
        },
        |wallet, account, sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_offer_cancel(
                wallet,
                account,
                params.offer_sequence,
                sequence,
                fee_drops,
                last_ledger_sequence,
            )
        },
        |hash| {
            vec![
                Action::OfferCancelSubmitOk(hash),
                Action::RefreshAccount,
                Action::RefreshTxHistory,
            ]
        },
    )
    .await;
}

/// Seconds since the Ripple epoch, from the local clock (simulate rejects a past FinishAfter).
pub(super) fn ripple_now_secs() -> u64 {
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    unix.saturating_sub(super::RIPPLE_EPOCH_UNIX as u64)
}

pub(super) async fn submit_escrow_create_transaction(
    rpc: &RpcClient,
    network: &Network,
    params: EscrowCreateSubmitParams,
    action_tx: &UnboundedSender<Action>,
    signing_credential: Option<&crate::signing::SigningCredential>,
) {
    let submit_err = Action::EscrowCreateSubmitErr;
    let Some((amount_drops, finish_secs, cancel_secs)) = unwrap_or_submit_err(
        signing::validate_escrow_create_input(
            &params.destination,
            &params.amount_xrp,
            &params.finish_after,
            &params.cancel_after,
        ),
        submit_err,
        action_tx,
    ) else {
        return;
    };
    let destination = params.destination.trim().to_string();

    // Times are fixed once here so simulate and the signed tx carry the same values.
    let now = ripple_now_secs();
    let after = |secs: u32| {
        u32::try_from(now + u64::from(secs)).map_err(|_| "escrow time out of range".to_string())
    };
    let times = after(finish_secs).and_then(|finish| {
        cancel_secs
            .map(after)
            .transpose()
            .map(|cancel| (finish, cancel))
    });
    let Some((finish_after, cancel_after)) = unwrap_or_submit_err(times, submit_err, action_tx)
    else {
        return;
    };

    run_submit(
        rpc,
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        "production network: restart lazyxrp with --yes to allow EscrowCreate writes",
        submit_err,
        action_tx,
        |account, sequence| {
            signing::build_escrow_create_tx_json_for_simulate(
                account,
                &destination,
                amount_drops,
                finish_after,
                cancel_after,
                sequence,
            )
        },
        |wallet, account, sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_escrow_create(
                wallet,
                account,
                &destination,
                amount_drops,
                finish_after,
                cancel_after,
                sequence,
                fee_drops,
                last_ledger_sequence,
            )
        },
        |hash| {
            vec![
                Action::EscrowCreateSubmitOk(hash),
                Action::RefreshAccount,
                Action::RefreshTxHistory,
                Action::RefreshLedgerObjects,
            ]
        },
    )
    .await;
}

pub(super) async fn submit_escrow_close_transaction(
    rpc: &RpcClient,
    network: &Network,
    params: EscrowCloseSubmitParams,
    action_tx: &UnboundedSender<Action>,
    signing_credential: Option<&crate::signing::SigningCredential>,
) {
    let submit_err = Action::EscrowCloseSubmitErr;
    let Some(owner) = unwrap_or_submit_err(
        signing::require_classic_address_shape("Owner", params.owner.trim()).map(str::to_string),
        submit_err,
        action_tx,
    ) else {
        return;
    };
    run_submit(
        rpc,
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        "production network: restart lazyxrp with --yes to allow EscrowFinish/EscrowCancel writes",
        submit_err,
        action_tx,
        |account, sequence| {
            signing::build_escrow_close_tx_json_for_simulate(
                account,
                &owner,
                params.offer_sequence,
                params.cancel,
                sequence,
            )
        },
        |wallet, account, sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_escrow_close(
                wallet,
                account,
                &owner,
                params.offer_sequence,
                params.cancel,
                sequence,
                fee_drops,
                last_ledger_sequence,
            )
        },
        |hash| {
            vec![
                Action::EscrowCloseSubmitOk(hash),
                Action::RefreshAccount,
                Action::RefreshTxHistory,
                Action::RefreshLedgerObjects,
            ]
        },
    )
    .await;
}

pub(super) async fn submit_amm_deposit_transaction(
    rpc: &RpcClient,
    network: &Network,
    params: AmmDepositSubmitParams,
    action_tx: &UnboundedSender<Action>,
    signing_credential: Option<&crate::signing::SigningCredential>,
) {
    let submit_err = Action::AmmSubmitErr;
    let (amount, amount2) = (params.amount.trim(), params.amount2.trim());
    if unwrap_or_submit_err(
        signing::validate_amm_deposit_specs(amount, amount2),
        submit_err,
        action_tx,
    )
    .is_none()
    {
        return;
    }
    run_submit(
        rpc,
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        "production network: restart lazyxrp with --yes to allow AMMDeposit writes",
        submit_err,
        action_tx,
        |account, sequence| {
            signing::build_amm_deposit_tx_json_for_simulate(account, amount, amount2, sequence)
        },
        |wallet, account, sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_amm_deposit(
                wallet,
                account,
                amount,
                amount2,
                sequence,
                fee_drops,
                last_ledger_sequence,
            )
        },
        |hash| {
            vec![
                Action::AmmSubmitOk(format!("AMMDeposit · {hash}")),
                Action::RefreshAccount,
                Action::RefreshTxHistory,
                Action::RefreshLines,
            ]
        },
    )
    .await;
}

pub(super) async fn submit_amm_withdraw_transaction(
    rpc: &RpcClient,
    network: &Network,
    params: AmmWithdrawSubmitParams,
    action_tx: &UnboundedSender<Action>,
    signing_credential: Option<&crate::signing::SigningCredential>,
) {
    let submit_err = Action::AmmSubmitErr;
    let (asset, asset2, lp) = (
        params.asset.trim(),
        params.asset2.trim(),
        params.lp_amount.trim(),
    );
    if unwrap_or_submit_err(
        signing::validate_amm_withdraw_input(asset, asset2, lp),
        submit_err,
        action_tx,
    )
    .is_none()
    {
        return;
    }
    let Some(wallet) = resolve_submit_wallet(
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        "production network: restart lazyxrp with --yes to allow AMMWithdraw writes",
        submit_err,
        action_tx,
    ) else {
        return;
    };
    let account = wallet.classic_address.clone();
    let Some(account_info) =
        fetch_account_summary_for_submit(rpc, network, &account, submit_err, action_tx).await
    else {
        return;
    };
    // LPTokenIn names the pool's LP token (currency + AMM account), read from amm_info.
    let lp_token = if lp.is_empty() {
        None
    } else {
        let fetched = tokio::time::timeout(RPC_TIMEOUT, rpc.amm_lp_token(asset, asset2))
            .await
            .map_err(|_| "amm_info: timeout".to_string())
            .and_then(|r| r.map_err(|e| e.to_string()));
        let Some(token) = unwrap_or_submit_err(fetched, submit_err, action_tx) else {
            return;
        };
        Some(token)
    };
    let lp_in = lp_token
        .as_ref()
        .map(|(currency, issuer)| (currency.as_str(), issuer.as_str(), lp));

    simulate_then_finalize(
        rpc,
        action_tx,
        submit_err,
        signing::build_amm_withdraw_tx_json_for_simulate(
            &account,
            asset,
            asset2,
            lp_in,
            account_info.sequence,
        ),
        |sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_amm_withdraw(
                &wallet,
                &account,
                asset,
                asset2,
                lp_in,
                sequence,
                fee_drops,
                last_ledger_sequence,
            )
        },
        |hash| {
            vec![
                Action::AmmSubmitOk(format!("AMMWithdraw · {hash}")),
                Action::RefreshAccount,
                Action::RefreshTxHistory,
                Action::RefreshLines,
            ]
        },
    )
    .await;
}

/// AccountDelete: the remaining XRP goes to `destination` (X-address tag kept); simulate
/// catches `tecTOO_SOON` / `tecHAS_OBLIGATIONS` / `tecDST_TAG_NEEDED` / `temDST_IS_SRC`.
pub(super) async fn submit_account_delete_transaction(
    rpc: &RpcClient,
    network: &Network,
    params: AccountDeleteSubmitParams,
    action_tx: &UnboundedSender<Action>,
    signing_credential: Option<&crate::signing::SigningCredential>,
) {
    let submit_err = Action::AccountDeleteSubmitErr;
    let Some(destination) = unwrap_or_submit_err(
        resolve_payment_destination(params.destination.trim()).and_then(|d| {
            ensure_xaddress_matches_network(&d, network)?;
            Ok(d)
        }),
        submit_err,
        action_tx,
    ) else {
        return;
    };
    run_submit(
        rpc,
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        "production network: restart lazyxrp with --yes to allow AccountDelete writes",
        submit_err,
        action_tx,
        |account, sequence| {
            signing::build_account_delete_tx_json_for_simulate(
                account,
                &destination.classic,
                destination.destination_tag,
                sequence,
            )
        },
        |wallet, account, sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_account_delete(
                wallet,
                account,
                &destination.classic,
                destination.destination_tag,
                sequence,
                fee_drops,
                last_ledger_sequence,
            )
        },
        |hash| {
            vec![
                Action::AccountDeleteSubmitOk(hash),
                Action::RefreshAccount,
                Action::RefreshTxHistory,
            ]
        },
    )
    .await;
}

/// Any spec'd transaction type (`txspec`): fields validated and `Time` fields fixed to absolute
/// seconds once, then the shared mainnet gate → `account_info` → simulate (`tesSUCCESS`, bound to
/// the request) → sign → submit. Simulate catches ledger-state failures (`tec*`) before signing.
pub(super) async fn submit_generic_transaction(
    rpc: &RpcClient,
    network: &Network,
    params: GenericTxSubmitParams,
    action_tx: &UnboundedSender<Action>,
    signing_credential: Option<&crate::signing::SigningCredential>,
) {
    let submit_err = Action::GenericTxSubmitErr;
    let Some(spec) = unwrap_or_submit_err(
        super::txspec::lookup(&params.tx_type).ok_or_else(|| {
            format!(
                "unknown transaction type {}",
                super::util::sanitize_remote(&params.tx_type)
            )
        }),
        submit_err,
        action_tx,
    ) else {
        return;
    };
    let Some(fields) = unwrap_or_submit_err(
        spec.prepare(&params.fields, ripple_now_secs()),
        submit_err,
        action_tx,
    ) else {
        return;
    };
    let mainnet_err = format!(
        "production network: restart lazyxrp with --yes to allow {} writes",
        spec.tx_type
    );
    run_submit(
        rpc,
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        &mainnet_err,
        submit_err,
        action_tx,
        |account, sequence| (spec.build_json)(account, &fields, sequence),
        |wallet, account, sequence, fee_drops, last_ledger_sequence| {
            (spec.sign)(
                wallet,
                account,
                &fields,
                sequence,
                fee_drops,
                last_ledger_sequence,
            )
        },
        |hash| {
            let mut actions = vec![
                Action::GenericTxSubmitOk(format!("{} · {hash}", spec.tx_type)),
                Action::RefreshAccount,
                Action::RefreshTxHistory,
            ];
            if spec.family == "NFT" {
                actions.push(Action::RefreshNfts);
            }
            actions
        },
    )
    .await;
}

pub(super) async fn submit_set_regular_key_transaction(
    rpc: &RpcClient,
    network: &Network,
    params: SetRegularKeySubmitParams,
    action_tx: &UnboundedSender<Action>,
    signing_credential: Option<&crate::signing::SigningCredential>,
) {
    let submit_err = Action::SetRegularKeySubmitErr;
    let regular_key_trim = params.regular_key.trim();
    let regular_key = unwrap_or_submit_err(
        if regular_key_trim.is_empty() {
            Ok(None)
        } else {
            signing::require_classic_address_shape("regular_key", regular_key_trim)
                .map(|k| Some(k.to_string()))
        },
        submit_err,
        action_tx,
    );
    let Some(regular_key) = regular_key else {
        return;
    };

    let Some(wallet) = resolve_submit_wallet(
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        "production network: restart lazyxrp with --yes to allow SetRegularKey writes",
        submit_err,
        action_tx,
    ) else {
        return;
    };
    if regular_key.as_deref() == Some(wallet.classic_address.as_str()) {
        send_action(
            action_tx,
            submit_err("regular_key must not match the account master address".into()),
        );
        return;
    }

    run_submit_with_wallet(
        rpc,
        network,
        &wallet,
        submit_err,
        action_tx,
        |account, sequence| {
            signing::build_set_regular_key_tx_json_for_simulate(
                account,
                regular_key.as_deref(),
                sequence,
            )
        },
        |wallet, account, sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_set_regular_key(
                wallet,
                account,
                regular_key.as_deref(),
                sequence,
                fee_drops,
                last_ledger_sequence,
            )
        },
        |hash| {
            vec![
                Action::SetRegularKeySubmitOk(hash),
                Action::RefreshAccount,
                Action::RefreshTxHistory,
            ]
        },
    )
    .await;
}

pub(super) async fn submit_payment_transaction(
    rpc: &RpcClient,
    network: &Network,
    params: PaymentSubmitParams,
    action_tx: &UnboundedSender<Action>,
    signing_credential: Option<&crate::signing::SigningCredential>,
) {
    let submit_err = Action::PaymentSubmitErr;
    let iou = params
        .iou_currency
        .as_deref()
        .zip(params.iou_issuer.as_deref());
    if let Err(e) = signing::validate_payment_input(
        &params.destination,
        &params.amount,
        iou,
        params.send_max.as_deref(),
    ) {
        send_action(action_tx, submit_err(e.to_string()));
        return;
    }
    // XRP payments debit `amount` drops; IOU payments only need XRP for the fee.
    let amount_drops = if iou.is_some() {
        0
    } else {
        xrp_to_drops(params.amount.trim()).unwrap_or(0)
    };
    // With SendMax the XRP debit is bounded by SendMax (0 when SendMax is an IOU).
    let xrp_debit_drops = match params.send_max.as_deref() {
        Some(sm) => match signing::send_max_xrp_drops(sm) {
            Ok(d) => d,
            Err(e) => {
                send_action(action_tx, submit_err(format!("send_max: {e}")));
                return;
            }
        },
        None => amount_drops,
    };
    let Some(destination) = unwrap_or_submit_err(
        resolve_payment_destination(params.destination.trim()),
        submit_err,
        action_tx,
    ) else {
        return;
    };
    if let Err(e) = ensure_xaddress_matches_network(&destination, network) {
        send_action(action_tx, submit_err(format!("{e}")));
        return;
    }
    let destination_resolved = destination.classic;
    let destination_tag = params.destination_tag.or(destination.destination_tag);
    let Some(wallet) = resolve_submit_wallet(
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        "production network: restart lazyxrp with --yes to allow Payment writes",
        submit_err,
        action_tx,
    ) else {
        return;
    };
    let account = wallet.classic_address.clone();
    if account == destination_resolved && params.send_max.is_none() {
        send_action(
            action_tx,
            submit_err("destination matches source account (set Send max to swap)".into()),
        );
        return;
    }

    let Some(account_info) =
        fetch_account_summary_for_submit(rpc, network, &account, submit_err, action_tx).await
    else {
        return;
    };

    let tx_json = match signing::build_payment_tx_json_for_simulate(
        &account,
        &destination_resolved,
        params.amount.trim(),
        params.iou_currency.as_deref(),
        params.iou_issuer.as_deref(),
        params.send_max.as_deref(),
        destination_tag,
        None,
        account_info.sequence,
    ) {
        Ok(j) => j,
        Err(e) => {
            send_action(action_tx, submit_err(format!("tx_json: {e}")));
            return;
        }
    };

    let sim = match simulate_tx_requiring_tes_success(rpc, tx_json).await {
        Ok(s) => s,
        Err(e) => {
            send_action(action_tx, submit_err(e));
            return;
        }
    };

    let fee_drops = match signing::sequence_fee_ledger_from_simulate(&sim.tx_json) {
        Ok((_, fee, _)) => fee,
        Err(e) => {
            send_action(action_tx, submit_err(format!("{e}")));
            return;
        }
    };

    let balance_drops = xrp_to_drops(&account_info.balance_xrp).unwrap_or(0);
    let total_need = xrp_debit_drops.saturating_add(u64::from(fee_drops));
    if balance_drops < total_need {
        send_action(
            action_tx,
            submit_err(if xrp_debit_drops == 0 {
                format!(
                    "insufficient XRP for fee: have {balance_drops} drops, need {fee_drops} fee"
                )
            } else {
                format!(
                    "insufficient balance: have {balance_drops} drops, need {total_need} (amount {xrp_debit_drops} + fee {fee_drops})"
                )
            }),
        );
        return;
    }

    finalize_simulate_sign_submit(
        rpc,
        action_tx,
        sim,
        |sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_payment(
                &wallet,
                &account,
                &destination_resolved,
                params.amount.trim(),
                params.iou_currency.as_deref(),
                params.iou_issuer.as_deref(),
                params.send_max.as_deref(),
                destination_tag,
                None,
                sequence,
                fee_drops,
                last_ledger_sequence,
            )
        },
        submit_err,
        |hash| {
            vec![
                Action::PaymentSubmitOk(hash),
                Action::RefreshAccount,
                Action::RefreshTxHistory,
            ]
        },
    )
    .await;
}

pub(super) async fn submit_fxrp_direct_mint_payment(
    rpc: &RpcClient,
    network: &Network,
    params: FxrpDirectMintPaymentParams,
    expected_core_vault: Option<&str>,
    action_tx: &UnboundedSender<Action>,
    signing_credential: Option<&crate::signing::SigningCredential>,
) {
    let submit_err = Action::FxrpDirectMintPaymentSubmitErr;
    let Some(memo) = unwrap_or_submit_err(
        signing::build_direct_mint_memo_data(&params.flare_recipient),
        submit_err,
        action_tx,
    ) else {
        return;
    };
    let amount_drops = match xrp_to_drops(params.amount_xrp.trim()) {
        Ok(v) => v,
        Err(e) => {
            send_action(action_tx, submit_err(format!("amount: {e}")));
            return;
        }
    };
    if amount_drops == 0 {
        send_action(
            action_tx,
            submit_err("amount must be greater than zero".into()),
        );
        return;
    }
    let Some(destination) = unwrap_or_submit_err(
        resolve_payment_destination(params.core_vault_xrpl.trim()),
        submit_err,
        action_tx,
    ) else {
        return;
    };
    if destination.destination_tag.is_some() {
        send_action(
            action_tx,
            submit_err("Core Vault destination must be classic address (no X-address tag)".into()),
        );
        return;
    }
    let destination_resolved = destination.classic;
    let Some(wallet) = resolve_submit_wallet(
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        "production network: restart lazyxrp with --yes to allow FXRP Direct Mint Payment writes",
        submit_err,
        action_tx,
    ) else {
        return;
    };
    let account = wallet.classic_address.clone();
    if account == destination_resolved {
        send_action(
            action_tx,
            submit_err("Core Vault destination matches source account".into()),
        );
        return;
    }
    if let Err(e) = crate::flare::check_core_vault(&destination_resolved, expected_core_vault) {
        send_action(action_tx, submit_err(e));
        return;
    }

    let Some(account_info) =
        fetch_account_summary_for_submit(rpc, network, &account, submit_err, action_tx).await
    else {
        return;
    };

    let tx_json = match signing::build_payment_tx_json_for_simulate(
        &account,
        &destination_resolved,
        params.amount_xrp.trim(),
        None,
        None,
        None,
        None,
        Some(memo.as_str()),
        account_info.sequence,
    ) {
        Ok(j) => j,
        Err(e) => {
            send_action(action_tx, submit_err(format!("tx_json: {e}")));
            return;
        }
    };

    let sim = match simulate_tx_requiring_tes_success(rpc, tx_json).await {
        Ok(s) => s,
        Err(e) => {
            send_action(action_tx, submit_err(e));
            return;
        }
    };

    let fee_drops = match signing::sequence_fee_ledger_from_simulate(&sim.tx_json) {
        Ok((_, fee, _)) => fee,
        Err(e) => {
            send_action(action_tx, submit_err(format!("{e}")));
            return;
        }
    };

    let balance_drops = xrp_to_drops(&account_info.balance_xrp).unwrap_or(0);
    let total_need = amount_drops.saturating_add(u64::from(fee_drops));
    if balance_drops < total_need {
        send_action(
            action_tx,
            submit_err(format!(
                "insufficient balance: have {balance_drops} drops, need {total_need} (amount {amount_drops} + fee {fee_drops})"
            )),
        );
        return;
    }

    finalize_simulate_sign_submit(
        rpc,
        action_tx,
        sim,
        |sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_payment(
                &wallet,
                &account,
                &destination_resolved,
                params.amount_xrp.trim(),
                None,
                None,
                None,
                None,
                Some(memo.as_str()),
                sequence,
                fee_drops,
                last_ledger_sequence,
            )
        },
        submit_err,
        |hash| {
            vec![
                Action::FxrpDirectMintPaymentSubmitOk(hash),
                Action::RefreshAccount,
                Action::RefreshTxHistory,
            ]
        },
    )
    .await;
}

/// Smart Account instruction (`0x01` FXRP transfer / `0x02` redeem): a Payment of exactly
/// `getInstructionFee(id)` drops to the pinned operator, no `DestinationTag`, one 32-byte memo.
/// The operator (and fee) come from one Flare RPC answer, so signing needs
/// `[flare.smart_accounts] expected_operator` to match.
pub(super) async fn submit_smart_account_instruction(
    rpc: &RpcClient,
    network: &Network,
    params: SmartAccountInstructionParams,
    expected_operator: Option<&str>,
    action_tx: &UnboundedSender<Action>,
    signing_credential: Option<&crate::signing::SigningCredential>,
) {
    let submit_err = Action::SmartAccountInstructionSubmitErr;
    // Wallet id 0: "use 0 if unassigned" (Flare Smart Accounts docs).
    let reference = if params.redeem {
        signing::encode_fxrp_redeem_instruction(0, params.value)
    } else {
        signing::encode_fxrp_transfer_instruction(0, params.value, &params.recipient)
    };
    let Some(reference) = unwrap_or_submit_err(reference, submit_err, action_tx) else {
        return;
    };
    let memo = signing::smart_account_memo_data(&reference);
    if unwrap_or_submit_err(
        signing::check_smart_account_fee(params.fee_drops),
        submit_err,
        action_tx,
    )
    .is_none()
    {
        return;
    }
    let Some(destination) = unwrap_or_submit_err(
        resolve_payment_destination(params.operator_xrpl.trim()),
        submit_err,
        action_tx,
    ) else {
        return;
    };
    if destination.destination_tag.is_some() {
        send_action(
            action_tx,
            submit_err("operator must be a classic address (no X-address tag)".into()),
        );
        return;
    }
    let operator = destination.classic;
    let Some(wallet) = resolve_submit_wallet(
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        "production network: restart lazyxrp with --yes to allow Smart Account instruction writes",
        submit_err,
        action_tx,
    ) else {
        return;
    };
    let account = wallet.classic_address.clone();
    if account == operator {
        send_action(
            action_tx,
            submit_err("operator matches source account".into()),
        );
        return;
    }
    if let Err(e) = crate::flare::check_operator(&operator, expected_operator) {
        send_action(action_tx, submit_err(e));
        return;
    }

    let Some(account_info) =
        fetch_account_summary_for_submit(rpc, network, &account, submit_err, action_tx).await
    else {
        return;
    };

    let amount_xrp = super::format::drops_to_xrp(&params.fee_drops.to_string());
    let tx_json = match signing::build_payment_tx_json_for_simulate(
        &account,
        &operator,
        &amount_xrp,
        None,
        None,
        None,
        None,
        Some(memo.as_str()),
        account_info.sequence,
    ) {
        Ok(j) => j,
        Err(e) => {
            send_action(action_tx, submit_err(format!("tx_json: {e}")));
            return;
        }
    };

    let sim = match simulate_tx_requiring_tes_success(rpc, tx_json).await {
        Ok(s) => s,
        Err(e) => {
            send_action(action_tx, submit_err(e));
            return;
        }
    };

    let network_fee_drops = match signing::sequence_fee_ledger_from_simulate(&sim.tx_json) {
        Ok((_, fee, _)) => fee,
        Err(e) => {
            send_action(action_tx, submit_err(format!("{e}")));
            return;
        }
    };

    let balance_drops = xrp_to_drops(&account_info.balance_xrp).unwrap_or(0);
    let total_need = params
        .fee_drops
        .saturating_add(u64::from(network_fee_drops));
    if balance_drops < total_need {
        send_action(
            action_tx,
            submit_err(format!(
                "insufficient balance: have {balance_drops} drops, need {total_need} (instruction fee {} + network fee {network_fee_drops})",
                params.fee_drops
            )),
        );
        return;
    }

    finalize_simulate_sign_submit(
        rpc,
        action_tx,
        sim,
        |sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_payment(
                &wallet,
                &account,
                &operator,
                &amount_xrp,
                None,
                None,
                None,
                None,
                Some(memo.as_str()),
                sequence,
                fee_drops,
                last_ledger_sequence,
            )
        },
        submit_err,
        |hash| {
            vec![
                Action::SmartAccountInstructionSubmitOk(hash),
                Action::RefreshAccount,
                Action::RefreshTxHistory,
            ]
        },
    )
    .await;
}

/// `ripple_path_find` from the signing account in the Send max field's currency →
/// Send max spec with slippage. Read-only; signing still goes through simulate.
pub(super) async fn quote_payment(
    rpc: &RpcClient,
    source: &str,
    params: PaymentQuoteParams,
) -> Result<String, String> {
    let is_iou = params.iou_currency.is_some() && params.iou_issuer.is_some();
    let source_currency = super::format::quote_source_currency(&params.send_max, is_iou)?;
    let destination = resolve_payment_destination(params.destination.trim())
        .map_err(|e| e.to_string())?
        .classic;
    let dest_amount = match (&params.iou_currency, &params.iou_issuer) {
        (Some(currency), Some(issuer)) => serde_json::json!({
            "currency": currency,
            "issuer": issuer,
            "value": params.amount.trim(),
        }),
        _ => serde_json::json!(
            xrp_to_drops(params.amount.trim())
                .map_err(|e| format!("amount: {e}"))?
                .to_string()
        ),
    };
    let result = tokio::time::timeout(
        RPC_TIMEOUT,
        rpc.ripple_path_find(
            source,
            &destination,
            &dest_amount,
            Some(std::slice::from_ref(&source_currency)),
        ),
    )
    .await
    .map_err(|_| "ripple_path_find: timeout".to_string())?
    .map_err(|e| format!("ripple_path_find: {e}"))?;
    super::format::send_max_spec_from_quote(&result, &source_currency, params.slippage_bps)
}

pub(super) async fn submit_fxrp_execute_direct_mint(
    flare_rpc_url: Option<&str>,
    flare_fassets_execute: bool,
    flare_evm_key_env: &str,
    flare_evm_key: Option<&secrecy::SecretString>,
    network: &Network,
    params: FxrpExecuteDirectMintParams,
    action_tx: &UnboundedSender<Action>,
) {
    let submit_err = Action::FxrpExecuteDirectMintSubmitErr;
    if let Err(e) = crate::flare::ensure_fassets_execute_enabled(flare_fassets_execute) {
        send_action(action_tx, submit_err(format!("{e}")));
        return;
    }
    if network.is_production() && !params.skip_mainnet_prompt {
        send_action(
            action_tx,
            submit_err(
                "production network: restart lazyxrp with --yes to allow Flare executeDirectMinting writes"
                    .into(),
            ),
        );
        return;
    }
    if params.proof_json.trim().is_empty() {
        send_action(action_tx, submit_err("proof_json required".into()));
        return;
    }
    let Some(rpc) = flare_rpc_url.filter(|u| !u.trim().is_empty()) else {
        send_action(action_tx, submit_err("flare RPC URL not configured".into()));
        return;
    };
    let Some(key) = flare_evm_key else {
        send_action(
            action_tx,
            submit_err(format!(
                "env {flare_evm_key_env} was not set at startup (Flare EVM executor key)"
            )),
        );
        return;
    };
    match crate::flare::execute_direct_minting(
        rpc,
        secrecy::ExposeSecret::expose_secret(key),
        &params.proof_json,
    )
    .await
    {
        Ok(hash) => send_action(action_tx, Action::FxrpExecuteDirectMintSubmitOk(hash)),
        Err(e) => send_action(action_tx, submit_err(format!("{e}"))),
    };
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::*;

    /// TC-107: closed action channel returns failure without panicking
    #[test]
    fn send_action_reports_closed_channel() {
        let (tx, rx) = mpsc::unbounded_channel();
        drop(rx);
        assert!(!send_action(&tx, Action::RefreshAccount));
    }

    /// TC-106: submit failure emits error and refreshes account sequence state
    #[test]
    fn submit_failure_requests_account_resync() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        send_submit_failure(&tx, Action::PaymentSubmitErr, "submit: tefPAST_SEQ".into());
        assert!(matches!(
            rx.try_recv().unwrap(),
            Action::PaymentSubmitErr(message) if message == "submit: tefPAST_SEQ"
        ));
        assert!(matches!(rx.try_recv().unwrap(), Action::RefreshAccount));
    }

    /// TC-199: poll write paths against local JSON-RPC — Payment and OfferCancel run
    /// account_info → simulate → submit and emit Ok; quote sends `source_currencies`
    /// (XRP for an IOU Amount) and returns the slippage-adjusted Send max spec.
    #[tokio::test]
    async fn write_paths_and_quote_against_local_rpc() {
        use crate::test_support::rpc_mock;
        use secrecy::SecretString;
        use serde_json::{Value, json};
        use tokio::net::TcpListener;

        const GENESIS: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        fn result_for(method: &str) -> Value {
            match method {
                "server_info" => json!({"info": {"network_id": 1, "validated_ledger": {"seq": 1}}}),
                "account_info" => json!({"account_data": {
                    "Account": GENESIS, "Balance": "10000000", "Sequence": 7,
                    "OwnerCount": 0, "Flags": 0
                }}),
                "simulate" => json!({
                    "engine_result": "tesSUCCESS",
                    // rippled 3.4.1 shape: no LastLedgerSequence; filled from ledger_index (TC-200).
                    "ledger_index": 80_000_000,
                    "tx_json": {"Sequence": 7, "Fee": "12"}
                }),
                "submit" => json!({"engine_result": "tesSUCCESS", "tx_json": {"hash": "ABC"}}),
                "ripple_path_find" => json!({
                    "alternatives": [
                        {"paths_computed": [], "source_amount": "2000000"},
                        {"paths_computed": [], "source_amount": "1000000"}
                    ],
                    "destination_account": GENESIS,
                    "destination_amount": {"currency": "USD", "issuer": GENESIS, "value": "10"}
                }),
                other => panic!("unexpected local RPC method: {other}"),
            }
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(rpc_mock::serve(listener, 10, result_for));
        let rpc = RpcClient::connect(&url).expect("rpc client");
        let credential = crate::signing::credential_from_secrets(
            Some(&SecretString::from("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB")),
            None,
        )
        .expect("credential")
        .expect("seed set");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let first = |rx: &mut mpsc::UnboundedReceiver<Action>| {
            let a = rx.try_recv().expect("action");
            while rx.try_recv().is_ok() {}
            a
        };

        submit_payment_transaction(
            &rpc,
            &Network::Testnet,
            PaymentSubmitParams {
                destination: GENESIS.into(),
                amount: "1".into(),
                iou_currency: None,
                iou_issuer: None,
                destination_tag: None,
                send_max: None,
                skip_mainnet_prompt: false,
            },
            &tx,
            Some(&credential),
        )
        .await;
        // The node claims hash "ABC"; the reported hash is the one computed from our signed blob,
        // followed by the node-chosen fee that was signed (TC-214).
        let local_hash = |a: Action| match a {
            Action::PaymentSubmitOk(h) | Action::OfferCancelSubmitOk(h) => h,
            other => panic!("expected submit ok, got {other:?}"),
        };
        let shown = local_hash(first(&mut rx));
        let (h, fee) = shown.split_once(" · ").expect("hash · fee");
        assert!(
            h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()),
            "{shown}"
        );
        assert_eq!(fee, "fee 12 drops");
        let h = h.to_string();

        submit_offer_cancel_transaction(
            &rpc,
            &Network::Testnet,
            OfferCancelSubmitParams {
                offer_sequence: 42,
                skip_mainnet_prompt: false,
            },
            &tx,
            Some(&credential),
        )
        .await;
        assert_ne!(local_hash(first(&mut rx)), h);

        let source = credential.address().expect("address");
        let spec = quote_payment(
            &rpc,
            &source,
            PaymentQuoteParams {
                destination: GENESIS.into(),
                amount: "10".into(),
                iou_currency: Some("USD".into()),
                iou_issuer: Some(GENESIS.into()),
                send_max: String::new(),
                slippage_bps: 100,
            },
        )
        .await;
        assert_eq!(spec.as_deref(), Ok("XRP:1010000"));

        // TC-203: a testnet node (network_id 1) under a devnet session is refused before signing.
        submit_offer_cancel_transaction(
            &rpc,
            &Network::Devnet,
            OfferCancelSubmitParams {
                offer_sequence: 42,
                skip_mainnet_prompt: false,
            },
            &tx,
            Some(&credential),
        )
        .await;
        assert!(
            matches!(first(&mut rx), Action::OfferCancelSubmitErr(e) if e.contains("network_id")),
        );

        let requests = server.await.expect("join").expect("serve");
        assert_eq!(
            rpc_mock::methods(&requests),
            [
                "server_info",
                "account_info",
                "simulate",
                "submit",
                "server_info",
                "account_info",
                "simulate",
                "submit",
                "ripple_path_find",
                "server_info"
            ]
        );
        let rpf = &requests[8]["params"][0];
        assert_eq!(rpf["source_currencies"], json!([{"currency": "XRP"}]));
        assert_eq!(rpf["destination_amount"]["value"], "10");
        // The reported hash is the hash of the blob that was actually submitted.
        let blob = requests[3]["params"][0]["tx_blob"].as_str().expect("blob");
        assert_eq!(crate::signing::tx_hash_from_blob(blob).expect("hash"), h);
    }

    /// TC-221: escrow paths vs local JSON-RPC — EscrowCreate simulates the absolute
    /// FinishAfter/CancelAfter it later signs; EscrowFinish carries Owner/OfferSequence;
    /// `account_escrows` resolves a missing Sequence from the creating tx (TicketSequence).
    #[tokio::test]
    async fn escrow_paths_against_local_rpc() {
        use crate::test_support::rpc_mock;
        use secrecy::SecretString;
        use serde_json::{Value, json};
        use tokio::net::TcpListener;

        const GENESIS: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        fn result_for(method: &str) -> Value {
            match method {
                "server_info" => json!({"info": {"network_id": 1, "validated_ledger": {"seq": 1}}}),
                "account_info" => json!({"account_data": {
                    "Account": GENESIS, "Balance": "100000000", "Sequence": 7,
                    "OwnerCount": 0, "Flags": 0
                }}),
                "simulate" => json!({
                    "engine_result": "tesSUCCESS",
                    "ledger_index": 80_000_000,
                    "tx_json": {"Sequence": 7, "Fee": "12"}
                }),
                "submit" => json!({"engine_result": "tesSUCCESS", "tx_json": {"hash": "ABC"}}),
                "account_objects" => json!({"account_objects": [{
                    "LedgerEntryType": "Escrow", "Account": GENESIS, "Destination": GENESIS,
                    "Amount": "1000000", "FinishAfter": 800, "PreviousTxnID": "EE"
                }]}),
                "tx" => json!({"tx_json": {"Sequence": 0, "TicketSequence": 30}}),
                other => panic!("unexpected local RPC method: {other}"),
            }
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(rpc_mock::serve(listener, 10, result_for));
        let rpc = RpcClient::connect(&url).expect("rpc client");
        let credential = crate::signing::credential_from_secrets(
            Some(&SecretString::from("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB")),
            None,
        )
        .expect("credential")
        .expect("seed set");
        let (tx, mut rx) = mpsc::unbounded_channel();

        let before = ripple_now_secs();
        submit_escrow_create_transaction(
            &rpc,
            &Network::Testnet,
            EscrowCreateSubmitParams {
                destination: GENESIS.into(),
                amount_xrp: "1".into(),
                finish_after: "1h".into(),
                cancel_after: "2h".into(),
                skip_mainnet_prompt: false,
            },
            &tx,
            Some(&credential),
        )
        .await;
        assert!(matches!(rx.try_recv(), Ok(Action::EscrowCreateSubmitOk(_))));
        while rx.try_recv().is_ok() {}

        submit_escrow_close_transaction(
            &rpc,
            &Network::Testnet,
            EscrowCloseSubmitParams {
                owner: GENESIS.into(),
                offer_sequence: 30,
                cancel: false,
                skip_mainnet_prompt: false,
            },
            &tx,
            Some(&credential),
        )
        .await;
        assert!(matches!(rx.try_recv(), Ok(Action::EscrowCloseSubmitOk(_))));

        let escrows = rpc.account_escrows(GENESIS).await.expect("escrows");
        assert_eq!(escrows[0].offer_sequence, Some(30));

        let requests = server.await.expect("join").expect("serve");
        let create = &requests[2]["params"][0]["tx_json"];
        assert_eq!(create["TransactionType"], "EscrowCreate");
        let finish = create["FinishAfter"].as_u64().expect("FinishAfter");
        assert!(
            (before + 3_600..=before + 3_660).contains(&finish),
            "{finish}"
        );
        assert_eq!(create["CancelAfter"].as_u64(), Some(finish + 3_600));
        let close = &requests[6]["params"][0]["tx_json"];
        assert_eq!(
            (
                &close["TransactionType"],
                &close["Owner"],
                &close["OfferSequence"]
            ),
            (&json!("EscrowFinish"), &json!(GENESIS), &json!(30))
        );
        assert_eq!(
            &rpc_mock::methods(&requests)[8..],
            ["account_objects", "tx"]
        );
    }

    /// TC-226: AMM paths vs local JSON-RPC — deposit simulates `tfTwoAsset` with both amounts;
    /// withdraw by LP amount reads the LP token from `amm_info` into `LPTokenIn` before simulate.
    #[tokio::test]
    async fn amm_paths_against_local_rpc() {
        use crate::test_support::rpc_mock;
        use secrecy::SecretString;
        use serde_json::{Value, json};
        use tokio::net::TcpListener;

        const GENESIS: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        const ISS: &str = "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN";
        const LP: &str = "03930D02208264E2E40EC1B0C09E4DB96EE197B1";
        fn result_for(method: &str) -> Value {
            match method {
                "server_info" => json!({"info": {"network_id": 1, "validated_ledger": {"seq": 1}}}),
                "account_info" => json!({"account_data": {
                    "Account": GENESIS, "Balance": "100000000", "Sequence": 7,
                    "OwnerCount": 0, "Flags": 0
                }}),
                "amm_info" => json!({"amm": {"lp_token": {
                    "currency": LP, "issuer": ISS, "value": "100"
                }}}),
                "simulate" => json!({
                    "engine_result": "tesSUCCESS",
                    "ledger_index": 80_000_000,
                    "tx_json": {"Sequence": 7, "Fee": "12"}
                }),
                "submit" => json!({"engine_result": "tesSUCCESS", "tx_json": {"hash": "ABC"}}),
                other => panic!("unexpected local RPC method: {other}"),
            }
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(rpc_mock::serve(listener, 9, result_for));
        let rpc = RpcClient::connect(&url).expect("rpc client");
        let credential = crate::signing::credential_from_secrets(
            Some(&SecretString::from("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB")),
            None,
        )
        .expect("credential")
        .expect("seed set");
        let (tx, mut rx) = mpsc::unbounded_channel();

        submit_amm_deposit_transaction(
            &rpc,
            &Network::Testnet,
            AmmDepositSubmitParams {
                amount: "XRP:1000000".into(),
                amount2: format!("USD:{ISS}:10"),
                skip_mainnet_prompt: false,
            },
            &tx,
            Some(&credential),
        )
        .await;
        let got = rx.try_recv();
        assert!(
            matches!(&got, Ok(Action::AmmSubmitOk(m)) if m.starts_with("AMMDeposit · ")),
            "{got:?}"
        );
        while rx.try_recv().is_ok() {}

        submit_amm_withdraw_transaction(
            &rpc,
            &Network::Testnet,
            AmmWithdrawSubmitParams {
                asset: "XRP".into(),
                asset2: format!("USD:{ISS}"),
                lp_amount: "5".into(),
                skip_mainnet_prompt: false,
            },
            &tx,
            Some(&credential),
        )
        .await;
        let got = rx.try_recv();
        assert!(
            matches!(&got, Ok(Action::AmmSubmitOk(m)) if m.starts_with("AMMWithdraw · ")),
            "{got:?}"
        );

        let requests = server.await.expect("join").expect("serve");
        assert_eq!(
            rpc_mock::methods(&requests),
            [
                "server_info",
                "account_info",
                "simulate",
                "submit",
                "server_info",
                "account_info",
                "amm_info",
                "simulate",
                "submit"
            ]
        );
        let deposit = &requests[2]["params"][0]["tx_json"];
        assert_eq!(deposit["TransactionType"], "AMMDeposit");
        assert_eq!(deposit["Flags"], 0x0010_0000);
        assert_eq!(deposit["Amount"], "1000000");
        assert_eq!(deposit["Amount2"]["value"], "10");
        let withdraw = &requests[7]["params"][0]["tx_json"];
        assert_eq!(withdraw["TransactionType"], "AMMWithdraw");
        assert_eq!(withdraw["Flags"], 0x0001_0000);
        assert_eq!(
            withdraw["LPTokenIn"],
            json!({"currency": LP, "issuer": ISS, "value": "5"})
        );
    }

    /// TC-232: AccountDelete — an X-address destination's tag reaches simulate and the signed
    /// blob, and the blob carries the node-chosen fee (owner reserve, not the base fee).
    #[tokio::test]
    async fn account_delete_against_local_rpc_keeps_xaddress_tag_and_node_fee() {
        use crate::test_support::rpc_mock;
        use secrecy::SecretString;
        use serde_json::{Value, json};
        use tokio::net::TcpListener;
        use xrpl::core::addresscodec::classic_address_to_xaddress;
        use xrpl::core::binarycodec::decode;

        const GENESIS: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        fn result_for(method: &str) -> Value {
            match method {
                "server_info" => json!({"info": {"network_id": 1, "validated_ledger": {"seq": 1}}}),
                "account_info" => json!({"account_data": {
                    "Account": GENESIS, "Balance": "100000000", "Sequence": 7,
                    "OwnerCount": 0, "Flags": 0
                }}),
                "simulate" => json!({
                    "engine_result": "tesSUCCESS",
                    "ledger_index": 80_000_000,
                    "tx_json": {"Sequence": 7, "Fee": "200000"}
                }),
                "submit" => json!({"engine_result": "tesSUCCESS", "tx_json": {"hash": "ABC"}}),
                other => panic!("unexpected local RPC method: {other}"),
            }
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(rpc_mock::serve(listener, 4, result_for));
        let rpc = RpcClient::connect(&url).expect("rpc client");
        let credential = crate::signing::credential_from_secrets(
            Some(&SecretString::from("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB")),
            None,
        )
        .expect("credential")
        .expect("seed set");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let xaddr = classic_address_to_xaddress(GENESIS, Some(12345), true).expect("xaddr");

        submit_account_delete_transaction(
            &rpc,
            &Network::Testnet,
            AccountDeleteSubmitParams {
                destination: format!(" {xaddr} "),
                skip_mainnet_prompt: false,
            },
            &tx,
            Some(&credential),
        )
        .await;
        let got = rx.try_recv();
        assert!(
            matches!(&got, Ok(Action::AccountDeleteSubmitOk(m)) if m.ends_with("fee 200000 drops")),
            "{got:?}"
        );

        let requests = server.await.expect("join").expect("serve");
        assert_eq!(
            rpc_mock::methods(&requests),
            ["server_info", "account_info", "simulate", "submit"]
        );
        let sim = &requests[2]["params"][0]["tx_json"];
        assert_eq!(
            (
                &sim["TransactionType"],
                &sim["Destination"],
                &sim["DestinationTag"]
            ),
            (&json!("AccountDelete"), &json!(GENESIS), &json!(12345))
        );
        let blob = requests[3]["params"][0]["tx_blob"].as_str().expect("blob");
        let signed = decode(blob).expect("decode");
        assert_eq!(
            (
                &signed["Destination"],
                &signed["DestinationTag"],
                &signed["Fee"]
            ),
            (&json!(GENESIS), &json!(12345), &json!("200000"))
        );
    }

    /// A `tec*` simulate result stops the TUI submit flow before signing/submit and reports it.
    #[tokio::test]
    async fn tec_simulate_stops_before_submit() {
        use crate::test_support::rpc_mock;
        use secrecy::SecretString;
        use serde_json::{Value, json};
        use tokio::net::TcpListener;

        const GENESIS: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        fn result_for(method: &str) -> Value {
            match method {
                "server_info" => json!({"info": {"network_id": 1, "validated_ledger": {"seq": 1}}}),
                "account_info" => json!({"account_data": {
                    "Account": GENESIS, "Balance": "10000000", "Sequence": 7,
                    "OwnerCount": 0, "Flags": 0
                }}),
                "simulate" => json!({
                    "engine_result": "tecUNFUNDED_PAYMENT",
                    "engine_result_message": "Insufficient XRP balance to send.",
                    "tx_json": {}
                }),
                other => panic!("unexpected local RPC method: {other}"),
            }
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(rpc_mock::serve(listener, 3, result_for));
        let rpc = RpcClient::connect(&url).expect("rpc client");
        let credential = crate::signing::credential_from_secrets(
            Some(&SecretString::from("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB")),
            None,
        )
        .expect("credential")
        .expect("seed set");
        let (tx, mut rx) = mpsc::unbounded_channel();
        submit_payment_transaction(
            &rpc,
            &Network::Testnet,
            PaymentSubmitParams {
                destination: GENESIS.into(),
                amount: "1".into(),
                iou_currency: None,
                iou_issuer: None,
                destination_tag: None,
                send_max: None,
                skip_mainnet_prompt: false,
            },
            &tx,
            Some(&credential),
        )
        .await;
        let got = rx.try_recv();
        assert!(
            matches!(&got, Ok(Action::PaymentSubmitErr(e)) if e.contains("tecUNFUNDED_PAYMENT")),
            "{got:?}"
        );
        let requests = server.await.expect("join").expect("serve");
        assert_eq!(
            rpc_mock::methods(&requests),
            ["server_info", "account_info", "simulate"]
        );
    }

    const SA_GENESIS: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
    const SA_RECIPIENT: &str = "0xf5488132432118596fa13800b68df4c0ff25131d";

    fn sa_params(redeem: bool, value: u128, fee_drops: u64) -> SmartAccountInstructionParams {
        SmartAccountInstructionParams {
            redeem,
            value,
            recipient: if redeem {
                String::new()
            } else {
                SA_RECIPIENT.into()
            },
            operator_xrpl: SA_GENESIS.into(),
            fee_drops,
            skip_mainnet_prompt: false,
        }
    }

    fn sa_credential() -> crate::signing::SigningCredential {
        crate::signing::credential_from_secrets(
            Some(&secrecy::SecretString::from(
                "sEdSkooMk31MeTjbHVE7vLvgCpEMAdB",
            )),
            None,
        )
        .expect("credential")
        .expect("seed set")
    }

    /// TC-242: Smart Account transfer and redeem (local JSON-RPC): simulate precedes submit, and
    /// the signed blob is a Payment to the pinned operator for exactly the instruction fee, with
    /// no DestinationTag and one Memo whose MemoData is the 32-byte instruction.
    #[tokio::test]
    async fn smart_account_instruction_pays_pinned_operator_with_instruction_memo() {
        use crate::test_support::rpc_mock;
        use serde_json::{Value, json};
        use tokio::net::TcpListener;
        use xrpl::core::binarycodec::decode;

        fn result_for(method: &str) -> Value {
            match method {
                "server_info" => json!({"info": {"network_id": 1, "validated_ledger": {"seq": 1}}}),
                "account_info" => json!({"account_data": {
                    "Account": "rSigner", "Balance": "10000000", "Sequence": 7,
                    "OwnerCount": 0, "Flags": 0
                }}),
                "simulate" => json!({
                    "engine_result": "tesSUCCESS",
                    "ledger_index": 80_000_000,
                    "tx_json": {"Sequence": 7, "Fee": "12"}
                }),
                "submit" => json!({"engine_result": "tesSUCCESS", "tx_json": {"hash": "ABC"}}),
                other => panic!("unexpected local RPC method: {other}"),
            }
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(rpc_mock::serve(listener, 8, result_for));
        let rpc = RpcClient::connect(&url).expect("rpc client");
        let credential = sa_credential();
        let (tx, mut rx) = mpsc::unbounded_channel();

        for params in [
            sa_params(false, 2_500_000, 200_000),
            sa_params(true, 3, 1_500_000),
        ] {
            submit_smart_account_instruction(
                &rpc,
                &Network::Testnet,
                params,
                Some(SA_GENESIS),
                &tx,
                Some(&credential),
            )
            .await;
            let got = rx.try_recv();
            assert!(
                matches!(&got, Ok(Action::SmartAccountInstructionSubmitOk(m)) if m.ends_with("fee 12 drops")),
                "{got:?}"
            );
            // Follow-ups after a successful write: account + history refresh.
            assert!(matches!(rx.try_recv(), Ok(Action::RefreshAccount)));
            assert!(matches!(rx.try_recv(), Ok(Action::RefreshTxHistory)));
        }

        let requests = server.await.expect("join").expect("serve");
        let methods = rpc_mock::methods(&requests);
        assert_eq!(
            methods,
            [
                "server_info",
                "account_info",
                "simulate",
                "submit",
                "server_info",
                "account_info",
                "simulate",
                "submit"
            ]
        );
        // (simulate request, submit request, fee, instruction id, value)
        for (sim, submit, fee, id, value) in [
            (
                &requests[2],
                &requests[3],
                "200000",
                0x01_u8,
                2_500_000_u128,
            ),
            (&requests[6], &requests[7], "1500000", 0x02, 3),
        ] {
            let sim = &sim["params"][0]["tx_json"];
            assert_eq!(sim["Destination"], json!(SA_GENESIS));
            assert_eq!(sim["Amount"], json!(fee));
            assert!(sim.get("DestinationTag").is_none(), "{sim}");
            let blob = submit["params"][0]["tx_blob"].as_str().expect("blob");
            let signed = decode(blob).expect("decode");
            assert_eq!(signed["TransactionType"], json!("Payment"));
            assert_eq!(signed["Destination"], json!(SA_GENESIS));
            assert_eq!(signed["Amount"], json!(fee));
            assert!(signed.get("DestinationTag").is_none(), "{signed}");
            let memos = signed["Memos"].as_array().expect("memos");
            assert_eq!(memos.len(), 1);
            let memo = memos[0]["Memo"]["MemoData"].as_str().expect("MemoData");
            let bytes: [u8; 32] = hex::decode(memo)
                .expect("hex")
                .try_into()
                .expect("32-byte instruction");
            let decoded = crate::signing::decode_smart_account_instruction(&bytes);
            assert_eq!(
                (decoded.id, decoded.wallet_id, decoded.value),
                (id, 0, value)
            );
            if id == 0x01 {
                assert_eq!(hex::encode(decoded.tail), SA_RECIPIENT[2..]);
            } else {
                assert_eq!(decoded.tail, [0; 20]);
            }
        }
    }

    /// TC-243: every Smart Account refusal happens before any RPC (the RPC here is unreachable,
    /// so an attempted call would surface as a connection error, not the expected message):
    /// missing / wrong operator pin, zero or absurd fee, zero-address recipient, zero or
    /// over-wide value, X-address operator, operator == signer.
    #[tokio::test]
    async fn smart_account_instruction_refusals_never_reach_rpc() {
        let rpc = RpcClient::connect("http://127.0.0.1:1").expect("rpc client");
        let credential = sa_credential();
        let signer = credential.address().expect("address");
        let xaddr =
            xrpl::core::addresscodec::classic_address_to_xaddress(SA_GENESIS, Some(1), true)
                .expect("xaddr");

        type Tweak = fn(&mut SmartAccountInstructionParams);
        let cases: Vec<(&str, Option<&str>, Tweak, &str)> = vec![
            ("pin unset", None, |_| {}, "expected_operator"),
            ("pin empty", Some(" "), |_| {}, "expected_operator"),
            (
                "pin mismatch",
                Some("rSomeoneElse"),
                |_| {},
                "differs from expected_operator",
            ),
            (
                "zero fee",
                Some(SA_GENESIS),
                |p| p.fee_drops = 0,
                "instruction fee",
            ),
            (
                "absurd fee",
                Some(SA_GENESIS),
                |p| p.fee_drops = crate::signing::MAX_SMART_ACCOUNT_FEE_DROPS + 1,
                "instruction fee",
            ),
            (
                "zero recipient",
                Some(SA_GENESIS),
                |p| p.recipient = "0x0000000000000000000000000000000000000000".into(),
                "zero address",
            ),
            (
                "bad recipient",
                Some(SA_GENESIS),
                |p| p.recipient = "0xnope".into(),
                "Flare recipient",
            ),
            (
                "zero value",
                Some(SA_GENESIS),
                |p| p.value = 0,
                "greater than zero",
            ),
            (
                "value over 10 bytes",
                Some(SA_GENESIS),
                |p| p.value = crate::signing::SMART_ACCOUNT_VALUE_MAX + 1,
                "10-byte",
            ),
            (
                "operator not an address",
                Some(SA_GENESIS),
                |p| p.operator_xrpl = "not-an-address".into(),
                "",
            ),
        ];
        for (name, pin, tweak, expect) in cases {
            let mut params = sa_params(false, 1_000_000, 200_000);
            tweak(&mut params);
            let (tx, mut rx) = mpsc::unbounded_channel();
            submit_smart_account_instruction(
                &rpc,
                &Network::Testnet,
                params,
                pin,
                &tx,
                Some(&credential),
            )
            .await;
            match rx.try_recv() {
                Ok(Action::SmartAccountInstructionSubmitErr(m)) => {
                    assert!(m.contains(expect), "{name}: {m}");
                    assert!(
                        !m.contains("server_info") && !m.contains("account_info"),
                        "{name}: RPC was attempted: {m}"
                    );
                }
                other => panic!("{name}: expected submit error, got {other:?}"),
            }
            assert!(rx.try_recv().is_err(), "{name}: exactly one action");
        }

        // X-address operator (carries a tag) and operator == signing account.
        for (name, operator, expect) in [
            ("x-address operator", xaddr, "no X-address tag"),
            ("operator is signer", signer, "matches source account"),
        ] {
            let mut params = sa_params(false, 1_000_000, 200_000);
            params.operator_xrpl = operator.clone();
            let (tx, mut rx) = mpsc::unbounded_channel();
            submit_smart_account_instruction(
                &rpc,
                &Network::Testnet,
                params,
                Some(&operator),
                &tx,
                Some(&credential),
            )
            .await;
            match rx.try_recv() {
                Ok(Action::SmartAccountInstructionSubmitErr(m)) => {
                    assert!(m.contains(expect), "{name}: {m}");
                }
                other => panic!("{name}: expected submit error, got {other:?}"),
            }
        }
    }

    /// TC-088 (R-006): every sign+submit path rejects production writes without `--yes`
    /// before any RPC/signing.
    #[tokio::test]
    async fn mainnet_submit_without_yes_is_rejected() {
        let rpc = RpcClient::connect("http://127.0.0.1:1").expect("rpc client");
        const GENESIS: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        type SubmitCase = Box<
            dyn for<'r> FnOnce(
                &'r RpcClient,
                mpsc::UnboundedSender<Action>,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = ()> + Send + 'r>,
            >,
        >;
        let cases: Vec<(&str, SubmitCase)> = vec![
            (
                "account_set",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_account_set_transaction(
                            rpc,
                            &Network::Mainnet,
                            AccountSetSubmitParams {
                                set_flag: None,
                                clear_flag: None,
                                domain_ascii: "example.com".into(),
                                tick_size: String::new(),
                                transfer_rate: String::new(),
                                skip_mainnet_prompt: false,
                            },
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
            (
                "payment",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_payment_transaction(
                            rpc,
                            &Network::Mainnet,
                            PaymentSubmitParams {
                                destination: GENESIS.into(),
                                amount: "0.001".into(),
                                iou_currency: None,
                                iou_issuer: None,
                                destination_tag: None,
                                send_max: None,
                                skip_mainnet_prompt: false,
                            },
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
            (
                "trust_set",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_trust_set_transaction(
                            rpc,
                            &Network::Mainnet,
                            TrustSetSubmitParams {
                                currency: "USD".into(),
                                issuer: GENESIS.into(),
                                limit: "1000".into(),
                                skip_mainnet_prompt: false,
                            },
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
            (
                "offer_create",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_offer_create_transaction(
                            rpc,
                            &Network::Mainnet,
                            OfferCreateSubmitParams {
                                taker_gets: "XRP:1000000".into(),
                                taker_pays: format!("USD:{GENESIS}:10"),
                                skip_mainnet_prompt: false,
                            },
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
            (
                "escrow_create",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_escrow_create_transaction(
                            rpc,
                            &Network::Mainnet,
                            EscrowCreateSubmitParams {
                                destination: GENESIS.into(),
                                amount_xrp: "1".into(),
                                finish_after: "1h".into(),
                                cancel_after: String::new(),
                                skip_mainnet_prompt: false,
                            },
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
            (
                "escrow_close",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_escrow_close_transaction(
                            rpc,
                            &Network::Mainnet,
                            EscrowCloseSubmitParams {
                                owner: GENESIS.into(),
                                offer_sequence: 1,
                                cancel: true,
                                skip_mainnet_prompt: false,
                            },
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
            (
                "offer_cancel",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_offer_cancel_transaction(
                            rpc,
                            &Network::Mainnet,
                            OfferCancelSubmitParams {
                                offer_sequence: 1,
                                skip_mainnet_prompt: false,
                            },
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
            (
                "set_regular_key",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_set_regular_key_transaction(
                            rpc,
                            &Network::Mainnet,
                            SetRegularKeySubmitParams {
                                regular_key: GENESIS.into(),
                                skip_mainnet_prompt: false,
                            },
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
            (
                "fxrp_direct_mint_payment",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_fxrp_direct_mint_payment(
                            rpc,
                            &Network::Mainnet,
                            FxrpDirectMintPaymentParams {
                                core_vault_xrpl: GENESIS.into(),
                                flare_recipient: "0xabcdef0123456789abcdef0123456789abcdef01"
                                    .into(),
                                amount_xrp: "1".into(),
                                skip_mainnet_prompt: false,
                            },
                            None,
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
            (
                "smart_account_instruction",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_smart_account_instruction(
                            rpc,
                            &Network::Mainnet,
                            sa_params(false, 1_000_000, 200_000),
                            None,
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
            (
                "amm_deposit",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_amm_deposit_transaction(
                            rpc,
                            &Network::Mainnet,
                            AmmDepositSubmitParams {
                                amount: "XRP:1000000".into(),
                                amount2: format!("USD:{GENESIS}:1"),
                                skip_mainnet_prompt: false,
                            },
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
            (
                "amm_withdraw",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_amm_withdraw_transaction(
                            rpc,
                            &Network::Mainnet,
                            AmmWithdrawSubmitParams {
                                asset: "XRP".into(),
                                asset2: format!("USD:{GENESIS}"),
                                lp_amount: String::new(),
                                skip_mainnet_prompt: false,
                            },
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
            (
                "account_delete",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_account_delete_transaction(
                            rpc,
                            &Network::Mainnet,
                            AccountDeleteSubmitParams {
                                destination: GENESIS.into(),
                                skip_mainnet_prompt: false,
                            },
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
            (
                "generic_nft_mint",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_generic_transaction(
                            rpc,
                            &Network::Mainnet,
                            generic_params("NFTokenMint", &[("NFTokenTaxon", "0")], false),
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
            (
                "generic_nft_burn",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_generic_transaction(
                            rpc,
                            &Network::Mainnet,
                            generic_params("NFTokenBurn", &[("NFTokenID", NFT_ID)], false),
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
        ];

        for (name, run) in cases {
            let (tx, mut rx) = mpsc::unbounded_channel();
            run(&rpc, tx).await;
            let action = rx
                .try_recv()
                .unwrap_or_else(|_| panic!("{name}: expected an action"));
            let msg = match &action {
                Action::AccountSetSubmitErr(m)
                | Action::PaymentSubmitErr(m)
                | Action::TrustSetSubmitErr(m)
                | Action::OfferCreateSubmitErr(m)
                | Action::OfferCancelSubmitErr(m)
                | Action::SetRegularKeySubmitErr(m)
                | Action::FxrpDirectMintPaymentSubmitErr(m)
                | Action::SmartAccountInstructionSubmitErr(m)
                | Action::EscrowCreateSubmitErr(m)
                | Action::EscrowCloseSubmitErr(m)
                | Action::AmmSubmitErr(m)
                | Action::AccountDeleteSubmitErr(m)
                | Action::GenericTxSubmitErr(m) => m,
                other => panic!("{name}: expected submit error, got {other:?}"),
            };
            assert!(msg.contains("production network"), "{name}: {msg}");
            assert!(msg.contains("--yes"), "{name}: {msg}");
        }
    }

    /// TC-088: the Flare executeDirectMinting path has its own gate (different signature, so it
    /// sits outside the table above): enabled execute + mainnet without `--yes` is refused.
    #[tokio::test]
    async fn fxrp_execute_direct_mint_mainnet_without_yes_is_rejected() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let params = FxrpExecuteDirectMintParams {
            proof_json: "{}".into(),
            skip_mainnet_prompt: false,
        };
        submit_fxrp_execute_direct_mint(
            Some("https://flare-api.flare.network/ext/C/rpc"),
            true,
            "FLARE_EVM_KEY",
            None,
            &Network::Mainnet,
            params,
            &tx,
        )
        .await;
        match rx.try_recv() {
            Ok(Action::FxrpExecuteDirectMintSubmitErr(m)) => {
                assert!(
                    m.contains("production network") && m.contains("--yes"),
                    "{m}"
                );
            }
            other => panic!("expected submit error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fxrp_execute_direct_mint_refuses_when_execute_disabled() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let params = FxrpExecuteDirectMintParams {
            proof_json: r#"{"proof":["0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],"response":"0xbb"}"#.into(),
            skip_mainnet_prompt: true,
        };
        submit_fxrp_execute_direct_mint(
            Some("https://flare-api.flare.network/ext/C/rpc"),
            false,
            "FLARE_EVM_KEY",
            None,
            &Network::Testnet,
            params,
            &tx,
        )
        .await;
        match rx.try_recv() {
            Ok(Action::FxrpExecuteDirectMintSubmitErr(msg)) => {
                assert!(msg.contains("disabled"), "unexpected message: {msg}");
                assert!(msg.contains("execute = true"));
            }
            other => panic!("expected FxrpExecuteDirectMintSubmitErr, got {other:?}"),
        }
    }

    /// Mainnet submit paths with `--yes` bypass the production-write guard and
    /// then fail on the missing seed, not on the guard.
    #[tokio::test]
    async fn mainnet_submit_with_yes_skips_guard_across_tx_types() {
        let rpc = RpcClient::connect("http://127.0.0.1:1").expect("rpc client");
        const GENESIS: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        type SubmitCase = Box<
            dyn for<'r> FnOnce(
                &'r RpcClient,
                mpsc::UnboundedSender<Action>,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = ()> + Send + 'r>,
            >,
        >;
        let cases: Vec<(&str, SubmitCase)> = vec![
            (
                "payment",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_payment_transaction(
                            rpc,
                            &Network::Mainnet,
                            PaymentSubmitParams {
                                destination: GENESIS.into(),
                                amount: "0.001".into(),
                                iou_currency: None,
                                iou_issuer: None,
                                destination_tag: None,
                                send_max: None,
                                skip_mainnet_prompt: true,
                            },
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
            (
                "set_regular_key",
                Box::new(|rpc, tx| {
                    Box::pin(async move {
                        submit_set_regular_key_transaction(
                            rpc,
                            &Network::Mainnet,
                            SetRegularKeySubmitParams {
                                regular_key: String::new(), // clear
                                skip_mainnet_prompt: true,
                            },
                            &tx,
                            None,
                        )
                        .await;
                    })
                }),
            ),
        ];
        for (name, run) in cases {
            let (tx, mut rx) = mpsc::unbounded_channel();
            run(&rpc, tx).await;
            let action = rx.recv().await.expect("action");
            match action {
                Action::PaymentSubmitErr(msg) | Action::SetRegularKeySubmitErr(msg) => {
                    assert!(
                        !msg.contains("restart lazyxrp with --yes"),
                        "{name}: mainnet guard should be skipped when --yes is set: {msg}"
                    );
                    assert!(
                        msg.contains("no signing credential"),
                        "{name}: expected seed error after guard skip, got: {msg}"
                    );
                }
                other => panic!("{name}: expected submit error after guard skip, got {other:?}"),
            }
        }
    }

    fn generic_params(tx_type: &str, fields: &[(&str, &str)], skip: bool) -> GenericTxSubmitParams {
        GenericTxSubmitParams {
            tx_type: tx_type.into(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            skip_mainnet_prompt: skip,
        }
    }

    fn test_credential() -> crate::signing::SigningCredential {
        use secrecy::SecretString;
        crate::signing::credential_from_secrets(
            Some(&SecretString::from("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB")),
            None,
        )
        .expect("credential")
        .expect("seed set")
    }

    const NFT_ID: &str = "000B013A95F14B0044F78A264E41713C64B5F89242540EE208C3098E00000D65";

    /// TC-252: generic spec'd submit vs local JSON-RPC — `server_info → account_info → simulate →
    /// submit` per transaction; the simulate request and the signed blob carry the spec's fields;
    /// the blob carries the simulate-chosen Sequence / Fee / LastLedgerSequence (distinct values);
    /// a relative `Expiration` is fixed once (simulate and blob agree); the reported hash is
    /// computed from the signed blob; every refusal (unknown type / field, invalid value) happens
    /// before any RPC.
    #[tokio::test]
    async fn generic_tx_against_local_rpc() {
        use crate::test_support::rpc_mock;
        use serde_json::{Value, json};
        use tokio::net::TcpListener;
        use xrpl::core::binarycodec::decode;

        fn result_for(method: &str) -> Value {
            match method {
                "server_info" => json!({"info": {"network_id": 1, "validated_ledger": {"seq": 1}}}),
                "account_info" => json!({"account_data": {
                    "Account": "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh", "Balance": "10000000",
                    "Sequence": 7, "OwnerCount": 0, "Flags": 0
                }}),
                "simulate" => json!({
                    "engine_result": "tesSUCCESS",
                    "ledger_index": 80_000_000,
                    "tx_json": {"Sequence": 7, "Fee": "15"}
                }),
                "submit" => json!({"engine_result": "tesSUCCESS", "tx_json": {"hash": "ABC"}}),
                other => panic!("unexpected local RPC method: {other}"),
            }
        }
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(rpc_mock::serve(listener, 8, result_for));
        let rpc = RpcClient::connect(&url).expect("rpc client");
        let credential = test_credential();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let drain = |rx: &mut mpsc::UnboundedReceiver<Action>| {
            let mut out = Vec::new();
            while let Ok(a) = rx.try_recv() {
                out.push(a);
            }
            out
        };

        // Refusals first: nothing may reach the RPC (the server expects exactly 8 requests).
        for (tx_type, fields) in [
            ("NoSuchType", vec![]),
            ("NFTokenMint", vec![("NFTokenTaxon", "1"), ("Bogus", "x")]),
            (
                "NFTokenMint",
                vec![("NFTokenTaxon", "1"), ("TransferFee", "50001")],
            ),
            ("NFTokenBurn", vec![("NFTokenID", "abc")]),
        ] {
            submit_generic_transaction(
                &rpc,
                &Network::Testnet,
                generic_params(tx_type, &fields, false),
                &tx,
                Some(&credential),
            )
            .await;
            let got = drain(&mut rx);
            assert!(
                matches!(got.as_slice(), [Action::GenericTxSubmitErr(_)]),
                "{tx_type} {fields:?}: {got:?}"
            );
        }

        submit_generic_transaction(
            &rpc,
            &Network::Testnet,
            generic_params(
                "NFTokenMint",
                &[
                    ("NFTokenTaxon", "3"),
                    ("URI", "hi"),
                    ("Flags", "transferable"),
                    ("TransferFee", "10"),
                ],
                false,
            ),
            &tx,
            Some(&credential),
        )
        .await;
        let got = drain(&mut rx);
        let Some(Action::GenericTxSubmitOk(shown)) = got.first() else {
            panic!("expected GenericTxSubmitOk, got {got:?}");
        };
        let (label, rest) = shown.split_once(" · ").expect("type · hash");
        assert_eq!(label, "NFTokenMint");
        let (hash, fee) = rest.split_once(" · ").expect("hash · fee");
        assert_eq!(fee, "fee 15 drops");
        assert!(
            got[1..].contains(&Action::RefreshNfts),
            "NFT writes refresh the NFT list: {got:?}"
        );

        submit_generic_transaction(
            &rpc,
            &Network::Testnet,
            generic_params(
                "NFTokenCreateOffer",
                &[
                    ("NFTokenID", NFT_ID),
                    ("Amount", "XRP:1000000"),
                    ("Flags", "sell"),
                    ("Expiration", "1h"),
                ],
                false,
            ),
            &tx,
            Some(&credential),
        )
        .await;
        assert!(matches!(
            drain(&mut rx).first(),
            Some(Action::GenericTxSubmitOk(_))
        ));

        let requests = server.await.expect("join").expect("serve");
        assert_eq!(
            rpc_mock::methods(&requests),
            [
                "server_info",
                "account_info",
                "simulate",
                "submit",
                "server_info",
                "account_info",
                "simulate",
                "submit"
            ]
        );
        let sim = &requests[2]["params"][0]["tx_json"];
        assert_eq!(sim["TransactionType"], "NFTokenMint");
        assert_eq!((&sim["Sequence"], &sim["URI"]), (&json!(7), &json!("6869")));
        assert_eq!(
            (&sim["NFTokenTaxon"], &sim["TransferFee"], &sim["Flags"]),
            (&json!(3), &json!(10), &json!(8))
        );
        let blob = requests[3]["params"][0]["tx_blob"].as_str().expect("blob");
        let signed = decode(blob).expect("decode");
        assert_eq!(
            (
                &signed["Sequence"],
                &signed["Fee"],
                &signed["LastLedgerSequence"]
            ),
            (&json!(7), &json!("15"), &json!(80_000_020))
        );
        assert_eq!(signed["URI"], "6869");
        assert_eq!(crate::signing::tx_hash_from_blob(blob).expect("hash"), hash);

        let sim_exp = requests[6]["params"][0]["tx_json"]["Expiration"]
            .as_u64()
            .expect("Expiration");
        let blob = requests[7]["params"][0]["tx_blob"].as_str().expect("blob");
        assert_eq!(decode(blob).expect("decode")["Expiration"], json!(sim_exp));
        let now = ripple_now_secs();
        assert!(
            (now + 3_590..=now + 3_700).contains(&sim_exp),
            "Expiration {sim_exp} is not ~now + 1h ({now})"
        );
    }

    /// TC-253: a failing simulate stops the generic flow before sign / submit — a `tec*` result
    /// and a `Sequence` that differs from the request both end after `simulate`.
    #[tokio::test]
    async fn generic_tx_stops_when_simulate_refuses() {
        use crate::test_support::rpc_mock;
        use serde_json::{Value, json};
        use tokio::net::TcpListener;

        fn node(simulate: Value) -> impl Fn(&str) -> Value {
            move |method| match method {
                "server_info" => json!({"info": {"network_id": 1, "validated_ledger": {"seq": 1}}}),
                "account_info" => json!({"account_data": {
                    "Account": "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh", "Balance": "10000000",
                    "Sequence": 7, "OwnerCount": 0, "Flags": 0
                }}),
                "simulate" => simulate.clone(),
                other => panic!("{other} must not be called after a refused simulate"),
            }
        }
        let tec = json!({
            "engine_result": "tecNO_ENTRY",
            "engine_result_message": "no such NFT",
            "tx_json": {}
        });
        let wrong_sequence = json!({
            "engine_result": "tesSUCCESS",
            "ledger_index": 80_000_000,
            "tx_json": {"Sequence": 8, "Fee": "15"}
        });
        for (simulate, want) in [(tec, "tecNO_ENTRY"), (wrong_sequence, "Sequence")] {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
            let url = format!("http://{}", listener.local_addr().expect("addr"));
            let respond = node(simulate);
            let server = tokio::spawn(rpc_mock::serve_raw(
                listener,
                3,
                move |req| json!({"result": respond(req["method"].as_str().unwrap_or_default())}),
            ));
            let rpc = RpcClient::connect(&url).expect("rpc client");
            let (tx, mut rx) = mpsc::unbounded_channel();
            submit_generic_transaction(
                &rpc,
                &Network::Testnet,
                generic_params("NFTokenBurn", &[("NFTokenID", NFT_ID)], false),
                &tx,
                Some(&test_credential()),
            )
            .await;
            match rx.try_recv() {
                Ok(Action::GenericTxSubmitErr(m)) => assert!(m.contains(want), "{m}"),
                other => panic!("expected GenericTxSubmitErr({want}), got {other:?}"),
            }
            assert!(rx.try_recv().is_err(), "exactly one action");
            let requests = server.await.expect("join").expect("serve");
            assert_eq!(
                rpc_mock::methods(&requests),
                ["server_info", "account_info", "simulate"]
            );
        }
    }
}
