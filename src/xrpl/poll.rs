use std::time::{Duration, Instant};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;

use tracing::warn;

use super::address::{ensure_xaddress_matches_network, resolve_payment_destination};
use crate::action::Action;
use crate::network::Network;
use crate::signing;

use super::client::{
    RPC_TIMEOUT, RpcClient, empty_account_tx_page_on_not_found, path_find_snapshot, xrp_to_drops,
};
use super::types::{
    AccountSetSubmitParams, BookPair, FxrpDirectMintPaymentParams, FxrpExecuteDirectMintParams,
    OfferCreateSubmitParams, OracleId, PaymentSubmitParams, PollCommand, PollContext,
    SetRegularKeySubmitParams, SimulateResult, TrustSetSubmitParams,
};
use super::util::next_backoff_secs;
use serde_json::Value;

pub fn start_poll_task(
    ctx: PollContext,
    refresh_rx: UnboundedReceiver<PollCommand>,
    poll_trigger_rx: UnboundedReceiver<()>,
    action_tx: UnboundedSender<Action>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(drive_poll_loop(
        ctx,
        refresh_rx,
        poll_trigger_rx,
        action_tx,
        cancel,
    ))
}

const MIN_POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Tab indices mirrored from `app::TAB_TITLES` (Overview, Account, Market, Assets).
pub(crate) const TAB_MARKET: usize = 2;
pub(crate) const TAB_ASSETS: usize = 3;

#[must_use]
pub(crate) fn should_poll_market_book(active_tab: usize) -> bool {
    active_tab == TAB_MARKET
}

#[must_use]
pub(crate) fn should_poll_asset_panels(active_tab: usize) -> bool {
    active_tab == TAB_ASSETS
}

pub(crate) fn drain_poll_trigger_burst(rx: &mut UnboundedReceiver<()>) {
    while rx.try_recv().is_ok() {}
}

pub(crate) fn should_skip_poll_trigger(last_poll: Option<Instant>) -> bool {
    last_poll.is_some_and(|last| last.elapsed() < MIN_POLL_INTERVAL)
}

/// True while a failed-poll backoff window is still open.
/// Kept outside `select!` arms so cancel / PollCommand stay responsive (unlike sleeping in-arm).
pub(crate) fn is_backoff_active(backoff_until: Option<Instant>) -> bool {
    backoff_until.is_some_and(|until| Instant::now() < until)
}

pub(crate) fn action_from_account_tx_result(
    result: Result<crate::xrpl::types::AccountTxPage, color_eyre::Report>,
    append: bool,
) -> Action {
    let history = |page: crate::xrpl::types::AccountTxPage| {
        if append {
            Action::XrplTxHistoryAppend(page.rows, page.marker)
        } else {
            Action::XrplTxHistory(page.rows, page.marker)
        }
    };
    match result {
        Ok(page) => history(page),
        Err(e) => empty_account_tx_page_on_not_found(&e)
            .map(history)
            .unwrap_or_else(|| Action::XrplError(format!("account_tx: {e}"))),
    }
}

pub(crate) async fn simulate_tx_requiring_tes_success(
    rpc: &RpcClient,
    tx_json: Value,
) -> Result<SimulateResult, String> {
    match tokio::time::timeout(RPC_TIMEOUT, rpc.simulate_tx(tx_json)).await {
        Ok(Ok(sim)) => {
            if sim.engine_result != "tesSUCCESS" {
                Err(format!(
                    "simulate {}: {}",
                    sim.engine_result, sim.engine_result_message
                ))
            } else {
                Ok(sim)
            }
        }
        Ok(Err(e)) => Err(format!("simulate: {e}")),
        Err(_) => Err("simulate: timeout".to_string()),
    }
}

/// `account_tx` page fetch with the shared timeout/error handling; used by the
/// `TxHistory` and `TxHistoryMore` poll commands.
async fn dispatch_account_tx(
    rpc: &RpcClient,
    watch_address: &str,
    marker: Option<serde_json::Value>,
    append: bool,
    action_tx: &UnboundedSender<Action>,
) {
    match tokio::time::timeout(RPC_TIMEOUT, rpc.account_tx(watch_address, 20, marker)).await {
        Ok(result) => {
            send_account_tx_action(action_tx, action_from_account_tx_result(result, append))
        }
        Err(_) => {
            send_account_tx_action(action_tx, Action::XrplError("account_tx: timeout".into()))
        }
    }
}

fn mainnet_write_guard_blocks(network: &Network, skip_mainnet_prompt: bool) -> bool {
    network.is_production() && !skip_mainnet_prompt
}

