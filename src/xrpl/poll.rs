use std::time::{Duration, Instant};

use secrecy::ExposeSecret;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;

use tracing::warn;

use super::address::{ensure_xaddress_matches_network, resolve_payment_destination};
use crate::action::Action;
use crate::network::Network;
use crate::signing::{self, SigningConfig};

use super::backoff::next_backoff_secs;
use super::client::{
    RPC_TIMEOUT, RpcClient, empty_account_tx_page_on_not_found, path_find_snapshot, xrp_to_drops,
};
use super::types::{
    AccountSetSubmitParams, BookPair, OracleId, PaymentSubmitParams, PollCommand, PollContext,
    SimulateResult,
};
use serde_json::Value;

pub fn start_poll_task(
    ctx: PollContext,
    refresh_rx: UnboundedReceiver<PollCommand>,
    poll_trigger_rx: UnboundedReceiver<()>,
    action_tx: UnboundedSender<Action>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run_poll_loop(
        ctx,
        refresh_rx,
        poll_trigger_rx,
        action_tx,
        cancel,
    ))
}

const MIN_POLL_INTERVAL: Duration = Duration::from_secs(10);

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
        Err(_) => Err("simulate: timeout".into()),
    }
}

fn mainnet_write_guard_blocks(network: &Network, skip_mainnet_prompt: bool) -> bool {
    network.is_mainnet() && !skip_mainnet_prompt
}

/// Mainnet guard, then load seed + wallet for AccountSet / Payment submit.
fn resolve_submit_wallet<E>(
    network: &Network,
    skip_mainnet_prompt: bool,
    config_seed: Option<String>,
    mainnet_err: &str,
    err: E,
    action_tx: &UnboundedSender<Action>,
) -> Option<(secrecy::SecretString, xrpl::wallet::Wallet)>
where
    E: Fn(String) -> Action,
{
    if mainnet_write_guard_blocks(network, skip_mainnet_prompt) {
        send_action(action_tx, err(mainnet_err.into()));
        return None;
    }
    let signing_config = SigningConfig::prime_seed_source(config_seed);
    let Some(seed) = signing_config.seed else {
        send_action(
            action_tx,
            err("no signing seed — set XRPL_SEED or config [xrpl.signing] seed".into()),
        );
        return None;
    };
    match signing::wallet_from_family_seed(seed.expose_secret(), 0) {
        Ok(wallet) => Some((seed, wallet)),
        Err(e) => {
            send_action(action_tx, err(format!("wallet: {e:?}")));
            None
        }
    }
}

fn send_action(action_tx: &UnboundedSender<Action>, action: Action) {
    if let Err(e) = action_tx.send(action) {
        warn!(?e, "action channel closed");
    }
}

async fn fetch_account_summary_for_submit<E>(
    rpc: &RpcClient,
    account: &str,
    err: E,
    action_tx: &UnboundedSender<Action>,
) -> Option<crate::xrpl::types::AccountSummary>
where
    E: Fn(String) -> Action,
{
    match tokio::time::timeout(RPC_TIMEOUT, rpc.account_info(account)).await {
        Ok(Ok(summary)) => Some(summary),
        Ok(Err(e)) => {
            send_action(action_tx, err(format!("account_info: {e}")));
            None
        }
        Err(_) => {
            send_action(action_tx, err("account_info: timeout".into()));
            None
        }
    }
}

async fn finalize_simulate_sign_submit<E, FO, FS>(
    rpc: &RpcClient,
    action_tx: &UnboundedSender<Action>,
    sim: SimulateResult,
    sign_blob: FS,
    err: E,
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
                send_action(action_tx, err(format!("{e}")));
                return;
            }
        };

    let blob = match sign_blob(sequence, fee_drops, last_ledger_sequence) {
        Ok(b) => b,
        Err(e) => {
            send_action(action_tx, err(format!("sign: {e}")));
            return;
        }
    };

    match tokio::time::timeout(RPC_TIMEOUT, rpc.submit_signed_tx(&blob)).await {
        Ok(Ok(tx)) => {
            for action in on_ok(tx.hash) {
                send_action(action_tx, action);
            }
        }
        Ok(Err(e)) => send_action(action_tx, err(format!("submit: {e}"))),
        Err(_) => send_action(action_tx, err("submit: timeout".into())),
    }
}

