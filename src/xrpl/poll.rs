use std::time::{Duration, Instant};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use tracing::warn;

use crate::action::Action;
use crate::network::Network;

use super::client::{
    RPC_TIMEOUT, RpcClient, empty_account_tx_page_on_not_found, path_find_snapshot,
};
use super::submit::*;
use super::types::{BookPair, OracleId, PollCommand, PollContext};
use super::util::next_backoff_secs;

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
pub(crate) const TAB_ACCOUNT: usize = 1;
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

#[must_use]
pub(crate) fn should_poll_account_panels(active_tab: usize) -> bool {
    active_tab == TAB_ACCOUNT
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
    /// XRPL account whose Smart Account is read: the signing account, else the watched one.
    smart_account_owner: &'a str,
    flare_fassets_execute: bool,
    flare_evm_key_set: bool,
    /// When a signing seed is configured, `poll_wallet_overview` fetches `account_tx` once.
    skip_account_tx: bool,
    active_tab: usize,
}

/// `amm_info` for the configured book pair (`Market` tab AMM panel). No pool for the pair
/// (`actNotFound`) is `Ok(None)`, not an error: it must not flag the status bar every poll.
async fn fetch_amm_info(
    rpc: &RpcClient,
    book_pair: &BookPair,
) -> Result<color_eyre::Result<Option<crate::xrpl::types::AmmSummary>>, tokio::time::error::Elapsed>
{
    let res = tokio::time::timeout(
        RPC_TIMEOUT,
        rpc.amm_info(
            book_pair.gets_currency(),
            book_pair.gets_issuer(),
            book_pair.pays_currency(),
            book_pair.pays_issuer(),
        ),
    )
    .await?;
    Ok(match res {
        Ok(s) => Ok(Some(s)),
        Err(e) if super::parse::is_not_found_error(&e.to_string()) => Ok(None),
        Err(e) => Err(e),
    })
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

async fn poll_batch(
    inputs: PollBatchInputs<'_>,
    action_tx: &UnboundedSender<Action>,
    flare_backoff_secs: &mut u64,
    flare_backoff_until: &mut Option<Instant>,
) -> bool {
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
        smart_account_owner,
        flare_fassets_execute,
        flare_evm_key_set,
        skip_account_tx,
        active_tab,
    } = inputs;
    let skip_market = !should_poll_market_book(active_tab);
    let skip_account_tx = skip_account_tx || !should_poll_account_panels(active_tab);
    let skip_assets = !should_poll_asset_panels(active_tab);
    let dest_amount = (!skip_market).then(|| book_pair.path_find_destination_amount_preview());
    let (
        server_info_result,
        dunl_result,
        fee_result,
        account_info_result,
        book_offers_result,
        path_find_result,
        amm_info_result,
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
                    rpc.ripple_path_find(watch_address, watch_address, dest_amount, None),
                )
                .await,
            )
        },
        async {
            if skip_market {
                None
            } else {
                Some(fetch_amm_info(rpc, book_pair).await)
            }
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
            any_rpc_succeeded |= send_timed(action_tx, $label, $result, $ok_action)
        };
    }
    send_rpc_outcome!(
        server_info_result,
        |v| Action::XrplServerInfo(Box::new(v)),
        "server_info"
    );
    // dUNL is a different host (unl.xrplf.org): its outcome must not key the
    // XRPL RPC backoff below — excluded from `any_rpc_succeeded`.
    dispatch_timed(action_tx, "dUNL", dunl_result, Action::XrplDunl);
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
        send_rpc_outcome!(
            path_find_result,
            |v| Action::XrplPathFind(path_find_snapshot(&v, &book_pair.quote)),
            "ripple_path_find"
        );
    }
    if let Some(amm_info_result) = amm_info_result {
        send_rpc_outcome!(
            amm_info_result,
            |v| Action::XrplAmmInfo(v.map(Box::new)),
            "amm_info"
        );
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
                send_account_tx_action(action_tx, action);
            }
            Err(_) => {
                send_account_tx_action(action_tx, Action::XrplError("account_tx: timeout".into()))
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

    // FTSO + FXRP AssetManager are shown on Overview; skip when another tab is
    // focused or a flare-specific backoff window is open. Failures stay
    // non-fatal (warn only) so XRPL polling continues; flare health never
    // feeds the XRPL backoff decision.
    if active_tab == 0
        && flare_display != crate::config::FlareDisplay::Off
        && !is_backoff_active(*flare_backoff_until)
        && let Some(flare_rpc) = flare_rpc_url
    {
        // Concurrent: worst-case arm occupancy is one RPC_TIMEOUT, not four.
        let (ftso_result, mint_result, smart_result, wallet_result) = tokio::join!(
            tokio::time::timeout(
                RPC_TIMEOUT,
                crate::flare::fetch_ftso_prices(flare_rpc, flare_feeds),
            ),
            tokio::time::timeout(
                RPC_TIMEOUT,
                crate::flare::fetch_fxrp_direct_mint_info(flare_rpc),
            ),
            async {
                if smart_account_owner.trim().is_empty() {
                    return None;
                }
                Some(
                    tokio::time::timeout(
                        RPC_TIMEOUT,
                        crate::flare::fetch_smart_account_info(flare_rpc, smart_account_owner),
                    )
                    .await,
                )
            },
            async {
                match flare_wallet_address {
                    Some(wallet_addr) => Some(
                        tokio::time::timeout(
                            RPC_TIMEOUT,
                            crate::flare::fetch_flare_wallet_balance(
                                flare_rpc,
                                wallet_addr,
                                flare_fassets_execute,
                                flare_evm_key_set,
                            ),
                        )
                        .await,
                    ),
                    None => None,
                }
            },
        );

        let mut flare_ok = false;
        match ftso_result {
            Ok(Ok(prices)) if !prices.is_empty() => {
                flare_ok = true;
                if let Err(e) = action_tx.send(Action::FlareOraclePrices(prices)) {
                    warn!(?e, "action channel closed (flare ftso)");
                }
            }
            Ok(Ok(_)) => warn!("flare ftso fetch returned no feeds (non-fatal)"),
            Ok(Err(e)) => warn!(?e, "flare ftso fetch failed (non-fatal)"),
            Err(_) => warn!("flare ftso fetch timed out (non-fatal)"),
        }
        match mint_result {
            Ok(Ok(info)) => {
                flare_ok = true;
                if let Err(e) = action_tx.send(Action::FxrpDirectMintInfo(Box::new(info))) {
                    warn!(?e, "action channel closed (fxrp direct mint)");
                }
            }
            Ok(Err(e)) => warn!(?e, "fxrp direct mint fetch failed (non-fatal)"),
            Err(_) => warn!("fxrp direct mint fetch timed out (non-fatal)"),
        }
        match smart_result {
            Some(Ok(Ok(info))) => {
                flare_ok = true;
                if let Err(e) = action_tx.send(Action::SmartAccountInfo(Box::new(info))) {
                    warn!(?e, "action channel closed (smart account)");
                }
            }
            Some(Ok(Err(e))) => warn!(?e, "smart account fetch failed (non-fatal)"),
            Some(Err(_)) => warn!("smart account fetch timed out (non-fatal)"),
            None => {}
        }
        match wallet_result {
            Some(Ok(Ok(summary))) => {
                flare_ok = true;
                if let Err(e) = action_tx.send(Action::FlareWalletBalance(Box::new(summary))) {
                    warn!(?e, "action channel closed (flare wallet)");
                }
            }
            Some(Ok(Err(e))) => warn!(?e, "flare wallet fetch failed (non-fatal)"),
            Some(Err(_)) => warn!("flare wallet fetch timed out (non-fatal)"),
            None => {}
        }

        // Flare-only cooldown: escalate on total failure, reset on any success.
        // Independent state from the XRPL backoff above (same schedule).
        update_backoff(flare_ok, flare_backoff_secs, flare_backoff_until);
        // Health visibility: a total flare failure arms the status-bar flare
        // dot; the next successful flare fetch clears it.
        if !flare_ok {
            send_action(action_tx, Action::FlareFetchFailed);
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

/// Sends the outcome of a timed RPC as an action; returns whether it succeeded.
fn send_timed<T, F>(
    action_tx: &UnboundedSender<Action>,
    label: &str,
    result: Result<color_eyre::Result<T>, tokio::time::error::Elapsed>,
    ok_action: F,
) -> bool
where
    F: FnOnce(T) -> Action,
{
    let (action, ok) = match result {
        Ok(Ok(value)) => (ok_action(value), true),
        Ok(Err(e)) => (Action::XrplError(format!("{label}: {e}")), false),
        Err(_) => (Action::XrplError(format!("{label}: timeout")), false),
    };
    send_account_tx_action(action_tx, action);
    ok
}

fn dispatch_timed<T, F>(
    action_tx: &UnboundedSender<Action>,
    label: &str,
    result: Result<color_eyre::Result<T>, tokio::time::error::Elapsed>,
    ok_action: F,
) where
    F: FnOnce(T) -> Action,
{
    send_timed(action_tx, label, result, ok_action);
}

#[allow(clippy::too_many_arguments)]
async fn run_scheduled_poll<'a>(
    rpc: &'a RpcClient,
    tab_watch: &tokio::sync::watch::Receiver<usize>,
    build_batch: &impl Fn(usize) -> PollBatchInputs<'a>,
    seed_address: Option<&str>,
    action_tx: &UnboundedSender<Action>,
    backoff_secs: &mut u64,
    backoff_until: &mut Option<Instant>,
    flare_backoff_secs: &mut u64,
    flare_backoff_until: &mut Option<Instant>,
) -> Option<Instant> {
    let active_tab = *tab_watch.borrow();
    Some(
        execute_scheduled_poll(
            rpc,
            build_batch(active_tab),
            seed_address,
            active_tab,
            action_tx,
            backoff_secs,
            backoff_until,
            flare_backoff_secs,
            flare_backoff_until,
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

#[allow(clippy::too_many_arguments)]
async fn execute_scheduled_poll(
    rpc: &RpcClient,
    inputs: PollBatchInputs<'_>,
    seed_address: Option<&str>,
    active_tab: usize,
    action_tx: &UnboundedSender<Action>,
    backoff_secs: &mut u64,
    backoff_until: &mut Option<Instant>,
    flare_backoff_secs: &mut u64,
    flare_backoff_until: &mut Option<Instant>,
) -> Instant {
    // Overlap wallet overview with the main batch (same RpcClient, independent RPCs).
    let wallet_fut = async {
        match seed_address {
            Some(addr) if should_poll_account_panels(active_tab) => {
                poll_wallet_overview(rpc, addr, action_tx).await
            }
            _ => false,
        }
    };
    let (batch_succeeded, wallet_overview_succeeded) = tokio::join!(
        poll_batch(inputs, action_tx, flare_backoff_secs, flare_backoff_until),
        wallet_fut
    );
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

/// A write carries the XRPL network it was composed on. A network switch rebinds the RPC client
/// while the command may still be queued, so a write dequeued after one would be signed for a
/// different network than the user saw: refuse it with the command's own error (nothing signed).
fn stale_write_refusal(cmd: &PollCommand, active: Network) -> Option<Action> {
    let (composed, err): (Network, fn(String) -> Action) = match cmd {
        PollCommand::AccountSetSubmit(p) => (p.network, Action::AccountSetSubmitErr),
        PollCommand::PaymentSubmit(p) => (p.network, Action::PaymentSubmitErr),
        PollCommand::FxrpDirectMintPayment(p) => {
            (p.network, Action::FxrpDirectMintPaymentSubmitErr)
        }
        PollCommand::FxrpExecuteDirectMint(p) => {
            (p.network, Action::FxrpExecuteDirectMintSubmitErr)
        }
        PollCommand::SmartAccountInstruction(p) => {
            (p.network, Action::SmartAccountInstructionSubmitErr)
        }
        PollCommand::SetRegularKeySubmit(p) => (p.network, Action::SetRegularKeySubmitErr),
        PollCommand::EscrowCreateSubmit(p) => (p.network, Action::EscrowCreateSubmitErr),
        PollCommand::EscrowCloseSubmit(p) => (p.network, Action::EscrowCloseSubmitErr),
        PollCommand::AmmDepositSubmit(p) => (p.network, Action::AmmSubmitErr),
        PollCommand::AmmWithdrawSubmit(p) => (p.network, Action::AmmSubmitErr),
        PollCommand::AccountDeleteSubmit(p) => (p.network, Action::AccountDeleteSubmitErr),
        PollCommand::GenericTx(p) => (p.network, Action::GenericTxSubmitErr),
        PollCommand::OfferCreateSubmit(p) => (p.network, Action::OfferCreateSubmitErr),
        PollCommand::OfferCancelSubmit(p) => (p.network, Action::OfferCancelSubmitErr),
        PollCommand::TrustSetSubmit(p) => (p.network, Action::TrustSetSubmitErr),
        _ => return None,
    };
    (composed != active).then(|| {
        err(format!(
            "network changed from {} to {} since this was composed; nothing was signed, reopen the form",
            composed.display_name(),
            active.display_name()
        ))
    })
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
        flare_network,
        flare_expected_core_vault,
        flare_expected_operator,
        flare_display,
        flare_evm_key_env,
        flare_evm_key,
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
    let mut flare_backoff_secs: u64 = 0;
    let mut flare_backoff_until: Option<Instant> = None;
    let mut tick = tokio::time::interval(poll_interval.max(Duration::from_millis(500)));
    let mut price_tick = tokio::time::interval(Duration::from_secs(90));
    // A slow poll must push the next one back, not trigger a catch-up burst of RPC calls.
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    price_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last_poll: Option<Instant> = None;
    // Shared by the interval and trigger arms: one scheduled poll against the live client.
    macro_rules! scheduled_poll {
        () => {{
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
                    smart_account_owner: seed_address.as_deref().unwrap_or(&watch_address),
                    flare_fassets_execute,
                    flare_evm_key_set: flare_evm_key.is_some(),
                    skip_account_tx: seed_address.is_some(),
                    active_tab,
                },
                seed_address.as_deref(),
                &action_tx,
                &mut backoff_secs,
                &mut backoff_until,
                &mut flare_backoff_secs,
                &mut flare_backoff_until,
            )
            .await;
        }};
    }
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
                scheduled_poll!();
            }
            Some(()) = poll_trigger_rx.recv() => {
                drain_poll_trigger_burst(&mut poll_trigger_rx);
                if is_backoff_active(backoff_until) || should_skip_poll_trigger(last_poll) {
                    continue;
                }
                scheduled_poll!();
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
                if let Some(refusal) = stale_write_refusal(&cmd, network) {
                    send_action(&action_tx, refusal);
                    continue;
                }
                macro_rules! submit {
                    ($submit_fn:ident, $params:expr $(, $extra:expr)*) => {{
                        let _guard = submit_lock.lock().await;
                        $submit_fn(&*rpc_cell.read().await, &network, $params, $($extra,)* &action_tx, signing_credential).await
                    }};
                }
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
                        submit!(submit_account_set_transaction, params);
                    }
                    PollCommand::PaymentSubmit(params) => {
                        submit!(submit_payment_transaction, params);
                    }
                    PollCommand::FxrpDirectMintPayment(params) => {
                        let _guard = submit_lock.lock().await;
                        let fresh = match flare_rpc_url.as_deref() {
                            Some(url) => tokio::time::timeout(
                                RPC_TIMEOUT,
                                crate::flare::verify_direct_mint_terms(url, &params),
                            )
                            .await
                            .unwrap_or_else(|_| Err("Flare re-check before signing timed out".into())),
                            None => Err("flare RPC URL not configured".into()),
                        };
                        match fresh {
                            Err(e) => {
                                send_action(&action_tx, Action::FxrpDirectMintPaymentSubmitErr(e));
                            }
                            Ok(()) => {
                                submit_fxrp_direct_mint_payment(
                                    &*rpc_cell.read().await,
                                    &network,
                                    flare_network,
                                    params,
                                    flare_expected_core_vault.as_deref(),
                                    &action_tx,
                                    signing_credential,
                                )
                                .await;
                            }
                        }
                    }
                    PollCommand::SmartAccountInstruction(params) => {
                        let _guard = submit_lock.lock().await;
                        let owner = signing_credential.and_then(|c| c.address().ok());
                        let fresh = match (flare_rpc_url.as_deref(), owner) {
                            (Some(url), Some(owner)) => tokio::time::timeout(
                                RPC_TIMEOUT,
                                crate::flare::verify_smart_account_terms(url, &owner, &params),
                            )
                            .await
                            .unwrap_or_else(|_| Err("Flare re-check before signing timed out".into())),
                            _ => Err("flare RPC URL or signing key not configured".into()),
                        };
                        match fresh {
                            Err(e) => {
                                send_action(&action_tx, Action::SmartAccountInstructionSubmitErr(e));
                            }
                            Ok(()) => {
                                submit_smart_account_instruction(
                                    &*rpc_cell.read().await,
                                    &network,
                                    flare_network,
                                    params,
                                    flare_expected_operator.as_deref(),
                                    &action_tx,
                                    signing_credential,
                                )
                                .await;
                            }
                        }
                    }
                    PollCommand::FxrpExecuteDirectMint(params) => {
                        submit_fxrp_execute_direct_mint(
                            flare_rpc_url.as_deref(),
                            flare_fassets_execute,
                            flare_network,
                            &flare_evm_key_env,
                            flare_evm_key.as_ref(),
                            params,
                            &action_tx,
                        ).await;
                    }
                    PollCommand::SetRegularKeySubmit(params) => {
                        submit!(submit_set_regular_key_transaction, params);
                    }
                    PollCommand::OfferCreateSubmit(params) => {
                        submit!(submit_offer_create_transaction, params);
                    }
                    PollCommand::AccountOffers => match seed_address.as_deref() {
                        Some(addr) => dispatch_timed(
                            &action_tx,
                            "account_offers",
                            tokio::time::timeout(RPC_TIMEOUT, rpc_cell.read().await.account_offers(addr)).await,
                            Action::XrplAccountOffers,
                        ),
                        None => {
                            send_action(
                                &action_tx,
                                Action::XrplError("account_offers: no signing credential".into()),
                            );
                        }
                    },
                    PollCommand::OfferCancelSubmit(params) => {
                        submit!(submit_offer_cancel_transaction, params);
                    }
                    PollCommand::PaymentQuote(params) => {
                        let action = match seed_address.as_deref() {
                            None => Action::PaymentQuoteErr("no signing credential".into()),
                            Some(source) => match quote_payment(&*rpc_cell.read().await, source, params).await {
                                Ok(spec) => Action::PaymentQuoteOk(spec),
                                Err(e) => Action::PaymentQuoteErr(e),
                            },
                        };
                        send_action(&action_tx, action);
                    }
                    PollCommand::TrustSetSubmit(params) => {
                        submit!(submit_trust_set_transaction, params);
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
                    PollCommand::EscrowCreateSubmit(params) => {
                        submit!(submit_escrow_create_transaction, params);
                    }
                    PollCommand::AccountEscrows => match seed_address.as_deref() {
                        Some(addr) => dispatch_timed(
                            &action_tx,
                            "account_escrows",
                            tokio::time::timeout(RPC_TIMEOUT, rpc_cell.read().await.account_escrows(addr)).await,
                            Action::XrplAccountEscrows,
                        ),
                        None => {
                            send_action(
                                &action_tx,
                                Action::XrplError("account_escrows: no signing credential".into()),
                            );
                        }
                    },
                    PollCommand::EscrowCloseSubmit(params) => {
                        submit!(submit_escrow_close_transaction, params);
                    }
                    PollCommand::AmmDepositSubmit(params) => {
                        submit!(submit_amm_deposit_transaction, params);
                    }
                    PollCommand::AmmWithdrawSubmit(params) => {
                        submit!(submit_amm_withdraw_transaction, params);
                    }
                    PollCommand::AccountDeleteSubmit(params) => {
                        submit!(submit_account_delete_transaction, params);
                    }
                    PollCommand::GenericTx(params) => {
                        submit!(submit_generic_transaction, params);
                    }
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

    /// TC-362: a write composed on one network is refused (nothing signed) when another is
    /// active at dequeue; the refusal is the command's own error action. Reads pass.
    #[test]
    fn stale_write_refusal_on_network_switch() {
        use crate::xrpl::{FxrpExecuteDirectMintParams, OfferCancelSubmitParams};
        let cancel = |network| {
            PollCommand::OfferCancelSubmit(OfferCancelSubmitParams {
                offer_sequence: 1,
                skip_mainnet_prompt: true,
                network,
            })
        };
        assert!(stale_write_refusal(&cancel(Network::Testnet), Network::Testnet).is_none());
        assert!(matches!(
            stale_write_refusal(&cancel(Network::Testnet), Network::Mainnet),
            Some(Action::OfferCancelSubmitErr(m)) if m.contains("TESTNET") && m.contains("MAINNET")
        ));
        let execute = PollCommand::FxrpExecuteDirectMint(FxrpExecuteDirectMintParams {
            proof_json: "{}".into(),
            skip_mainnet_prompt: true,
            network: Network::Mainnet,
        });
        assert!(matches!(
            stale_write_refusal(&execute, Network::Testnet),
            Some(Action::FxrpExecuteDirectMintSubmitErr(_))
        ));
        assert!(stale_write_refusal(&PollCommand::Account, Network::Testnet).is_none());
    }

    /// Poll gates are tab-scoped: each predicate polls only on its own tab.
    #[test]
    fn should_poll_tab_gates() {
        let cases = [
            (
                "account panels poll only on the account tab",
                should_poll_account_panels as fn(usize) -> bool,
                TAB_ACCOUNT,
            ),
            (
                "market book polls only on the market tab",
                should_poll_market_book as _,
                TAB_MARKET,
            ),
            (
                "asset panels poll only on the assets tab",
                should_poll_asset_panels as _,
                TAB_ASSETS,
            ),
        ];
        for (name, poll, on_tab) in cases {
            for tab in [0, TAB_ACCOUNT, TAB_MARKET, TAB_ASSETS] {
                assert_eq!(poll(tab), tab == on_tab, "{name}: tab {tab}");
            }
        }
    }

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
    fn should_skip_poll_trigger_after_min_interval() {
        let cases: [(&str, Option<Instant>, bool); 3] = [
            (
                "recent poll within the min interval is skipped",
                Some(Instant::now()),
                true,
            ),
            ("no previous poll is never skipped", None, false),
            (
                "poll older than the min interval is allowed",
                Some(Instant::now() - MIN_POLL_INTERVAL - Duration::from_millis(1)),
                false,
            ),
        ];
        for (name, last, expected) in cases {
            assert_eq!(should_skip_poll_trigger(last), expected, "{name}");
        }
    }

    /// deadline matrix: no window is inactive, a future deadline is active, a
    /// past deadline has already expired.
    #[test]
    fn is_backoff_active_deadline_matrix() {
        let cases: [(&str, Option<Instant>, bool); 3] = [
            ("absent window is inactive", None, false),
            (
                "future deadline is active",
                Some(Instant::now() + Duration::from_secs(30)),
                true,
            ),
            (
                "past deadline is expired",
                Some(Instant::now() - Duration::from_millis(1)),
                false,
            ),
        ];
        for (name, deadline, expected) in cases {
            assert_eq!(is_backoff_active(deadline), expected, "{name}");
        }
    }

    /// TC-089/TC-144: not-found → empty history (I-7); action follows append flag
    #[test]
    fn action_from_account_tx_result_maps_all_outcomes() {
        use crate::xrpl::types::AccountTxPage;

        #[derive(Debug)]
        enum Expect {
            History {
                append: bool,
                marker: Option<serde_json::Value>,
            },
            Error(&'static str),
        }
        let page = AccountTxPage {
            rows: Vec::new(),
            marker: Some(serde_json::json!({"ledger_index": 1})),
        };
        let marker_expected = page.marker.clone();
        let cases: Vec<(
            &str,
            Result<AccountTxPage, color_eyre::Report>,
            bool,
            Expect,
        )> = vec![
            (
                "not-found without append yields an empty history",
                Err(color_eyre::eyre::eyre!("actNotFound")),
                false,
                Expect::History {
                    append: false,
                    marker: None,
                },
            ),
            (
                "non-not-found error without append is an XrplError",
                Err(color_eyre::eyre::eyre!("timeout")),
                false,
                Expect::Error("account_tx"),
            ),
            (
                "success without append follows the flag as XrplTxHistory",
                Ok(page.clone()),
                false,
                Expect::History {
                    append: false,
                    marker: marker_expected.clone(),
                },
            ),
            (
                "success with append follows the flag as XrplTxHistoryAppend",
                Ok(page.clone()),
                true,
                Expect::History {
                    append: true,
                    marker: marker_expected,
                },
            ),
            (
                "not-found with append yields an empty append",
                Err(color_eyre::eyre::eyre!("actNotFound")),
                true,
                Expect::History {
                    append: true,
                    marker: None,
                },
            ),
        ];
        for (name, result, append, expected) in cases {
            let action = action_from_account_tx_result(result, append);
            match (action, expected) {
                (
                    Action::XrplTxHistory(rows, marker),
                    Expect::History {
                        append: false,
                        marker: want,
                    },
                ) => {
                    assert!(rows.is_empty(), "{name}");
                    assert_eq!(marker, want, "{name}");
                }
                (
                    Action::XrplTxHistoryAppend(rows, marker),
                    Expect::History {
                        append: true,
                        marker: want,
                    },
                ) => {
                    assert!(rows.is_empty(), "{name}");
                    assert_eq!(marker, want, "{name}");
                }
                (Action::XrplError(msg), Expect::Error(needle)) => {
                    assert!(msg.contains(needle), "{name}: {msg}");
                }
                (other, expected) => {
                    panic!("{name}: unexpected action {other:?} for expectation {expected:?}")
                }
            }
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
        for _ in 0..10 {
            update_backoff(false, &mut secs, &mut until);
        }
        assert_eq!(secs, 60, "backoff must cap at 60 s");
        update_backoff(true, &mut secs, &mut until);
        assert_eq!(secs, 0, "success must reset backoff to the floor");
        assert!(until.is_none(), "success must clear the backoff window");
    }

    /// The Market-tab AMM read maps a pool to `XrplAmmInfo(Some)`, and "no pool" (`actNotFound`)
    /// to `XrplAmmInfo(None)` — never `XrplError`, which would flag the status bar every poll.
    #[tokio::test]
    async fn fetch_amm_info_maps_pool_and_no_pool() {
        use crate::test_support::rpc_mock;
        use serde_json::{Value, json};
        use std::sync::atomic::{AtomicBool, Ordering};
        use tokio::net::TcpListener;

        static SERVED_POOL: AtomicBool = AtomicBool::new(false);
        fn result_for(method: &str) -> Value {
            assert_eq!(method, "amm_info");
            if SERVED_POOL.swap(true, Ordering::SeqCst) {
                return json!({"error": "actNotFound", "error_code": 19,
                    "error_message": "Account not found.", "status": "error"});
            }
            json!({"amm": {
                "amount": "5000000",
                "amount2": {"currency": "USD", "issuer": "rIss", "value": "10"},
                "lp_token": {"currency": "03AB", "issuer": "rAmm", "value": "7"},
                "trading_fee": 500
            }})
        }
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(rpc_mock::serve(listener, 2, result_for));
        let rpc = RpcClient::connect(&url).expect("rpc client");
        let pair = BookPair {
            base: "XRP".into(),
            quote: "rIss".into(),
            quote_code: "USD".into(),
            issuer: "rIss".into(),
            limit: 10,
        };
        let (tx, mut rx) = mpsc::unbounded_channel();
        for _ in 0..2 {
            assert!(send_timed(
                &tx,
                "amm_info",
                fetch_amm_info(&rpc, &pair).await,
                |v| Action::XrplAmmInfo(v.map(Box::new))
            ));
        }
        let _ = server.await;
        match rx.try_recv() {
            Ok(Action::XrplAmmInfo(Some(s))) => {
                assert_eq!(s.trading_fee, 500);
                assert_eq!(s.lp_token, "7 03AB");
                assert_eq!(s.pool1, "5.000000");
                assert_eq!(s.pool2, "10 USD");
            }
            other => panic!("expected XrplAmmInfo(Some), got {other:?}"),
        }
        let no_pool = rx.try_recv();
        assert!(
            matches!(no_pool, Ok(Action::XrplAmmInfo(None))),
            "{no_pool:?}"
        );
    }
}