/// Mainnet guard, then load seed + wallet for AccountSet / Payment submit.
fn resolve_submit_wallet<E>(
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
    if mainnet_write_guard_blocks(network, skip_mainnet_prompt) {
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

fn send_action(action_tx: &UnboundedSender<Action>, action: Action) -> bool {
    match action_tx.send(action) {
        Ok(()) => true,
        Err(e) => {
            warn!(?e, "action channel closed");
            false
        }
    }
}

fn send_submit_failure<E>(action_tx: &UnboundedSender<Action>, submit_err: E, message: String)
where
    E: Fn(String) -> Action,
{
    send_action(action_tx, submit_err(message));
    send_action(action_tx, Action::RefreshAccount);
}

async fn fetch_account_summary_for_submit<E>(
    rpc: &RpcClient,
    account: &str,
    submit_err: E,
    action_tx: &UnboundedSender<Action>,
) -> Option<crate::xrpl::types::AccountSummary>
where
    E: Fn(String) -> Action,
{
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

async fn finalize_simulate_sign_submit<E, FO, FS>(
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
            for action in on_ok(tx.hash) {
                send_action(action_tx, action);
            }
        }
        Ok(Err(e)) => send_submit_failure(action_tx, submit_err, format!("submit: {e}")),
        Err(_) => send_submit_failure(action_tx, submit_err, "submit: timeout".into()),
    }
}

/// Unwrap a validation Result; on error, send `submit_err` and give the caller
/// a `None` to return on.
fn unwrap_or_submit_err<T, E>(
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
async fn simulate_then_finalize<E, FS, FO>(
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

struct PollBatchInputs<'a> {
    rpc: &'a RpcClient,
    watch_address: &'a str,
    book_pair: &'a BookPair,
    oracles: &'a [OracleId],
    oracle_pairs: &'a [crate::xrpl::OraclePricePair],
    flare_rpc_url: Option<&'a str>,
    flare_feeds: &'a [String],
    /// `[flare] display` — Off skips FTSO/FXRP read fetches.
    flare_display: crate::config::FlareDisplay,
    flare_wallet_address: Option<&'a str>,
    flare_fassets_execute: bool,
    flare_evm_key_env: &'a str,
    /// When a signing seed is configured, `poll_wallet_overview` fetches `account_tx` once.
    skip_account_tx: bool,
    active_tab: usize,
}

async fn maybe_account_tx(
    rpc: &RpcClient,
    watch_address: &str,
    skip: bool,
) -> Option<
    Result<
        Result<crate::xrpl::types::AccountTxPage, color_eyre::Report>,
        tokio::time::error::Elapsed,
    >,
> {
    if skip {
        return None;
    }
    Some(tokio::time::timeout(RPC_TIMEOUT, rpc.account_tx(watch_address, 20, None)).await)
}

async fn poll_batch(inputs: PollBatchInputs<'_>, action_tx: &UnboundedSender<Action>) -> bool {
    let PollBatchInputs {
        rpc,
        watch_address,
        book_pair,
        oracles,
        oracle_pairs,
        flare_rpc_url,
        flare_feeds,
        flare_display,
        flare_wallet_address,
        flare_fassets_execute,
        flare_evm_key_env,
        skip_account_tx,
        active_tab,
    } = inputs;
    let skip_market = !should_poll_market_book(active_tab);
    let skip_assets = !should_poll_asset_panels(active_tab);
    let dest_amount = (!skip_market).then(|| book_pair.path_find_destination_amount_preview());
    let (
        server_info_result,
        dunl_result,
        fee_result,
        account_info_result,
        book_offers_result,
        path_find_result,
        account_nfts_result,
        trust_lines_result,
        account_tx_result,
    ) = tokio::join!(
        tokio::time::timeout(RPC_TIMEOUT, rpc.server_info()),
        tokio::time::timeout(RPC_TIMEOUT, rpc.fetch_xrplf_dunl()),
        tokio::time::timeout(RPC_TIMEOUT, rpc.fee()),
        tokio::time::timeout(RPC_TIMEOUT, rpc.account_info(watch_address)),
        async {
            if skip_market {
                None
            } else {
                Some(
                    tokio::time::timeout(
                        RPC_TIMEOUT,
                        rpc.book_offers(
                            book_pair.gets_currency(),
                            book_pair.gets_issuer(),
                            book_pair.pays_currency(),
                            book_pair.pays_issuer(),
                            book_pair.limit,
                        ),
                    )
                    .await,
                )
            }
        },
        async {
            if skip_market {
                return None;
            }
            let dest_amount = dest_amount.as_ref()?;
            Some(
                tokio::time::timeout(
                    RPC_TIMEOUT,
                    rpc.ripple_path_find(watch_address, watch_address, dest_amount),
                )
                .await,
            )
        },
        async {
            if skip_assets {
                None
            } else {
                Some(tokio::time::timeout(RPC_TIMEOUT, rpc.account_nfts(watch_address)).await)
            }
        },
        async {
            if skip_assets {
                None
            } else {
                Some(tokio::time::timeout(RPC_TIMEOUT, rpc.account_lines(watch_address)).await)
            }
        },
        maybe_account_tx(rpc, watch_address, skip_account_tx),
    );
    let mut any_rpc_succeeded = false;
    macro_rules! send_rpc_outcome {
        ($result:expr, $ok_action:expr, $label:literal) => {
            match $result {
                Ok(Ok(v)) => {
                    any_rpc_succeeded = true;
                    if let Err(e) = action_tx.send($ok_action(v)) {
                        warn!(?e, "action channel closed ({})", $label);
                    }
                }
                Ok(Err(e)) => {
                    if let Err(e2) = action_tx.send(Action::XrplError(format!("{}: {e}", $label))) {
                        warn!(?e2, "action channel closed ({})", $label);
                    }
                }
                Err(_) => {
                    if let Err(e) =
                        action_tx.send(Action::XrplError(format!("{}: timeout", $label)))
                    {
                        warn!(?e, "action channel closed ({})", $label);
                    }
                }
            }
        };
    }
    send_rpc_outcome!(
        server_info_result,
        |v| Action::XrplServerInfo(Box::new(v)),
        "server_info"
    );
    send_rpc_outcome!(dunl_result, Action::XrplDunl, "dUNL");
    send_rpc_outcome!(fee_result, Action::XrplFee, "fee");
    send_rpc_outcome!(
        account_info_result,
        |v| Action::XrplAccount(Box::new(v)),
        "account_info"
    );
    if let Some(book_offers_result) = book_offers_result {
        send_rpc_outcome!(book_offers_result, Action::XrplBookOffers, "book_offers");
    }
    if let Some(path_find_result) = path_find_result {
        match path_find_result {
            Ok(Ok(v)) => {
                any_rpc_succeeded = true;
                let snap = path_find_snapshot(&v, &book_pair.quote);
                if let Err(e) = action_tx.send(Action::XrplPathFind(snap)) {
                    warn!(?e, "action channel closed (ripple_path_find)");
                }
            }
            Ok(Err(e)) => {
                if let Err(e2) = action_tx.send(Action::XrplError(format!("ripple_path_find: {e}")))
                {
                    warn!(?e2, "action channel closed (ripple_path_find)");
                }
            }
            Err(_) => {
                if let Err(e) =
                    action_tx.send(Action::XrplError("ripple_path_find: timeout".into()))
                {
                    warn!(?e, "action channel closed (ripple_path_find)");
                }
            }
        }
    }
    if let Some(account_nfts_result) = account_nfts_result {
        send_rpc_outcome!(account_nfts_result, Action::XrplAccountNfts, "account_nfts");
    }
    if let Some(trust_lines_result) = trust_lines_result {
        send_rpc_outcome!(trust_lines_result, Action::XrplTrustLines, "account_lines");
    }
    if let Some(account_tx_result) = account_tx_result {
        match account_tx_result {
            Ok(result) => {
                let action = action_from_account_tx_result(result, false);
                if matches!(
                    action,
                    Action::XrplTxHistory(_, _) | Action::XrplTxHistoryAppend(_, _)
                ) {
                    any_rpc_succeeded = true;
                }
                if let Err(e) = action_tx.send(action) {
                    warn!(?e, "action channel closed");
                }
            }
            Err(_) => {
                if let Err(e) = action_tx.send(Action::XrplError("account_tx: timeout".into())) {
                    warn!(?e, "action channel closed");
                }
            }
        }
    }
    // Oracle aggregate prices (opt-in)
    if !oracles.is_empty() && !oracle_pairs.is_empty() {
        let futs: Vec<_> = oracle_pairs
            .iter()
            .map(|pair| async move {
                let result = tokio::time::timeout(
                    RPC_TIMEOUT,
                    rpc.get_aggregate_price(oracles, &pair.base_asset, &pair.quote_asset),
                )
                .await;
                (pair, result)
            })
            .collect();
        let results = futures::future::join_all(futs).await;
        let mut prices = Vec::new();
        for (pair, result) in results {
            let label = format!(
                "get_aggregate_price({}/{})",
                pair.base_asset, pair.quote_asset
            );
            match result {
                Ok(Ok(price)) => {
                    any_rpc_succeeded = true;
                    prices.push(price);
                }
                Ok(Err(e)) => {
                    if let Err(e2) = action_tx.send(Action::XrplError(format!("{label}: {e}"))) {
                        warn!(?e2, "action channel closed");
                    }
                }
                Err(_) => {
                    if let Err(e2) = action_tx.send(Action::XrplError(format!("{label}: timeout")))
                    {
                        warn!(?e2, "action channel closed");
                    }
                }
            }
        }
        if !prices.is_empty()
            && let Err(e) = action_tx.send(Action::XrplOraclePrices(prices))
        {
            warn!(?e, "action channel closed (get_aggregate_price)");
        }
    }

    // FTSO + FXRP AssetManager are shown on Overview; skip when another tab is focused.
    // Failures stay non-fatal so XRPL polling continues.
    if active_tab == 0
        && flare_display != crate::config::FlareDisplay::Off
        && let Some(flare_rpc) = flare_rpc_url
    {
        match tokio::time::timeout(
            RPC_TIMEOUT,
            crate::flare::fetch_ftso_prices(flare_rpc, flare_feeds),
        )
        .await
        {
            Ok(Ok(prices)) if !prices.is_empty() => {
                any_rpc_succeeded = true;
                if let Err(e) = action_tx.send(Action::FlareOraclePrices(prices)) {
                    warn!(?e, "action channel closed (flare ftso)");
                }
            }
            Ok(Ok(_)) | Ok(Err(_)) | Err(_) => {
                // Keep Oracle tab non-blocking when Flare endpoint/feed is unavailable.
            }
        }

        match tokio::time::timeout(
            RPC_TIMEOUT,
            crate::flare::fetch_fxrp_direct_mint_info(flare_rpc),
        )
        .await
        {
            Ok(Ok(info)) => {
                any_rpc_succeeded = true;
                if let Err(e) = action_tx.send(Action::FxrpDirectMintInfo(Box::new(info))) {
                    warn!(?e, "action channel closed (fxrp direct mint)");
                }
            }
            Ok(Err(_)) | Err(_) => {
                // Non-fatal: AssetManager read must not break XRPL poll.
            }
        }

        if let Some(wallet_addr) = flare_wallet_address {
            match tokio::time::timeout(
                RPC_TIMEOUT,
                crate::flare::fetch_flare_wallet_balance(
                    flare_rpc,
                    wallet_addr,
                    flare_fassets_execute,
                    flare_evm_key_env,
                ),
            )
            .await
            {
                Ok(Ok(summary)) => {
                    any_rpc_succeeded = true;
                    if let Err(e) = action_tx.send(Action::FlareWalletBalance(Box::new(summary))) {
                        warn!(?e, "action channel closed (flare wallet)");
                    }
                }
                Ok(Err(_)) | Err(_) => {
                    // Non-fatal: wallet read must not break XRPL poll.
                }
            }
        }
    }

    any_rpc_succeeded
}

async fn poll_wallet_overview(
    rpc: &RpcClient,
    seed_address: &str,
    action_tx: &UnboundedSender<Action>,
) -> bool {
    match tokio::time::timeout(RPC_TIMEOUT, rpc.account_overview(seed_address)).await {
        Ok(Ok((acc, txs, marker))) => {
            let page = crate::xrpl::types::AccountTxPage { rows: txs, marker };
            let tx_action = action_from_account_tx_result(Ok(page), false);
            if let Err(e) = action_tx.send(Action::XrplWalletOverview(acc)) {
                warn!(?e, "action channel closed");
            }
            if let Err(e) = action_tx.send(tx_action) {
                warn!(?e, "action channel closed");
            }
            true
        }
        Ok(Err(e)) => {
            if let Err(e) = action_tx.send(Action::XrplError(format!("wallet_overview: {e}"))) {
                warn!(?e, "action channel closed");
            }
            false
        }
        Err(_) => {
            if let Err(e) = action_tx.send(Action::XrplError("wallet_overview: timeout".into())) {
                warn!(?e, "action channel closed");
            }
            false
        }
    }
}

fn account_set_params_nonempty(p: &AccountSetSubmitParams) -> bool {
    let ds = p.domain_ascii.trim();
    let ts = p.tick_size.trim();
    let tr = p.transfer_rate.trim();
    signing::resolved_account_set_flag(&p.set_flag)
        || signing::resolved_account_set_flag(&p.clear_flag)
        || !ds.is_empty()
        || !ts.is_empty()
        || !tr.is_empty()
}

async fn submit_account_set_transaction(
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
    let account = wallet.classic_address.clone();

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

    let Some(account_info) =
        fetch_account_summary_for_submit(rpc, &account, submit_err, action_tx).await
    else {
        return;
    };

    simulate_then_finalize(
        rpc,
        action_tx,
        submit_err,
        signing::build_account_set_tx_json_for_simulate(
            &account,
            account_info.sequence,
            set_flag,
            clear_flag,
            domain_hex.as_deref(),
            tick_size,
            transfer_rate,
        ),
        |sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_account_set(
                &wallet,
                &account,
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

async fn submit_trust_set_transaction(
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

    let Some(wallet) = resolve_submit_wallet(
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        "production network: restart lazyxrp with --yes to allow TrustSet writes",
        submit_err,
        action_tx,
    ) else {
        return;
    };
    let account = wallet.classic_address.clone();

    let Some(account_info) =
        fetch_account_summary_for_submit(rpc, &account, submit_err, action_tx).await
    else {
        return;
    };

    simulate_then_finalize(
        rpc,
        action_tx,
        submit_err,
        signing::build_trust_set_tx_json_for_simulate(
            &account,
            &currency,
            &issuer,
            &limit,
            account_info.sequence,
        ),
        |sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_trust_set(
                &wallet,
                &account,
                &currency,
                &issuer,
                &limit,
                sequence,
                fee_drops,
                last_ledger_sequence,
                network,
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

async fn submit_offer_create_transaction(
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

    let Some(wallet) = resolve_submit_wallet(
        network,
        params.skip_mainnet_prompt,
        signing_credential,
        "production network: restart lazyxrp with --yes to allow OfferCreate writes",
        submit_err,
        action_tx,
    ) else {
        return;
    };
    let account = wallet.classic_address.clone();

    let Some(account_info) =
        fetch_account_summary_for_submit(rpc, &account, submit_err, action_tx).await
    else {
        return;
    };

    simulate_then_finalize(
        rpc,
        action_tx,
        submit_err,
        signing::build_offer_create_tx_json_for_simulate(
            &account,
            &taker_gets,
            &taker_pays,
            account_info.sequence,
        ),
        |sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_offer_create(
                &wallet,
                &account,
                &taker_gets,
                &taker_pays,
                sequence,
                fee_drops,
                last_ledger_sequence,
                network,
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

async fn submit_set_regular_key_transaction(
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
    let account = wallet.classic_address.clone();
    if let Some(ref key) = regular_key
        && key == &account
    {
        send_action(
            action_tx,
            submit_err("regular_key must not match the account master address".into()),
        );
        return;
    }

    let Some(account_info) =
        fetch_account_summary_for_submit(rpc, &account, submit_err, action_tx).await
    else {
        return;
    };

    simulate_then_finalize(
        rpc,
        action_tx,
        submit_err,
        signing::build_set_regular_key_tx_json_for_simulate(
            &account,
            regular_key.as_deref(),
            account_info.sequence,
        ),
        |sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_set_regular_key(
                &wallet,
                &account,
                regular_key.as_deref(),
                sequence,
                fee_drops,
                last_ledger_sequence,
                network,
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

async fn submit_payment_transaction(
    rpc: &RpcClient,
    network: &Network,
    params: PaymentSubmitParams,
    action_tx: &UnboundedSender<Action>,
    signing_credential: Option<&crate::signing::SigningCredential>,
) {
    let submit_err = Action::PaymentSubmitErr;
    if params.amount.trim().is_empty() {
        send_action(
            action_tx,
            submit_err("amount is empty — enter an amount to send".into()),
        );
        return;
    }
    let is_iou = params.iou_currency.is_some() && params.iou_issuer.is_some();
    // XRP payments debit `amount` drops; IOU payments only need XRP for the fee.
    let amount_drops = if is_iou {
        let Ok(v) = params.amount.trim().parse::<f64>() else {
            send_action(action_tx, submit_err("amount must be a number".into()));
            return;
        };
        if v <= 0.0 {
            send_action(
                action_tx,
                submit_err("amount must be greater than zero".into()),
            );
            return;
        }
        0
    } else {
        match xrp_to_drops(params.amount.trim()) {
            Ok(d) => d,
            Err(e) => {
                send_action(action_tx, submit_err(format!("amount: {e}")));
                return;
            }
        }
    };
    if !is_iou && amount_drops == 0 {
        send_action(
            action_tx,
            submit_err("amount must be greater than zero".into()),
        );
        return;
    }
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
    if account == destination_resolved {
        send_action(
            action_tx,
            submit_err("destination matches source account".into()),
        );
        return;
    }

    let Some(account_info) =
        fetch_account_summary_for_submit(rpc, &account, submit_err, action_tx).await
    else {
        return;
    };

    let tx_json = match signing::build_payment_tx_json_for_simulate(
        &account,
        &destination_resolved,
        params.amount.trim(),
        params.iou_currency.as_deref(),
        params.iou_issuer.as_deref(),
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
    let total_need = amount_drops.saturating_add(u64::from(fee_drops));
    if balance_drops < total_need {
        send_action(
            action_tx,
            submit_err(if is_iou {
                format!(
                    "insufficient XRP for fee: have {balance_drops} drops, need {fee_drops} fee"
                )
            } else {
                format!(
                    "insufficient balance: have {balance_drops} drops, need {total_need} (amount {amount_drops} + fee {fee_drops})"
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
                destination_tag,
                None,
                sequence,
                fee_drops,
                last_ledger_sequence,
                network,
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

async fn submit_fxrp_direct_mint_payment(
    rpc: &RpcClient,
    network: &Network,
    params: FxrpDirectMintPaymentParams,
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

    let Some(account_info) =
        fetch_account_summary_for_submit(rpc, &account, submit_err, action_tx).await
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
                Some(memo.as_str()),
                sequence,
                fee_drops,
                last_ledger_sequence,
                network,
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

fn dispatch_timed<T, F>(
    action_tx: &UnboundedSender<Action>,
    label: &str,
    result: Result<color_eyre::Result<T>, tokio::time::error::Elapsed>,
    ok_action: F,
) where
    F: FnOnce(T) -> Action,
{
    match result {
        Ok(Ok(value)) => {
            if let Err(e) = action_tx.send(ok_action(value)) {
                warn!(?e, "action channel closed");
            }
        }
        Ok(Err(e)) => {
            if let Err(e) = action_tx.send(Action::XrplError(format!("{label}: {e}"))) {
                warn!(?e, "action channel closed");
            }
        }
        Err(_) => {
            if let Err(e) = action_tx.send(Action::XrplError(format!("{label}: timeout"))) {
                warn!(?e, "action channel closed");
            }
        }
    }
}

async fn submit_fxrp_execute_direct_mint(
    flare_rpc_url: Option<&str>,
    flare_fassets_execute: bool,
    flare_evm_key_env: &str,
    network: &Network,
    params: FxrpExecuteDirectMintParams,
    action_tx: &UnboundedSender<Action>,
) {
    let submit_err = Action::FxrpExecuteDirectMintSubmitErr;
    if let Err(e) = crate::flare::ensure_fassets_execute_enabled(flare_fassets_execute) {
        send_action(action_tx, submit_err(format!("{e}")));
        return;
    }
    if mainnet_write_guard_blocks(network, params.skip_mainnet_prompt) {
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
    let key = match std::env::var(flare_evm_key_env) {
        Ok(k) if !k.trim().is_empty() => {
            // SAFETY: poll task is the sole reader of this env var after startup; clear it so
            // /proc/self/environ and child processes cannot observe the Flare executor key.
            unsafe { std::env::remove_var(flare_evm_key_env) };
            k
        }
        _ => {
            send_action(
                action_tx,
                submit_err(format!(
                    "env {flare_evm_key_env} not set (Flare EVM executor key)"
                )),
            );
            return;
        }
    };
    match crate::flare::execute_direct_minting(rpc, key.trim(), &params.proof_json).await {
        Ok(hash) => send_action(action_tx, Action::FxrpExecuteDirectMintSubmitOk(hash)),
        Err(e) => send_action(action_tx, submit_err(format!("{e}"))),
    };
}

async fn run_scheduled_poll<'a>(
    rpc: &'a RpcClient,
    tab_watch: &tokio::sync::watch::Receiver<usize>,
    build_batch: &impl Fn(usize) -> PollBatchInputs<'a>,
    seed_address: Option<&str>,
    action_tx: &UnboundedSender<Action>,
    backoff_secs: &mut u64,
    backoff_until: &mut Option<Instant>,
) -> Option<Instant> {
    let active_tab = *tab_watch.borrow();
    Some(
        execute_scheduled_poll(
            rpc,
            build_batch(active_tab),
            seed_address,
            action_tx,
            backoff_secs,
            backoff_until,
        )
        .await,
    )
}

/// Reset the poll backoff on success; on failure escalate via [`next_backoff_secs`]
/// and arm a backoff window observed by [`is_backoff_active`].
fn update_backoff(succeeded: bool, backoff_secs: &mut u64, backoff_until: &mut Option<Instant>) {
    if succeeded {
        *backoff_secs = 0;
        *backoff_until = None;
    } else {
        *backoff_secs = next_backoff_secs(*backoff_secs);
        *backoff_until = Some(Instant::now() + Duration::from_secs(*backoff_secs));
    }
}

async fn execute_scheduled_poll(
    rpc: &RpcClient,
    inputs: PollBatchInputs<'_>,
    seed_address: Option<&str>,
    action_tx: &UnboundedSender<Action>,
    backoff_secs: &mut u64,
    backoff_until: &mut Option<Instant>,
) -> Instant {
    // Overlap wallet overview with the main batch (same RpcClient, independent RPCs).
    let wallet_fut = async {
        match seed_address {
            Some(addr) => poll_wallet_overview(rpc, addr, action_tx).await,
            None => false,
        }
    };
    let (batch_succeeded, wallet_overview_succeeded) =
        tokio::join!(poll_batch(inputs, action_tx), wallet_fut);
    update_backoff(
        batch_succeeded || wallet_overview_succeeded,
        backoff_secs,
        backoff_until,
    );
    Instant::now()
}

fn send_account_tx_action(action_tx: &UnboundedSender<Action>, action: Action) {
    if let Err(e) = action_tx.send(action) {
        warn!(?e, "action channel closed");
    }
}

async fn drive_poll_loop(
    ctx: PollContext,
    mut refresh_rx: UnboundedReceiver<PollCommand>,
    mut poll_trigger_rx: UnboundedReceiver<()>,
    action_tx: UnboundedSender<Action>,
    cancel: CancellationToken,
) {
    let PollContext {
        rpc_url,
        custom_rpc,
        watch_address,
        book_pair,
        poll_interval,
        seed_address,
        signing_credential,
        mut network_watch,
        oracles,
        oracle_pairs,
        flare_rpc_url,
        flare_feeds,
        flare_fassets_execute,
        flare_display,
        flare_evm_key_env,
        flare_wallet_address,
        tab_watch,
        submit_lock,
    } = ctx;
    // Immutable across the loop; the seven submit arms below borrow it.
    let signing_credential = signing_credential.as_ref();
    // Wrapped in an async RwLock so the network-switch arm can swap the client
    // mid-loop; read guards are Send and safe across awaits.
    let rpc_cell = tokio::sync::RwLock::new(match RpcClient::connect(&rpc_url) {
        Ok(rpc) => rpc,
        Err(err) => {
            if let Err(e) = action_tx.send(Action::XrplError(format!("rpc init failed: {err}"))) {
                warn!(?e, "action channel closed");
            }
            return;
        }
    });
    let mut current_network = *network_watch.borrow();
    // Shared batch inputs for the two scheduled-poll arms below.
    // Inputs captured by reference via a helper defined below (network-agnostic).
    // NOTE: rpc is threaded through explicitly to allow live re-binding on switch.
    let mut backoff_secs: u64 = 0;
    let mut backoff_until: Option<Instant> = None;
    let mut tick = tokio::time::interval(poll_interval.max(Duration::from_millis(500)));
    let mut price_tick = tokio::time::interval(Duration::from_secs(90));
    let mut last_poll: Option<Instant> = None;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            network_res = network_watch.changed() => {
                if network_res.is_err() {
                    continue;
                }
                let new_network = *network_watch.borrow();
                if new_network == current_network {
                    continue;
                }
                current_network = new_network;
                if custom_rpc {
                    continue;
                }
                match RpcClient::connect(new_network.rpc_url()) {
                    Ok(new_rpc) => {
                        *rpc_cell.write().await = new_rpc;
                        warn!(network = ?new_network, "rpc client rebound");
                        send_action(&action_tx, Action::RefreshAccount);
                        send_action(&action_tx, Action::RefreshBook);
                        send_action(&action_tx, Action::RefreshNfts);
                        send_action(&action_tx, Action::RefreshLines);
                        send_action(&action_tx, Action::RefreshTxHistory);
                        send_action(&action_tx, Action::RefreshLedgerObjects);
                    }
                    Err(err) => {
                        warn!(?err, "rpc reconnect after network switch failed");
                        continue;
                    }
                };
            }
            _ = tick.tick() => {
                if is_backoff_active(backoff_until) {
                    continue;
                }
                let rpc = &*rpc_cell.read().await;
                last_poll = run_scheduled_poll(
                    rpc,
                    &tab_watch,
                    &|active_tab| PollBatchInputs {
                        rpc,
                        watch_address: &watch_address,
                        book_pair: &book_pair,
                        oracles: &oracles,
                        oracle_pairs: &oracle_pairs,
                        flare_rpc_url: flare_rpc_url.as_deref(),
                        flare_feeds: &flare_feeds,
                        flare_display,
                        flare_wallet_address: flare_wallet_address.as_deref(),
                        flare_fassets_execute,
                        flare_evm_key_env: &flare_evm_key_env,
                        skip_account_tx: seed_address.is_some(),
                        active_tab,
                    },
                    seed_address.as_deref(),
                    &action_tx,
                    &mut backoff_secs,
                    &mut backoff_until,
                )
                .await;
            }
            Some(()) = poll_trigger_rx.recv() => {
                drain_poll_trigger_burst(&mut poll_trigger_rx);
                if is_backoff_active(backoff_until) || should_skip_poll_trigger(last_poll) {
                    continue;
                }
                let rpc = &*rpc_cell.read().await;
                last_poll = run_scheduled_poll(
                    rpc,
                    &tab_watch,
                    &|active_tab| PollBatchInputs {
                        rpc,
                        watch_address: &watch_address,
                        book_pair: &book_pair,
                        oracles: &oracles,
                        oracle_pairs: &oracle_pairs,
                        flare_rpc_url: flare_rpc_url.as_deref(),
                        flare_feeds: &flare_feeds,
                        flare_display,
                        flare_wallet_address: flare_wallet_address.as_deref(),
                        flare_fassets_execute,
                        flare_evm_key_env: &flare_evm_key_env,
                        skip_account_tx: seed_address.is_some(),
                        active_tab,
                    },
                    seed_address.as_deref(),
                    &action_tx,
                    &mut backoff_secs,
                    &mut backoff_until,
                )
                .await;
            }
            _ = price_tick.tick() => {
                let price = tokio::time::timeout(
                    RPC_TIMEOUT,
                    rpc_cell.read().await.book_mid_price(book_pair.pays_currency(), &book_pair.issuer),
                )
                .await;
                match price {
                    Ok(Ok(p)) => send_action(&action_tx, Action::BookMidPrice(p)),
                    Ok(Err(e)) => send_action(&action_tx, Action::XrplError(format!("price: {e}"))),
                    Err(_) => send_action(&action_tx, Action::XrplError("price: timeout".into())),
                };
            }
            Some(cmd) = refresh_rx.recv() => {
                let network = *network_watch.borrow();
                match cmd {
                    PollCommand::Account => dispatch_timed(
                        &action_tx,
                        "account_info",
                        tokio::time::timeout(RPC_TIMEOUT, rpc_cell.read().await.account_info(&watch_address)).await,
                        |account| Action::XrplAccount(Box::new(account)),
                    ),
                    PollCommand::Book => dispatch_timed(
                        &action_tx,
                        "book_offers",
                        tokio::time::timeout(
                            RPC_TIMEOUT,
                            rpc_cell.read().await.book_offers(
                                book_pair.gets_currency(),
                                book_pair.gets_issuer(),
                                book_pair.pays_currency(),
                                book_pair.pays_issuer(),
                                book_pair.limit,
                            ),
                        )
                        .await,
                        Action::XrplBookOffers,
                    ),
                    PollCommand::Nfts => dispatch_timed(
                        &action_tx,
                        "account_nfts",
                        tokio::time::timeout(RPC_TIMEOUT, rpc_cell.read().await.account_nfts(&watch_address)).await,
                        Action::XrplAccountNfts,
                    ),
                    PollCommand::Lines => dispatch_timed(
                        &action_tx,
                        "account_lines",
                        tokio::time::timeout(RPC_TIMEOUT, rpc_cell.read().await.account_lines(&watch_address)).await,
                        Action::XrplTrustLines,
                    ),
                    PollCommand::TxHistory => {
                        dispatch_account_tx(&*rpc_cell.read().await, &watch_address, None, false, &action_tx)
                            .await;
                    }
                    PollCommand::TxHistoryMore(marker) => {
                        dispatch_account_tx(&*rpc_cell.read().await, &watch_address, marker, true, &action_tx)
                            .await;
                    }
                    PollCommand::LedgerObjects => dispatch_timed(
                        &action_tx,
                        "account_objects",
                        tokio::time::timeout(RPC_TIMEOUT, rpc_cell.read().await.account_objects(&watch_address)).await,
                        Action::XrplLedgerObjects,
                    ),
                    PollCommand::AccountSetSubmit(params) => {
                        let _guard = submit_lock.lock().await;
                        submit_account_set_transaction(&*rpc_cell.read().await, &network, params, &action_tx, signing_credential).await;
                    }
                    PollCommand::PaymentSubmit(params) => {
                        let _guard = submit_lock.lock().await;
                        submit_payment_transaction(&*rpc_cell.read().await, &network, params, &action_tx, signing_credential).await;
                    }
                    PollCommand::FxrpDirectMintPayment(params) => {
                        let _guard = submit_lock.lock().await;
                        submit_fxrp_direct_mint_payment(&*rpc_cell.read().await, &network, params, &action_tx, signing_credential).await;
                    }
                    PollCommand::FxrpExecuteDirectMint(params) => {
                        submit_fxrp_execute_direct_mint(
                            flare_rpc_url.as_deref(),
                            flare_fassets_execute,
                            &flare_evm_key_env,
                            &network,
                            params,
                            &action_tx,
                        ).await;
                    }
                    PollCommand::SetRegularKeySubmit(params) => {
                        let _guard = submit_lock.lock().await;
                        submit_set_regular_key_transaction(&*rpc_cell.read().await, &network, params, &action_tx, signing_credential).await;
                    }
                    PollCommand::OfferCreateSubmit(params) => {
                        let _guard = submit_lock.lock().await;
                        submit_offer_create_transaction(&*rpc_cell.read().await, &network, params, &action_tx, signing_credential).await;
                    }
                    PollCommand::TrustSetSubmit(params) => {
                        let _guard = submit_lock.lock().await;
                        submit_trust_set_transaction(&*rpc_cell.read().await, &network, params, &action_tx, signing_credential).await;
                    }
                    PollCommand::GenerateWalletKeys(key_type) => {
                        match crate::signing::generate_wallet_keys_local(&key_type) {
                            Ok(result) => {
                                if let Err(e) = action_tx.send(Action::GenerateWalletKeysOk(result)) {
                                    warn!(?e, "action channel closed");
                                }
                            }
                            Err(e) => {
                                if let Err(e) =
                                    action_tx.send(Action::GenerateWalletKeysErr(format!("{e}")))
                                {
                                    warn!(?e, "action channel closed");
                                }
                            }
                        }
                    }
                    // EscrowCreate remains deferred: see map out-of-scope.
                    // Do not silently drop new submit commands here.
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use tokio::sync::mpsc;

    use super::*;
    use crate::network::Network;
    use crate::signing::SEED_ENV;
    use crate::test_support::{TestEnvGuard, env_lock_async};
    use crate::xrpl::client::RpcClient;
    use crate::xrpl::types::{
        AccountSetSubmitParams, FxrpDirectMintPaymentParams, FxrpExecuteDirectMintParams,
        OfferCreateSubmitParams, PaymentSubmitParams, SetRegularKeySubmitParams,
        TrustSetSubmitParams,
    };

    /// TC-087: poll trigger burst drain
    #[test]
    fn drain_poll_trigger_burst_coalesces_pending_triggers() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        for _ in 0..3 {
            tx.send(()).expect("send trigger");
        }
        assert!(rx.try_recv().is_ok());
        drain_poll_trigger_burst(&mut rx);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn should_poll_market_book_only_on_market_tab() {
        assert!(!should_poll_market_book(0));
        assert!(!should_poll_market_book(1));
        assert!(should_poll_market_book(TAB_MARKET));
        assert!(!should_poll_market_book(TAB_ASSETS));
    }

    #[test]
    fn should_poll_asset_panels_only_on_assets_tab() {
        assert!(!should_poll_asset_panels(0));
        assert!(should_poll_asset_panels(TAB_ASSETS));
        assert!(!should_poll_asset_panels(TAB_MARKET));
    }

    #[test]
    fn should_skip_poll_trigger_within_min_interval() {
        assert!(should_skip_poll_trigger(Some(Instant::now())));
        assert!(!should_skip_poll_trigger(None));
    }

    #[test]
    fn should_skip_poll_trigger_after_min_interval() {
        let last = Instant::now() - MIN_POLL_INTERVAL - Duration::from_millis(1);
        assert!(!should_skip_poll_trigger(Some(last)));
    }

    #[test]
    fn is_backoff_active_none_is_inactive() {
        assert!(!is_backoff_active(None));
    }

    #[test]
    fn is_backoff_active_future_deadline() {
        assert!(is_backoff_active(Some(
            Instant::now() + Duration::from_secs(30)
        )));
    }

    #[test]
    fn is_backoff_active_past_deadline() {
        assert!(!is_backoff_active(Some(
            Instant::now() - Duration::from_millis(1)
        )));
    }

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

    /// TC-089 (I-7): account_tx not-found in poll batch → empty history, not error
    #[test]
    fn action_from_account_tx_result_not_found_returns_empty_history() {
        let err = color_eyre::eyre::eyre!("actNotFound");
        match action_from_account_tx_result(Err(err), false) {
            Action::XrplTxHistory(rows, marker) => {
                assert!(rows.is_empty());
                assert!(marker.is_none());
            }
            other => panic!("expected empty history, got {other:?}"),
        }
    }

    #[test]
    fn action_from_account_tx_result_other_error_is_xrpl_error() {
        let err = color_eyre::eyre::eyre!("timeout");
        match action_from_account_tx_result(Err(err), false) {
            Action::XrplError(msg) => assert!(msg.contains("account_tx")),
            other => panic!("expected XrplError, got {other:?}"),
        }
    }

    #[test]
    fn mainnet_write_guard_blocks_without_yes() {
        assert!(mainnet_write_guard_blocks(&Network::Mainnet, false));
        assert!(mainnet_write_guard_blocks(&Network::Xahau, false));
        assert!(!mainnet_write_guard_blocks(&Network::Mainnet, true));
        assert!(!mainnet_write_guard_blocks(&Network::Testnet, false));
        assert!(!mainnet_write_guard_blocks(&Network::XahauTest, false));
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
                | Action::SetRegularKeySubmitErr(m)
                | Action::FxrpDirectMintPaymentSubmitErr(m) => m,
                other => panic!("{name}: expected submit error, got {other:?}"),
            };
            assert!(msg.contains("production network"), "{name}: {msg}");
            assert!(msg.contains("--yes"), "{name}: {msg}");
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

    #[tokio::test]
    async fn payment_submit_mainnet_with_yes_skips_mainnet_guard() {
        let _env_lock = env_lock_async().await;
        let _test_env = TestEnvGuard::new(&[SEED_ENV]);
        _test_env.remove(SEED_ENV);
        let (action_tx, mut action_rx) = mpsc::unbounded_channel();
        let rpc = RpcClient::connect("http://127.0.0.1:1").expect("rpc client");
        let params = PaymentSubmitParams {
            destination: "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into(),
            amount: "0.001".into(),
            iou_currency: None,
            iou_issuer: None,
            destination_tag: None,
            skip_mainnet_prompt: true,
        };
        submit_payment_transaction(&rpc, &Network::Mainnet, params, &action_tx, None).await;
        let action = action_rx.recv().await.expect("action");
        match action {
            Action::PaymentSubmitErr(msg) => {
                assert!(
                    !msg.contains("restart lazyxrp with --yes"),
                    "mainnet guard should be skipped when --yes is set: {msg}"
                );
                assert!(
                    msg.contains("no signing credential"),
                    "expected seed error after guard skip, got: {msg}"
                );
            }
            other => panic!("expected PaymentSubmitErr after guard skip, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn set_regular_key_submit_mainnet_with_yes_skips_mainnet_guard() {
        let _env_lock = env_lock_async().await;
        let _test_env = TestEnvGuard::new(&[SEED_ENV]);
        _test_env.remove(SEED_ENV);
        let (action_tx, mut action_rx) = mpsc::unbounded_channel();
        let rpc = RpcClient::connect("http://127.0.0.1:1").expect("rpc client");
        let params = SetRegularKeySubmitParams {
            regular_key: String::new(), // clear
            skip_mainnet_prompt: true,
        };
        submit_set_regular_key_transaction(&rpc, &Network::Mainnet, params, &action_tx, None).await;
        let action = action_rx.recv().await.expect("action");
        match action {
            Action::SetRegularKeySubmitErr(msg) => {
                assert!(
                    !msg.contains("restart lazyxrp with --yes"),
                    "mainnet guard should be skipped when --yes is set: {msg}"
                );
                assert!(
                    msg.contains("no signing credential"),
                    "expected seed error after guard skip, got: {msg}"
                );
            }
            other => panic!("expected SetRegularKeySubmitErr after guard skip, got {other:?}"),
        }
    }

    /// TC-144: `account_tx` success mapping follows the append flag, and a
    /// not-found page on the append path still yields `XrplTxHistoryAppend`.
    #[test]
    fn action_from_account_tx_result_follows_append_flag() {
        use crate::xrpl::types::AccountTxPage;
        let page = AccountTxPage {
            rows: Vec::new(),
            marker: Some(serde_json::json!({"ledger_index": 1})),
        };
        match action_from_account_tx_result(Ok(page.clone()), false) {
            Action::XrplTxHistory(rows, marker) => {
                assert!(rows.is_empty());
                assert_eq!(marker, page.marker);
            }
            other => panic!("expected XrplTxHistory, got {other:?}"),
        }
        let marker_expected = page.marker.clone();
        match action_from_account_tx_result(Ok(page), true) {
            Action::XrplTxHistoryAppend(rows, marker) => {
                assert!(rows.is_empty());
                assert_eq!(marker, marker_expected);
            }
            other => panic!("expected XrplTxHistoryAppend, got {other:?}"),
        }
    }

    /// TC-144: not-found error on the append path becomes an empty
    /// `XrplTxHistoryAppend`, not `XrplError`.
    #[test]
    fn action_from_account_tx_result_not_found_append_yields_empty_append() {
        let err = color_eyre::eyre::eyre!("actNotFound");
        match action_from_account_tx_result(Err(err), true) {
            Action::XrplTxHistoryAppend(rows, marker) => {
                assert!(rows.is_empty());
                assert!(marker.is_none());
            }
            other => panic!("expected empty XrplTxHistoryAppend, got {other:?}"),
        }
    }

    /// TC-145: a failed scheduled poll escalates `backoff_secs` and arms an
    /// open backoff window; success resets both.
    #[test]
    fn scheduled_poll_backoff_escalates_then_resets() {
        let mut secs = 0_u64;
        let mut until = None;
        for expected in [2, 4, 8] {
            update_backoff(false, &mut secs, &mut until);
            assert_eq!(secs, expected, "backoff must escalate after failure");
            let deadline = until.expect("backoff window must be armed after failure");
            assert!(
                is_backoff_active(Some(deadline)),
                "armed window must be open immediately after failure"
            );
            assert!(
                deadline <= Instant::now() + Duration::from_secs(secs),
                "window must be no longer than the escalated backoff"
            );
        }
        update_backoff(true, &mut secs, &mut until);
        assert_eq!(secs, 0, "success must reset backoff to the floor");
        assert!(until.is_none(), "success must clear the backoff window");
    }
}
