use std::time::Duration;

use secrecy::ExposeSecret;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;

use tracing::warn;

use crate::action::Action;
use crate::network::Network;
use crate::signing::{self, SigningConfig};

use super::backoff::next_backoff_secs;
use super::client::{RPC_TIMEOUT, RpcClient, is_not_found_error, path_find_snapshot, xrp_to_drops};
use super::types::{
    AccountSetSubmitParams, BookPair, OracleId, PaymentSubmitParams, PollCommand, PollContext,
};

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

struct PollBatchInputs<'a> {
    rpc: &'a RpcClient,
    watch_address: &'a str,
    book_pair: &'a BookPair,
    oracles: &'a [OracleId],
    oracle_pairs: &'a [crate::xrpl::OraclePricePair],
    flare_rpc_url: Option<&'a str>,
    flare_feeds: &'a [String],
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
    } = inputs;
    let dest_amount = book_pair.path_find_destination_amount_preview();
    let (r_srv, r_dunl, r_fee, r_acc, r_book, r_path, r_nfts, r_lines, r_tx) = tokio::join!(
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
        tokio::time::timeout(RPC_TIMEOUT, rpc.account_tx(watch_address, 20, None)),
    );
    let mut any_ok = false;
    macro_rules! dispatch {
        ($result:expr, $ok_action:expr, $label:literal) => {
            match $result {
                Ok(Ok(v)) => {
                    any_ok = true;
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
    dispatch!(
        r_srv,
        |v| Action::XrplServerInfo(Box::new(v)),
        "server_info"
    );
    dispatch!(r_dunl, Action::XrplDunl, "dUNL");
    dispatch!(r_fee, Action::XrplFee, "fee");
    dispatch!(r_acc, |v| Action::XrplAccount(Box::new(v)), "account_info");
    dispatch!(r_book, Action::XrplBookOffers, "book_offers");
    match r_path {
        Ok(Ok(v)) => {
            any_ok = true;
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
    dispatch!(r_nfts, Action::XrplAccountNfts, "account_nfts");
    dispatch!(r_lines, Action::XrplTrustLines, "account_lines");
    match r_tx {
        Ok(Ok(page)) => {
            any_ok = true;
            if let Err(e) = action_tx.send(Action::XrplTxHistory(page.rows, page.marker)) {
                warn!(?e, "action channel closed");
            }
        }
        Ok(Err(e)) => {
            let msg = format!("account_tx: {e}");
            if is_not_found_error(&msg) {
                any_ok = true;
                if let Err(e) = action_tx.send(Action::XrplTxHistory(vec![], None)) {
                    warn!(?e, "action channel closed");
                }
            } else {
                if let Err(e) = action_tx.send(Action::XrplError(msg)) {
                    warn!(?e, "action channel closed");
                }
            }
        }
        Err(_) => {
            if let Err(e) = action_tx.send(Action::XrplError("account_tx: timeout".into())) {
                warn!(?e, "action channel closed");
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
                    any_ok = true;
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
                any_ok = true;
                if let Err(e) = action_tx.send(Action::FlareOraclePrices(prices)) {
                    warn!(?e, "action channel closed (flare ftso)");
                }
            }
            Ok(Ok(_)) | Ok(Err(_)) | Err(_) => {
                // Keep Oracle tab non-blocking when Flare endpoint/feed is unavailable.
            }
        }
    }

    any_ok
}

async fn poll_wallet_overview(
    rpc: &RpcClient,
    seed_address: &str,
    action_tx: &UnboundedSender<Action>,
) -> bool {
    match tokio::time::timeout(RPC_TIMEOUT, rpc.account_overview(seed_address)).await {
        Ok(Ok((acc, txs, marker))) => {
            if let Err(e) = action_tx.send(Action::XrplWalletOverview(acc, txs, marker)) {
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
    if !account_set_params_nonempty(&params) {
        if let Err(e) = action_tx.send(Action::AccountSetSubmitErr(
            "nothing to change — pick a flag and/or fill domain, tick size, transfer rate".into(),
        )) {
            warn!(?e, "action channel closed");
        }
        return;
    }
    if network.is_mainnet() && !params.skip_mainnet_prompt {
        if let Err(e) = action_tx.send(Action::AccountSetSubmitErr(
            "mainnet: restart lazyxrp with --yes to allow AccountSet writes".into(),
        )) {
            warn!(?e, "action channel closed");
        }
        return;
    }
    let signing_config = SigningConfig::prime_seed_source(params.config_seed.clone());
    let Some(seed) = signing_config.seed.as_ref() else {
        if let Err(e) = action_tx.send(Action::AccountSetSubmitErr(
            "no signing seed — set XRPL_SEED or config [xrpl.signing] seed".into(),
        )) {
            warn!(?e, "action channel closed");
        }
        return;
    };
    let wallet = match signing::wallet_from_family_seed(seed.expose_secret(), 0) {
        Ok(w) => w,
        Err(e) => {
            if let Err(e) = action_tx.send(Action::AccountSetSubmitErr(format!("wallet: {e:?}"))) {
                warn!(?e, "action channel closed");
            }
            return;
        }
    };
    let account = wallet.classic_address.clone();

    let tick_size = if params.tick_size.trim().is_empty() {
        None
    } else {
        match params.tick_size.trim().parse::<u32>() {
            Ok(n) => Some(n),
            Err(_) => {
                if let Err(e) = action_tx.send(Action::AccountSetSubmitErr(
                    "tick size: invalid number (use 0 or 3–15)".into(),
                )) {
                    warn!(?e, "action channel closed");
                }
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
                if let Err(e) = action_tx.send(Action::AccountSetSubmitErr(
                    "transfer rate: invalid number".into(),
                )) {
                    warn!(?e, "action channel closed");
                }
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

    let account_info = match tokio::time::timeout(RPC_TIMEOUT, rpc.account_info(&account)).await {
        Ok(Ok(a)) => a,
        Ok(Err(e)) => {
            if let Err(e) =
                action_tx.send(Action::AccountSetSubmitErr(format!("account_info: {e}")))
            {
                warn!(?e, "action channel closed");
            }
            return;
        }
        Err(_) => {
            if let Err(e) =
                action_tx.send(Action::AccountSetSubmitErr("account_info: timeout".into()))
            {
                warn!(?e, "action channel closed");
            }
            return;
        }
    };

    let fee_info = match tokio::time::timeout(RPC_TIMEOUT, rpc.fee()).await {
        Ok(Ok(f)) => f,
        Ok(Err(e)) => {
            if let Err(e) = action_tx.send(Action::AccountSetSubmitErr(format!("fee: {e}"))) {
                warn!(?e, "action channel closed");
            }
            return;
        }
        Err(_) => {
            if let Err(e) = action_tx.send(Action::AccountSetSubmitErr("fee: timeout".into())) {
                warn!(?e, "action channel closed");
            }
            return;
        }
    };

    let server_info = match tokio::time::timeout(RPC_TIMEOUT, rpc.server_info()).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            if let Err(e) = action_tx.send(Action::AccountSetSubmitErr(format!("server_info: {e}")))
            {
                warn!(?e, "action channel closed");
            }
            return;
        }
        Err(_) => {
            if let Err(e) =
                action_tx.send(Action::AccountSetSubmitErr("server_info: timeout".into()))
            {
                warn!(?e, "action channel closed");
            }
            return;
        }
    };
    let last_ledger_sequence = server_info.ledger_index.saturating_add(20);

    let blob = match signing::create_and_sign_account_set(
        seed,
        &account,
        account_info.sequence,
        fee_info.open_ledger_fee_drops,
        last_ledger_sequence,
        set_flag,
        clear_flag,
        domain_hex.as_deref(),
        tick_size,
        transfer_rate,
    ) {
        Ok(b) => b,
        Err(e) => {
            if let Err(e) = action_tx.send(Action::AccountSetSubmitErr(format!("sign: {e}"))) {
                warn!(?e, "action channel closed");
            }
            return;
        }
    };

    match tokio::time::timeout(RPC_TIMEOUT, rpc.submit_signed_tx(&blob)).await {
        Ok(Ok(tx)) => {
            if let Err(e) = action_tx.send(Action::AccountSetSubmitOk(tx.hash.clone())) {
                warn!(?e, "action channel closed");
            }
            if let Err(e) = action_tx.send(Action::RefreshAccount) {
                warn!(?e, "action channel closed");
            }
            if let Err(e) = action_tx.send(Action::RefreshTxHistory) {
                warn!(?e, "action channel closed");
            }
        }
        Ok(Err(e)) => {
            if let Err(e) = action_tx.send(Action::AccountSetSubmitErr(format!("submit: {e}"))) {
                warn!(?e, "action channel closed");
            }
        }
        Err(_) => {
            if let Err(e) = action_tx.send(Action::AccountSetSubmitErr("submit: timeout".into())) {
                warn!(?e, "action channel closed");
            }
        }
    }
}

fn resolve_wallet_payment_destination(trimmed: &str) -> color_eyre::Result<String> {
    use xrpl::core::addresscodec::{
        is_valid_classic_address, is_valid_xaddress, xaddress_to_classic_address,
    };
    if trimmed.is_empty() {
        return Err(color_eyre::eyre::eyre!("destination is empty"));
    }
    if is_valid_classic_address(trimmed) {
        return Ok(trimmed.to_string());
    }
    if is_valid_xaddress(trimmed) {
        let (classic, _, _) = xaddress_to_classic_address(trimmed)
            .map_err(|e| color_eyre::eyre::eyre!("invalid X-address: {e:?}"))?;
        return Ok(classic);
    }
    Err(color_eyre::eyre::eyre!(
        "invalid destination (need classic `r…` or X-address)"
    ))
}

async fn submit_payment_transaction(
    rpc: &RpcClient,
    network: &Network,
    params: PaymentSubmitParams,
    action_tx: &UnboundedSender<Action>,
) {
    if params.amount.trim().is_empty() {
        if let Err(e) = action_tx.send(Action::PaymentSubmitErr(
            "amount is empty — enter XRP to send".into(),
        )) {
            warn!(?e, "action channel closed");
        }
        return;
    }
    let destination_resolved = match resolve_wallet_payment_destination(params.destination.trim()) {
        Ok(d) => d,
        Err(e) => {
            if let Err(e) = action_tx.send(Action::PaymentSubmitErr(format!("{e}"))) {
                warn!(?e, "action channel closed");
            }
            return;
        }
    };
    if network.is_mainnet() && !params.skip_mainnet_prompt {
        if let Err(e) = action_tx.send(Action::PaymentSubmitErr(
            "mainnet: restart lazyxrp with --yes to allow Payment writes".into(),
        )) {
            warn!(?e, "action channel closed");
        }
        return;
    }
    let signing_config = SigningConfig::prime_seed_source(params.config_seed.clone());
    let Some(seed) = signing_config.seed.as_ref() else {
        if let Err(e) = action_tx.send(Action::PaymentSubmitErr(
            "no signing seed — set XRPL_SEED or config [xrpl.signing] seed".into(),
        )) {
            warn!(?e, "action channel closed");
        }
        return;
    };
    let wallet = match signing::wallet_from_family_seed(seed.expose_secret(), 0) {
        Ok(w) => w,
        Err(e) => {
            if let Err(e) = action_tx.send(Action::PaymentSubmitErr(format!("wallet: {e:?}"))) {
                warn!(?e, "action channel closed");
            }
            return;
        }
    };
    let account = wallet.classic_address.clone();
    if account == destination_resolved {
        if let Err(e) = action_tx.send(Action::PaymentSubmitErr(
            "destination matches source account".into(),
        )) {
            warn!(?e, "action channel closed");
        }
        return;
    }

    let amount_drops = match xrp_to_drops(params.amount.trim()) {
        Ok(d) => d,
        Err(e) => {
            if let Err(e) = action_tx.send(Action::PaymentSubmitErr(format!("amount: {e}"))) {
                warn!(?e, "action channel closed");
            }
            return;
        }
    };
    if amount_drops == 0 {
        if let Err(e) = action_tx.send(Action::PaymentSubmitErr(
            "amount must be greater than zero".into(),
        )) {
            warn!(?e, "action channel closed");
        }
        return;
    }

    let account_info = match tokio::time::timeout(RPC_TIMEOUT, rpc.account_info(&account)).await {
        Ok(Ok(a)) => a,
        Ok(Err(e)) => {
            if let Err(e) = action_tx.send(Action::PaymentSubmitErr(format!("account_info: {e}"))) {
                warn!(?e, "action channel closed");
            }
            return;
        }
        Err(_) => {
            if let Err(e) = action_tx.send(Action::PaymentSubmitErr("account_info: timeout".into()))
            {
                warn!(?e, "action channel closed");
            }
            return;
        }
    };

    let balance_drops = xrp_to_drops(&account_info.balance_xrp).unwrap_or(0);
    let fee_info = match tokio::time::timeout(RPC_TIMEOUT, rpc.fee()).await {
        Ok(Ok(f)) => f,
        Ok(Err(e)) => {
            if let Err(e) = action_tx.send(Action::PaymentSubmitErr(format!("fee: {e}"))) {
                warn!(?e, "action channel closed");
            }
            return;
        }
        Err(_) => {
            if let Err(e) = action_tx.send(Action::PaymentSubmitErr("fee: timeout".into())) {
                warn!(?e, "action channel closed");
            }
            return;
        }
    };
    let fee_drops = fee_info.open_ledger_fee_drops;
    if balance_drops < amount_drops + u64::from(fee_drops) {
        let total_need = amount_drops.saturating_add(u64::from(fee_drops));
        if let Err(e) = action_tx.send(Action::PaymentSubmitErr(format!(            "insufficient balance: have {balance_drops} drops, need {total_need} (amount {amount_drops} + fee {fee_drops})"        ))
        ) { warn!(?e, "action channel closed"); }
        return;
    }

    let server_info = match tokio::time::timeout(RPC_TIMEOUT, rpc.server_info()).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            if let Err(e) = action_tx.send(Action::PaymentSubmitErr(format!("server_info: {e}"))) {
                warn!(?e, "action channel closed");
            }
            return;
        }
        Err(_) => {
            if let Err(e) = action_tx.send(Action::PaymentSubmitErr("server_info: timeout".into()))
            {
                warn!(?e, "action channel closed");
            }
            return;
        }
    };
    let last_ledger_sequence = server_info.ledger_index.saturating_add(20);

    let blob = match signing::create_and_sign_payment(
        seed,
        &account,
        &destination_resolved,
        params.amount.trim(),
        params.iou_currency.as_deref(),
        params.iou_issuer.as_deref(),
        account_info.sequence,
        fee_drops,
        last_ledger_sequence,
        network,
    ) {
        Ok(b) => b,
        Err(e) => {
            if let Err(e) = action_tx.send(Action::PaymentSubmitErr(format!("sign: {e}"))) {
                warn!(?e, "action channel closed");
            }
            return;
        }
    };

    match tokio::time::timeout(RPC_TIMEOUT, rpc.submit_signed_tx(&blob)).await {
        Ok(Ok(tx)) => {
            if let Err(e) = action_tx.send(Action::PaymentSubmitOk(tx.hash.clone())) {
                warn!(?e, "action channel closed");
            }
            if let Err(e) = action_tx.send(Action::RefreshAccount) {
                warn!(?e, "action channel closed");
            }
            if let Err(e) = action_tx.send(Action::RefreshTxHistory) {
                warn!(?e, "action channel closed");
            }
        }
        Ok(Err(e)) => {
            if let Err(e) = action_tx.send(Action::PaymentSubmitErr(format!("submit: {e}"))) {
                warn!(?e, "action channel closed");
            }
        }
        Err(_) => {
            if let Err(e) = action_tx.send(Action::PaymentSubmitErr("submit: timeout".into())) {
                warn!(?e, "action channel closed");
            }
        }
    }
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
    let mut tick = tokio::time::interval(poll_interval.max(Duration::from_millis(500)));
    let mut price_tick = tokio::time::interval(Duration::from_secs(90));
    let mut last_poll: Option<std::time::Instant> = None;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tick.tick() => {
                if backoff_secs > 0 {
                    tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                }
                let batch_ok = poll_batch(
                    PollBatchInputs {
                        rpc: &rpc,
                        watch_address: &watch_address,
                        book_pair: &book_pair,
                        oracles: &oracles,
                        oracle_pairs: &oracle_pairs,
                        flare_rpc_url: flare_rpc_url.as_deref(),
                        flare_feeds: &flare_feeds,
                    },
                    &action_tx,
                )
                .await;
                if let Some(ref addr) = seed_address {
                    poll_wallet_overview(&rpc, addr, &action_tx).await;
                }
                if batch_ok {
                    backoff_secs = 0;
                } else {
                    backoff_secs = next_backoff_secs(backoff_secs);
                }
                last_poll = Some(std::time::Instant::now());
            }
            Some(()) = poll_trigger_rx.recv() => {
                // Coalesce rapid trigger bursts
                while poll_trigger_rx.try_recv().is_ok() {}
                if let Some(last) = last_poll
                    && last.elapsed() < MIN_POLL_INTERVAL
                {
                    continue;
                }
                if backoff_secs > 0 {
                    tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                }
                let batch_ok = poll_batch(
                    PollBatchInputs {
                        rpc: &rpc,
                        watch_address: &watch_address,
                        book_pair: &book_pair,
                        oracles: &oracles,
                        oracle_pairs: &oracle_pairs,
                        flare_rpc_url: flare_rpc_url.as_deref(),
                        flare_feeds: &flare_feeds,
                    },
                    &action_tx,
                )
                .await;
                if let Some(ref addr) = seed_address {
                    poll_wallet_overview(&rpc, addr, &action_tx).await;
                }
                if batch_ok {
                    backoff_secs = 0;
                } else {
                    backoff_secs = next_backoff_secs(backoff_secs);
                }
                last_poll = Some(std::time::Instant::now());
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
                    PollCommand::TxHistory => dispatch_timed(
                        &action_tx,
                        "account_tx",
                        tokio::time::timeout(RPC_TIMEOUT, rpc.account_tx(&watch_address, 20, None)).await,
                        |page: crate::xrpl::types::AccountTxPage| Action::XrplTxHistory(page.rows, page.marker),
                    ),
                    PollCommand::TxHistoryMore(marker) => dispatch_timed(
                        &action_tx,
                        "account_tx",
                        tokio::time::timeout(RPC_TIMEOUT, rpc.account_tx(&watch_address, 20, marker)).await,
                        |page: crate::xrpl::types::AccountTxPage| Action::XrplTxHistoryAppend(page.rows, page.marker),
                    ),
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
                    _ => {}
                }
            }
        }
    }
}