struct PollBatchInputs<'a> {
    rpc: &'a RpcClient,
    watch_address: &'a str,
    book_pair: &'a BookPair,
    oracles: &'a [OracleId],
    oracle_pairs: &'a [crate::xrpl::OraclePricePair],
    flare_rpc_url: Option<&'a str>,
    flare_feeds: &'a [String],
    /// When a signing seed is configured, `poll_wallet_overview` fetches `account_tx` once.
    skip_account_tx: bool,
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
        skip_account_tx,
    } = inputs;
    let dest_amount = book_pair.path_find_destination_amount_preview();
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
        tokio::time::timeout(
            RPC_TIMEOUT,
            rpc.book_offers(
                book_pair.gets_currency(),
                book_pair.gets_issuer(),
                book_pair.pays_currency(),
                book_pair.pays_issuer(),
                book_pair.limit
            )
        ),
        tokio::time::timeout(
            RPC_TIMEOUT,
            rpc.ripple_path_find(watch_address, watch_address, &dest_amount),
        ),
        tokio::time::timeout(RPC_TIMEOUT, rpc.account_nfts(watch_address)),
        tokio::time::timeout(RPC_TIMEOUT, rpc.account_lines(watch_address)),
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
    send_rpc_outcome!(book_offers_result, Action::XrplBookOffers, "book_offers");
    match path_find_result {
        Ok(Ok(v)) => {
            any_rpc_succeeded = true;
            let snap = path_find_snapshot(&v, &book_pair.quote);
            if let Err(e) = action_tx.send(Action::XrplPathFind(snap)) {
                warn!(?e, "action channel closed (ripple_path_find)");
            }
        }
        Ok(Err(e)) => {
            if let Err(e2) = action_tx.send(Action::XrplError(format!("ripple_path_find: {e}"))) {
                warn!(?e2, "action channel closed (ripple_path_find)");
            }
        }
        Err(_) => {
            if let Err(e) = action_tx.send(Action::XrplError("ripple_path_find: timeout".into())) {
                warn!(?e, "action channel closed (ripple_path_find)");
            }
        }
    }
    send_rpc_outcome!(account_nfts_result, Action::XrplAccountNfts, "account_nfts");
    send_rpc_outcome!(trust_lines_result, Action::XrplTrustLines, "account_lines");
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

    if let Some(flare_rpc) = flare_rpc_url {
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
) {
    let err = Action::AccountSetSubmitErr;
    if !account_set_params_nonempty(&params) {
        send_action(
            action_tx,
            err(
                "nothing to change — pick a flag and/or fill domain, tick size, transfer rate"
                    .into(),
            ),
        );
        return;
    }
    let Some((seed, wallet)) = resolve_submit_wallet(
        network,
        params.skip_mainnet_prompt,
        params.config_seed.clone(),
        "mainnet: restart lazyxrp with --yes to allow AccountSet writes",
        err,
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
                    err("tick size: invalid number (use 0 or 3–15)".into()),
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
                send_action(action_tx, err("transfer rate: invalid number".into()));
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

    let Some(account_info) = fetch_account_summary_for_submit(rpc, &account, err, action_tx).await
    else {
        return;
    };

    let tx_json = match signing::build_account_set_tx_json_for_simulate(
        &account,
        account_info.sequence,
        set_flag,
        clear_flag,
        domain_hex.as_deref(),
        tick_size,
        transfer_rate,
    ) {
        Ok(j) => j,
        Err(e) => {
            send_action(action_tx, err(format!("tx_json: {e}")));
            return;
        }
    };

    let sim = match simulate_tx_requiring_tes_success(rpc, tx_json).await {
        Ok(s) => s,
        Err(e) => {
            send_action(action_tx, err(e));
            return;
        }
    };

    finalize_simulate_sign_submit(
        rpc,
        action_tx,
        sim,
        |sequence, fee_drops, last_ledger_sequence| {
            signing::create_and_sign_account_set(
                &seed,
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
        err,
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

async fn submit_payment_transaction(
    rpc: &RpcClient,
    network: &Network,
    params: PaymentSubmitParams,
    action_tx: &UnboundedSender<Action>,
) {
    let err = Action::PaymentSubmitErr;
    if params.amount.trim().is_empty() {
        send_action(
            action_tx,
            err("amount is empty — enter an amount to send".into()),
        );
        return;
    }
    let is_iou = params.iou_currency.is_some() && params.iou_issuer.is_some();
    // XRP payments debit `amount` drops; IOU payments only need XRP for the fee.
    let amount_drops = if is_iou {
        let Ok(v) = params.amount.trim().parse::<f64>() else {
            send_action(action_tx, err("amount must be a number".into()));
            return;
        };
        if v <= 0.0 {
            send_action(action_tx, err("amount must be greater than zero".into()));
            return;
        }
        0
    } else {
        match xrp_to_drops(params.amount.trim()) {
            Ok(d) => d,
            Err(e) => {
                send_action(action_tx, err(format!("amount: {e}")));
                return;
            }
        }
    };
    if !is_iou && amount_drops == 0 {
        send_action(action_tx, err("amount must be greater than zero".into()));
        return;
    }
    let destination = match resolve_payment_destination(params.destination.trim()) {
        Ok(d) => d,
        Err(e) => {
            send_action(action_tx, err(format!("{e}")));
            return;
        }
    };
    if let Err(e) = ensure_xaddress_matches_network(&destination, network) {
        send_action(action_tx, err(format!("{e}")));
        return;
    }
    let destination_resolved = destination.classic;
    let destination_tag = params.destination_tag.or(destination.destination_tag);
    let Some((seed, wallet)) = resolve_submit_wallet(
        network,
        params.skip_mainnet_prompt,
        params.config_seed.clone(),
        "mainnet: restart lazyxrp with --yes to allow Payment writes",
        err,
        action_tx,
    ) else {
        return;
    };
    let account = wallet.classic_address.clone();
    if account == destination_resolved {
        send_action(action_tx, err("destination matches source account".into()));
        return;
    }

    let Some(account_info) = fetch_account_summary_for_submit(rpc, &account, err, action_tx).await
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
        account_info.sequence,
    ) {
        Ok(j) => j,
        Err(e) => {
            send_action(action_tx, err(format!("tx_json: {e}")));
            return;
        }
    };

    let sim = match simulate_tx_requiring_tes_success(rpc, tx_json).await {
        Ok(s) => s,
        Err(e) => {
            send_action(action_tx, err(e));
            return;
        }
    };

    let fee_drops = match signing::sequence_fee_ledger_from_simulate(&sim.tx_json) {
        Ok((_, fee, _)) => fee,
        Err(e) => {
            send_action(action_tx, err(format!("{e}")));
            return;
        }
    };

    let balance_drops = xrp_to_drops(&account_info.balance_xrp).unwrap_or(0);
    let total_need = amount_drops.saturating_add(u64::from(fee_drops));
    if balance_drops < total_need {
        send_action(
            action_tx,
            err(if is_iou {
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
                &seed,
                &account,
                &destination_resolved,
                params.amount.trim(),
                params.iou_currency.as_deref(),
                params.iou_issuer.as_deref(),
                destination_tag,
                sequence,
                fee_drops,
                last_ledger_sequence,
                network,
            )
        },
        err,
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

async fn run_scheduled_poll(
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
    if batch_succeeded || wallet_overview_succeeded {
        *backoff_secs = 0;
        *backoff_until = None;
    } else {
        *backoff_secs = next_backoff_secs(*backoff_secs);
        *backoff_until = Some(Instant::now() + Duration::from_secs(*backoff_secs));
    }
    Instant::now()
}

fn send_account_tx_action(action_tx: &UnboundedSender<Action>, action: Action) {
    if let Err(e) = action_tx.send(action) {
        warn!(?e, "action channel closed");
    }
}

async fn run_poll_loop(
    ctx: PollContext,
    mut refresh_rx: UnboundedReceiver<PollCommand>,
    mut poll_trigger_rx: UnboundedReceiver<()>,
    action_tx: UnboundedSender<Action>,
    cancel: CancellationToken,
) {
    let PollContext {
        rpc_url,
        watch_address,
        book_pair,
        poll_interval,
        seed_address,
        network_watch,
        oracles,
        oracle_pairs,
        flare_rpc_url,
        flare_feeds,
    } = ctx;
    let rpc = match RpcClient::connect(&rpc_url) {
        Ok(rpc) => rpc,
        Err(err) => {
            if let Err(e) = action_tx.send(Action::XrplError(format!("rpc init failed: {err}"))) {
                warn!(?e, "action channel closed");
            }
            return;
        }
    };
    let mut backoff_secs: u64 = 0;
    let mut backoff_until: Option<Instant> = None;
    let mut tick = tokio::time::interval(poll_interval.max(Duration::from_millis(500)));
    let mut price_tick = tokio::time::interval(Duration::from_secs(90));
    let mut last_poll: Option<Instant> = None;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tick.tick() => {
                if is_backoff_active(backoff_until) {
                    continue;
                }
                last_poll = Some(
                    run_scheduled_poll(
                        &rpc,
                        PollBatchInputs {
                            rpc: &rpc,
                            watch_address: &watch_address,
                            book_pair: &book_pair,
                            oracles: &oracles,
                            oracle_pairs: &oracle_pairs,
                            flare_rpc_url: flare_rpc_url.as_deref(),
                            flare_feeds: &flare_feeds,
                            skip_account_tx: seed_address.is_some(),
                        },
                        seed_address.as_deref(),
                        &action_tx,
                        &mut backoff_secs,
                        &mut backoff_until,
                    )
                    .await,
                );
            }
            Some(()) = poll_trigger_rx.recv() => {
                drain_poll_trigger_burst(&mut poll_trigger_rx);
                if is_backoff_active(backoff_until) {
                    continue;
                }
                if should_skip_poll_trigger(last_poll) {
                    continue;
                }
                last_poll = Some(
                    run_scheduled_poll(
                        &rpc,
                        PollBatchInputs {
                            rpc: &rpc,
                            watch_address: &watch_address,
                            book_pair: &book_pair,
                            oracles: &oracles,
                            oracle_pairs: &oracle_pairs,
                            flare_rpc_url: flare_rpc_url.as_deref(),
                            flare_feeds: &flare_feeds,
                            skip_account_tx: seed_address.is_some(),
                        },
                        seed_address.as_deref(),
                        &action_tx,
                        &mut backoff_secs,
                        &mut backoff_until,
                    )
                    .await,
                );
            }
            _ = price_tick.tick() => {
                match tokio::time::timeout(RPC_TIMEOUT, rpc.xrp_rlusd_price(book_pair.pays_currency(), &book_pair.issuer)).await {
                    Ok(Ok(p)) => { if let Err(e) = action_tx.send(Action::XrplRlusdPrice(p)) { warn!(?e, "action channel closed"); } }
                    Ok(Err(e)) => { if let Err(e) = action_tx.send(Action::XrplError(format!("price: {e}"))) { warn!(?e, "action channel closed"); } }
                    Err(_) => { if let Err(e) = action_tx.send(Action::XrplError("price: timeout".into())) { warn!(?e, "action channel closed"); } }
                }
            }
            Some(cmd) = refresh_rx.recv() => {
                match cmd {
                    PollCommand::Account => dispatch_timed(
                        &action_tx,
                        "account_info",
                        tokio::time::timeout(RPC_TIMEOUT, rpc.account_info(&watch_address)).await,
                        |account| Action::XrplAccount(Box::new(account)),
                    ),
                    PollCommand::Book => dispatch_timed(
                        &action_tx,
                        "book_offers",
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
                        Action::XrplBookOffers,
                    ),
                    PollCommand::Nfts => dispatch_timed(
                        &action_tx,
                        "account_nfts",
                        tokio::time::timeout(RPC_TIMEOUT, rpc.account_nfts(&watch_address)).await,
                        Action::XrplAccountNfts,
                    ),
                    PollCommand::Lines => dispatch_timed(
                        &action_tx,
                        "account_lines",
                        tokio::time::timeout(RPC_TIMEOUT, rpc.account_lines(&watch_address)).await,
                        Action::XrplTrustLines,
                    ),
                    PollCommand::TxHistory => match tokio::time::timeout(
                        RPC_TIMEOUT,
                        rpc.account_tx(&watch_address, 20, None),
                    )
                    .await
                    {
                        Ok(result) => {
                            send_account_tx_action(&action_tx, action_from_account_tx_result(result, false));
                        }
                        Err(_) => send_account_tx_action(
                            &action_tx,
                            Action::XrplError("account_tx: timeout".into()),
                        ),
                    },
                    PollCommand::TxHistoryMore(marker) => match tokio::time::timeout(
                        RPC_TIMEOUT,
                        rpc.account_tx(&watch_address, 20, marker),
                    )
                    .await
                    {
                        Ok(result) => {
                            send_account_tx_action(&action_tx, action_from_account_tx_result(result, true));
                        }
                        Err(_) => send_account_tx_action(
                            &action_tx,
                            Action::XrplError("account_tx: timeout".into()),
                        ),
                    },
                    PollCommand::LedgerObjects => dispatch_timed(
                        &action_tx,
                        "account_objects",
                        tokio::time::timeout(RPC_TIMEOUT, rpc.account_objects(&watch_address)).await,
                        Action::XrplLedgerObjects,
                    ),
                    PollCommand::AccountSetSubmit(params) => {
                        let network = *network_watch.borrow();
                        submit_account_set_transaction(&rpc, &network, params, &action_tx).await;
                    }
                    PollCommand::PaymentSubmit(params) => {
                        let network = *network_watch.borrow();
                        submit_payment_transaction(&rpc, &network, params, &action_tx).await;
                    }
                    PollCommand::WalletPropose(key_type) => {
                        match crate::signing::propose_wallet_local(&key_type) {
                            Ok(result) => {
                                if let Err(e) = action_tx.send(Action::WalletProposeOk(result)) {
                                    warn!(?e, "action channel closed");
                                }
                            }
                            Err(e) => {
                                if let Err(e) =
                                    action_tx.send(Action::WalletProposeErr(format!("{e}")))
                                {
                                    warn!(?e, "action channel closed");
                                }
                            }
                        }
                    }
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
    use crate::config::{TestEnvGuard, env_lock};
    use crate::network::Network;
    use crate::signing::SEED_ENV;
    use crate::xrpl::client::RpcClient;
    use crate::xrpl::types::PaymentSubmitParams;

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
    fn should_skip_poll_trigger_within_min_interval() {
        assert!(should_skip_poll_trigger(Some(Instant::now())));
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
        assert!(!mainnet_write_guard_blocks(&Network::Mainnet, true));
        assert!(!mainnet_write_guard_blocks(&Network::Testnet, false));
    }

    /// TC-088 (R-006): mainnet Payment without `--yes` is rejected before RPC/signing
    #[tokio::test]
    async fn payment_submit_mainnet_without_yes_is_rejected() {
        let (action_tx, mut action_rx) = mpsc::unbounded_channel();
        let rpc = RpcClient::connect("http://127.0.0.1:1").expect("rpc client");
        let params = PaymentSubmitParams {
            destination: "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into(),
            amount: "0.001".into(),
            iou_currency: None,
            iou_issuer: None,
            destination_tag: None,
            skip_mainnet_prompt: false,
            config_seed: None,
        };
        submit_payment_transaction(&rpc, &Network::Mainnet, params, &action_tx).await;
        let action = action_rx.recv().await.expect("action");
        match action {
            Action::PaymentSubmitErr(msg) => {
                assert!(msg.contains("mainnet"));
                assert!(msg.contains("--yes"));
            }
            other => panic!("expected PaymentSubmitErr, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn payment_submit_mainnet_with_yes_skips_mainnet_guard() {
        let _env = {
            let _g = env_lock();
            let env = TestEnvGuard::new(&[SEED_ENV]);
            env.remove(SEED_ENV);
            env
        };
        let (action_tx, mut action_rx) = mpsc::unbounded_channel();
        let rpc = RpcClient::connect("http://127.0.0.1:1").expect("rpc client");
        let params = PaymentSubmitParams {
            destination: "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into(),
            amount: "0.001".into(),
            iou_currency: None,
            iou_issuer: None,
            destination_tag: None,
            skip_mainnet_prompt: true,
            config_seed: None,
        };
        submit_payment_transaction(&rpc, &Network::Mainnet, params, &action_tx).await;
        let action = action_rx.recv().await.expect("action");
        match action {
            Action::PaymentSubmitErr(msg) => {
                assert!(
                    !msg.contains("restart lazyxrp with --yes"),
                    "mainnet guard should be skipped when --yes is set: {msg}"
                );
                assert!(
                    msg.contains("no signing seed"),
                    "expected seed error after guard skip, got: {msg}"
                );
            }
            other => panic!("expected PaymentSubmitErr after guard skip, got {other:?}"),
        }
    }

    #[test]
    fn resolve_xaddress_preserves_destination_tag() {
        use xrpl::core::addresscodec::classic_address_to_xaddress;
        let classic = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        let xaddr = classic_address_to_xaddress(classic, Some(42), false).expect("xaddr");
        let resolved = resolve_payment_destination(&xaddr).expect("resolve");
        assert_eq!(resolved.classic, classic);
        assert_eq!(resolved.destination_tag, Some(42));
        assert_eq!(resolved.xaddress_is_test, Some(false));
    }

    #[test]
    fn resolve_classic_address_has_no_tag() {
        let classic = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        let resolved = resolve_payment_destination(classic).expect("resolve");
        assert_eq!(resolved.classic, classic);
        assert!(resolved.destination_tag.is_none());
        assert!(resolved.xaddress_is_test.is_none());
    }
}
