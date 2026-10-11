use crate::cli::Cmd;
use crate::network::Network;
use crate::signing::{self, SigningCredential, prompt_production_confirmation};

use super::client::{RpcClient, xrp_to_drops};

fn validate_cmd_account_and_issuer_addresses(cmd: &Cmd) -> color_eyre::Result<()> {
    use crate::signing::require_classic_address_shape;

    match cmd {
        Cmd::Account { address }
        | Cmd::Nfts { address }
        | Cmd::Lines { address }
        | Cmd::TxHistory { address, .. }
        | Cmd::AccountStatus { address }
        | Cmd::Offers { address } => {
            require_classic_address_shape("account", address)?;
        }
        Cmd::Summary { account } => {
            if let Some(account) = account.as_deref().filter(|s| !s.is_empty()) {
                require_classic_address_shape("account", account)?;
            }
        }
        Cmd::Book { issuer, .. } => {
            if let Some(issuer) = issuer.as_deref().filter(|s| !s.is_empty()) {
                require_classic_address_shape("issuer", issuer)?;
            }
        }
        Cmd::Amm {
            issuer1, issuer2, ..
        } => {
            if let Some(issuer) = issuer1.as_deref().filter(|s| !s.is_empty()) {
                require_classic_address_shape("issuer", issuer)?;
            }
            if let Some(issuer) = issuer2.as_deref().filter(|s| !s.is_empty()) {
                require_classic_address_shape("issuer", issuer)?;
            }
        }
        Cmd::OfferCreate {
            taker_gets,
            taker_pays,
        } => crate::signing::validate_offer_specs(taker_gets, taker_pays)?,
        Cmd::AmmDeposit { amount, amount2 } => {
            crate::signing::validate_amm_deposit_specs(amount, amount2)?
        }
        Cmd::AmmWithdraw { asset, asset2, lp } => crate::signing::validate_amm_withdraw_input(
            asset,
            asset2,
            lp.as_deref().unwrap_or_default(),
        )?,
        Cmd::AccountDelete { destination } => {
            super::address::resolve_payment_destination(destination.trim())?;
        }
        Cmd::Faucet {
            address: Some(address),
        } => {
            require_classic_address_shape("account", address)?;
        }
        Cmd::Info | Cmd::Send { .. } | Cmd::OfferCancel { .. } | Cmd::Faucet { address: None } => {}
    }
    Ok(())
}

fn cli_wallet(credential: Option<&SigningCredential>) -> color_eyre::Result<xrpl::wallet::Wallet> {
    credential
        .ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "No signing credential: set XRPL_SEED or XRPL_MNEMONIC, configure [xrpl.signing] seed or mnemonic, or use --seed/--mnemonic."
            )
        })?
        .wallet()
}

/// Public faucet endpoint; production networks are refused (no faucet, and never "fund" mainnet).
pub(crate) fn faucet_url(network: &Network) -> color_eyre::Result<&'static str> {
    match network {
        Network::Testnet => Ok("https://faucet.altnet.rippletest.net/accounts"),
        Network::Devnet => Ok("https://faucet.devnet.rippletest.net/accounts"),
        _ => color_eyre::eyre::bail!(
            "faucet is available on testnet/devnet only (use --network testnet)"
        ),
    }
}

#[derive(Debug)]
pub(crate) struct FaucetFunded {
    pub amount_xrp: String,
    pub tx_hash: Option<String>,
}

/// POST `{destination}` to the faucet; the response must name the same address.
pub(crate) async fn fund_from_faucet(
    url: &str,
    destination: &str,
) -> color_eyre::Result<FaucetFunded> {
    use super::util::sanitize_remote;

    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?
        .post(url)
        .json(&serde_json::json!({
            "destination": destination,
            "userAgent": concat!("lazyxrp/", env!("CARGO_PKG_VERSION")),
        }))
        .send()
        .await?;
    let status = response.status();
    let text = super::client::read_response_text_capped(response, "faucet").await?;
    if !status.is_success() {
        color_eyre::eyre::bail!("faucet HTTP {status}: {}", sanitize_remote(&text));
    }
    let body: serde_json::Value = serde_json::from_str(&text)?;
    let funded = body["account"]["classicAddress"]
        .as_str()
        .unwrap_or_default();
    if funded != destination {
        color_eyre::eyre::bail!(
            "faucet funded {:?}, not {destination}",
            sanitize_remote(funded)
        );
    }
    Ok(FaucetFunded {
        amount_xrp: body["amount"]
            .as_f64()
            .map_or_else(|| "?".into(), |a| a.to_string()),
        tx_hash: body["transactionHash"].as_str().map(sanitize_remote),
    })
}

const WAIT_POLL: std::time::Duration = std::time::Duration::from_secs(1);
/// LLS window is ~20 ledgers (≈ 80 s); this only guards a node that never advances.
const WAIT_LIMIT: std::time::Duration = std::time::Duration::from_secs(300);

/// Poll `tx` until validated: `tes*` → `Ok(code)`, anything else → `Err` naming the code.
/// Still missing once the validated ledger passes `last_ledger_sequence` → one more `tx`
/// check (it may have validated between the two reads), then `Err` (expired; it can never be
/// included). Still pending after `limit` → `Err` (stuck node or no `validated_ledger`).
/// Transport errors from `server_info` propagate.
pub(crate) async fn wait_validated(
    rpc: &RpcClient,
    hash: &str,
    last_ledger_sequence: u32,
    poll: std::time::Duration,
    limit: std::time::Duration,
) -> color_eyre::Result<String> {
    use super::util::sanitize_remote;

    let deadline = std::time::Instant::now() + limit;
    let mut past_lls = None;
    loop {
        // `txnNotFound` (not yet in a ledger) is an RPC error: treat like pending.
        if let Ok(tx) = rpc.tx(hash).await
            && tx["result"]["validated"] == true
        {
            let code = sanitize_remote(
                tx["result"]["meta"]["TransactionResult"]
                    .as_str()
                    .unwrap_or("?"),
            );
            if code.starts_with("tes") {
                return Ok(code);
            }
            color_eyre::eyre::bail!("transaction {hash} validated with {code}");
        }
        if let Some(validated) = past_lls {
            color_eyre::eyre::bail!(
                "transaction {hash} expired: not validated by LastLedgerSequence {last_ledger_sequence} (validated ledger {validated})"
            );
        }
        let validated = rpc.server_info().await?.ledger_index;
        if validated > last_ledger_sequence {
            past_lls = Some(validated);
            continue;
        }
        if std::time::Instant::now() >= deadline {
            color_eyre::eyre::bail!(
                "transaction {hash} wait timed out after {}s (validated ledger {validated}, LastLedgerSequence {last_ledger_sequence}); check it later with its hash",
                limit.as_secs()
            );
        }
        tokio::time::sleep(poll).await;
    }
}

/// Simulate (never sign without it) → `describe` prints the summary and returns the confirmation
/// prompt → confirm → sign → submit → optionally wait for validation. `Ok` on user cancel.
/// `describe(engine_result, fee_drops, tx_sequence)`; `sign(tx_sequence, fee_drops, last_ledger_sequence)`.
#[allow(clippy::too_many_arguments)]
async fn simulate_confirm_submit(
    rpc: &RpcClient,
    network: &Network,
    yes: bool,
    wait: bool,
    announce_success: bool,
    tx_json: serde_json::Value,
    describe: impl FnOnce(&str, u32, u32) -> color_eyre::Result<String>,
    sign: impl FnOnce(u32, u32, u32) -> color_eyre::Result<String>,
) -> color_eyre::Result<()> {
    let sim = super::submit::simulate_tx_requiring_tes_success(rpc, tx_json)
        .await
        .map_err(|e| color_eyre::eyre::eyre!(e))?;
    let (tx_sequence, fee_drops, last_ledger_sequence) =
        signing::sequence_fee_ledger_from_simulate(&sim.tx_json)?;
    let prompt = describe(&sim.engine_result, fee_drops, tx_sequence)?;
    if !prompt_production_confirmation(&prompt, network, yes) {
        println!("Transaction cancelled by user.");
        return Ok(());
    }
    let blob = sign(tx_sequence, fee_drops, last_ledger_sequence)?;
    // Sign/submit failures propagate so scripts see a non-zero exit.
    let summary = rpc.submit_signed_tx(&blob).await?;
    println!("Transaction Hash: {}", summary.hash);
    if announce_success {
        println!("Transaction submitted successfully!");
    }
    if wait {
        let result = wait_validated(
            rpc,
            &summary.hash,
            last_ledger_sequence,
            WAIT_POLL,
            WAIT_LIMIT,
        )
        .await?;
        println!("Validated: {result}");
    }
    Ok(())
}

pub async fn execute_cli_command(
    cmd: Cmd,
    rpc_url: &str,
    network: &Network,
    signing_credential: Option<SigningCredential>,
    yes: bool,
    wait: bool,
) -> color_eyre::Result<()> {
    validate_cmd_account_and_issuer_addresses(&cmd)?;
    let rpc = RpcClient::connect(rpc_url)?;
    match cmd {
        Cmd::Info => {
            println!(
                "{}",
                serde_json::to_string_pretty(&rpc.server_info().await?)?
            );
        }
        Cmd::Account { address } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&rpc.account_info(&address).await?)?
            );
        }
        Cmd::Book {
            base,
            quote,
            issuer,
            limit,
        } => {
            let issuer = issuer.unwrap_or_default();
            let rows = rpc
                .book_offers(
                    &base,
                    if base.eq_ignore_ascii_case("XRP") {
                        None
                    } else {
                        Some(&issuer)
                    },
                    &quote,
                    if quote.eq_ignore_ascii_case("XRP") {
                        None
                    } else {
                        Some(&issuer)
                    },
                    limit,
                )
                .await?;
            println!("{}", serde_json::to_string_pretty(&rows)?);
        }
        Cmd::Summary { account } => {
            let account = account.unwrap_or_default();
            let server_info = rpc.server_info().await?;
            let fee = rpc.fee().await?;
            println!("LedgerIndex: {}", server_info.ledger_index);
            println!("OpenLedgerFee: {}", fee.open_ledger_fee_drops);
            if !account.is_empty() {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&rpc.account_info(&account).await?)?
                );
            }
        }
        Cmd::Nfts { address } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&rpc.account_nfts(&address).await?)?
            );
        }
        Cmd::Lines { address } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&rpc.account_lines(&address).await?)?
            );
        }
        Cmd::Amm {
            asset1,
            asset2,
            issuer1,
            issuer2,
        } => {
            let summary = rpc
                .amm_info(&asset1, issuer1.as_deref(), &asset2, issuer2.as_deref())
                .await?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        Cmd::TxHistory { address, limit } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&rpc.account_tx(&address, limit, None).await?.rows)?
            );
        }
        Cmd::AccountStatus { address } => {
            let is_activated = rpc.fetch_account_activation_status(&address).await?;
            println!("Account: {}", address);
            println!(
                "Status: {}",
                if is_activated {
                    "Activated"
                } else {
                    "Not Activated"
                }
            );
            if !is_activated {
                println!("Note: Account requires 10+ XRP to be activated for transactions");
            }
        }
        Cmd::Offers { address } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&rpc.account_offers(&address).await?)?
            );
        }
        Cmd::OfferCancel { sequence } => {
            let wallet = cli_wallet(signing_credential.as_ref())?;
            let account = wallet.classic_address.clone();
            rpc.ensure_network(network).await?;
            let account_info = rpc.account_info(&account).await?;
            // Never sign without simulate (Sequence/Fee/LLS from simulate, fee capped).
            let tx_json = signing::build_offer_cancel_tx_json_for_simulate(
                &account,
                sequence,
                account_info.sequence,
            )?;
            simulate_confirm_submit(
                &rpc,
                network,
                yes,
                wait,
                false,
                tx_json,
                |_, fee_drops, _| {
                    println!("OfferCancel #{sequence} from {account} (fee {fee_drops} drops)");
                    Ok(format!("Cancel offer #{sequence} of {account}"))
                },
                |tx_sequence, fee_drops, last_ledger_sequence| {
                    signing::create_and_sign_offer_cancel(
                        &wallet,
                        &account,
                        sequence,
                        tx_sequence,
                        fee_drops,
                        last_ledger_sequence,
                    )
                },
            )
            .await?;
        }
        Cmd::OfferCreate {
            taker_gets,
            taker_pays,
        } => {
            let wallet = cli_wallet(signing_credential.as_ref())?;
            let account = wallet.classic_address.clone();
            rpc.ensure_network(network).await?;
            let account_info = rpc.account_info(&account).await?;
            let tx_json = signing::build_offer_create_tx_json_for_simulate(
                &account,
                &taker_gets,
                &taker_pays,
                account_info.sequence,
            )?;
            simulate_confirm_submit(
                &rpc,
                network,
                yes,
                wait,
                false,
                tx_json,
                |_, fee_drops, tx_sequence| {
                    println!(
                        "OfferCreate from {account}: give {taker_gets}, want {taker_pays} (fee {fee_drops} drops, offer seq {tx_sequence})"
                    );
                    Ok(format!("Place offer: give {taker_gets}, want {taker_pays}"))
                },
                |tx_sequence, fee_drops, last_ledger_sequence| {
                    signing::create_and_sign_offer_create(
                        &wallet,
                        &account,
                        &taker_gets,
                        &taker_pays,
                        tx_sequence,
                        fee_drops,
                        last_ledger_sequence,
                    )
                },
            )
            .await?;
        }
        Cmd::AmmDeposit { amount, amount2 } => {
            let wallet = cli_wallet(signing_credential.as_ref())?;
            let account = wallet.classic_address.clone();
            rpc.ensure_network(network).await?;
            let account_info = rpc.account_info(&account).await?;
            let tx_json = signing::build_amm_deposit_tx_json_for_simulate(
                &account,
                &amount,
                &amount2,
                account_info.sequence,
            )?;
            simulate_confirm_submit(
                &rpc,
                network,
                yes,
                wait,
                false,
                tx_json,
                |_, fee_drops, _| {
                    println!(
                        "AMMDeposit from {account}: up to {amount} + {amount2} (fee {fee_drops} drops)"
                    );
                    Ok(format!("AMM deposit up to {amount} + {amount2}"))
                },
                |tx_sequence, fee_drops, last_ledger_sequence| {
                    signing::create_and_sign_amm_deposit(
                        &wallet,
                        &account,
                        &amount,
                        &amount2,
                        tx_sequence,
                        fee_drops,
                        last_ledger_sequence,
                    )
                },
            )
            .await?;
        }
        Cmd::AmmWithdraw { asset, asset2, lp } => {
            let wallet = cli_wallet(signing_credential.as_ref())?;
            let account = wallet.classic_address.clone();
            rpc.ensure_network(network).await?;
            let account_info = rpc.account_info(&account).await?;
            let lp_token = match lp.as_deref().map(str::trim) {
                Some(value) => Some((rpc.amm_lp_token(&asset, &asset2).await?, value)),
                None => None,
            };
            let lp_in = lp_token
                .as_ref()
                .map(|((currency, issuer), value)| (currency.as_str(), issuer.as_str(), *value));
            let what = lp_in.map_or("all LP tokens".to_string(), |(_, _, v)| format!("{v} LP"));
            let tx_json = signing::build_amm_withdraw_tx_json_for_simulate(
                &account,
                &asset,
                &asset2,
                lp_in,
                account_info.sequence,
            )?;
            simulate_confirm_submit(
                &rpc,
                network,
                yes,
                wait,
                false,
                tx_json,
                |_, fee_drops, _| {
                    println!(
                        "AMMWithdraw from {account}: {what} of {asset}/{asset2} (fee {fee_drops} drops)"
                    );
                    Ok(format!("AMM withdraw {what} of {asset}/{asset2}"))
                },
                |tx_sequence, fee_drops, last_ledger_sequence| {
                    signing::create_and_sign_amm_withdraw(
                        &wallet,
                        &account,
                        &asset,
                        &asset2,
                        lp_in,
                        tx_sequence,
                        fee_drops,
                        last_ledger_sequence,
                    )
                },
            )
            .await?;
        }
        Cmd::AccountDelete { destination } => {
            use super::address::{ensure_xaddress_matches_network, resolve_payment_destination};

            let resolved = resolve_payment_destination(destination.trim())?;
            ensure_xaddress_matches_network(&resolved, network)?;
            let (dest, tag) = (resolved.classic, resolved.destination_tag);
            let wallet = cli_wallet(signing_credential.as_ref())?;
            let account = wallet.classic_address.clone();
            rpc.ensure_network(network).await?;
            let account_info = rpc.account_info(&account).await?;
            let tx_json = signing::build_account_delete_tx_json_for_simulate(
                &account,
                &dest,
                tag,
                account_info.sequence,
            )?;
            let tag_note = tag.map_or(String::new(), |t| format!(" (tag {t})"));
            simulate_confirm_submit(
                &rpc,
                network,
                yes,
                wait,
                false,
                tx_json,
                |_, fee_drops, _| {
                    println!(
                        "AccountDelete {account} → {dest}{tag_note}: balance {} XRP minus fee {fee_drops} drops goes to the destination; the account is removed (irreversible)",
                        account_info.balance_xrp
                    );
                    Ok(format!(
                        "AccountDelete {account} (remaining XRP to {dest}{tag_note})"
                    ))
                },
                |tx_sequence, fee_drops, last_ledger_sequence| {
                    signing::create_and_sign_account_delete(
                        &wallet,
                        &account,
                        &dest,
                        tag,
                        tx_sequence,
                        fee_drops,
                        last_ledger_sequence,
                    )
                },
            )
            .await?;
        }
        Cmd::Faucet { address } => {
            let url = faucet_url(network)?;
            // No address: keys are generated here; only the address goes to the faucet.
            let (address, seed) = match address {
                Some(address) => (address, None),
                None => {
                    let keys = signing::generate_wallet_keys_local("ed25519")?;
                    let seed = secrecy::SecretString::from(keys.master_seed.clone());
                    (keys.account_id.clone(), Some(seed))
                }
            };
            let funded = fund_from_faucet(url, &address).await?;
            println!("Funded {address} with {} XRP", funded.amount_xrp);
            if let Some(hash) = funded.tx_hash {
                println!("Transaction Hash: {hash}");
            }
            if let Some(seed) = seed {
                println!("Seed: {}", secrecy::ExposeSecret::expose_secret(&seed));
                eprintln!(
                    "note: test-network seed shown once and not saved; export XRPL_SEED to sign with it"
                );
            }
        }
        Cmd::Send {
            destination,
            amount,
        } => {
            use super::address::{ensure_xaddress_matches_network, resolve_payment_destination};

            signing::validate_payment_input(&destination, &amount, None, None)?;
            let wallet = cli_wallet(signing_credential.as_ref())?;
            let account = wallet.classic_address.clone();

            let resolved = resolve_payment_destination(destination.trim())?;
            ensure_xaddress_matches_network(&resolved, network)?;
            let destination_classic = resolved.classic;
            let destination_tag = resolved.destination_tag;

            rpc.ensure_network(network).await?;
            let account_info = rpc.account_info(&account).await?;
            let balance_xrp_str = account_info.balance_xrp;
            let balance_drops = xrp_to_drops(&balance_xrp_str).unwrap_or(0);

            println!("From: {}", account);
            println!("To: {}", destination_classic);
            if let Some(tag) = destination_tag {
                println!("Destination Tag: {}", tag);
            }
            println!("Amount: {} XRP", amount);
            println!("Current Balance: {} XRP", balance_xrp_str);

            // Never sign without simulate: Sequence/Fee/LastLedgerSequence come from simulate
            // (fee capped by `sequence_fee_ledger_from_simulate`).
            let tx_json = signing::build_payment_tx_json_for_simulate(
                &account,
                &destination_classic,
                &amount,
                None,
                None,
                None,
                destination_tag,
                None,
                account_info.sequence,
            )?;
            simulate_confirm_submit(
                &rpc,
                network,
                yes,
                wait,
                true,
                tx_json,
                |engine_result, fee_drops, _| {
                    println!("Simulate: {} (fee {} drops)", engine_result, fee_drops);
                    let amount_drops = xrp_to_drops(&amount)?;
                    let needed_drops = amount_drops
                        .checked_add(u64::from(fee_drops))
                        .ok_or_else(|| color_eyre::eyre::eyre!("amount too large"))?;
                    if balance_drops < needed_drops {
                        return Err(color_eyre::eyre::eyre!(
                            "Insufficient balance: current {} drops, need {} drops",
                            balance_drops,
                            needed_drops
                        ));
                    }
                    Ok(format!("Send {} XRP to {}", amount, destination_classic))
                },
                |sequence, fee_drops, last_ledger_sequence| {
                    signing::create_and_sign_payment(
                        &wallet,
                        &account,
                        &destination_classic,
                        &amount,
                        None, // iou_currency: XRP-only
                        None, // iou_issuer: XRP-only
                        None, // send_max
                        destination_tag,
                        None, // memo_data
                        sequence,
                        fee_drops,
                        last_ledger_sequence,
                    )
                },
            )
            .await?;
        }
    }
    Ok(())
}

/// Lookup entry used by the short `rp` argv0 alias.
pub async fn execute_rp_lookup(rpc_url: &str, raw: &str) -> color_eyre::Result<()> {
    let rpc = RpcClient::connect(rpc_url)?;
    match classify_rp_target(raw.trim())? {
        RpTarget::Account(address) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&rpc.account_info(&address).await?)?
            );
        }
        RpTarget::TxHash(hash) => {
            let value = rpc.tx(&hash).await?;
            let body = value.get("result").cloned().unwrap_or(value);
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum RpTarget {
    Account(String),
    TxHash(String),
}

fn looks_like_tx_hash(s: &str) -> bool {
    let s = s.strip_prefix("0x").unwrap_or(s);
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn classify_rp_target(raw: &str) -> color_eyre::Result<RpTarget> {
    if raw.is_empty() {
        return Err(color_eyre::eyre::eyre!("rp: empty target"));
    }
    if looks_like_tx_hash(raw) {
        let hash = raw.strip_prefix("0x").unwrap_or(raw).to_string();
        return Ok(RpTarget::TxHash(hash));
    }
    match super::address::resolve_payment_destination(raw) {
        Ok(resolved) => Ok(RpTarget::Account(resolved.classic)),
        Err(_) => Err(color_eyre::eyre::eyre!(
            "rp: not a tx hash (64 hex) or account address (classic/X): {raw}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{RpTarget, classify_rp_target, execute_cli_command, looks_like_tx_hash};
    use crate::cli::Cmd;
    use crate::network::Network;

    #[test]
    fn looks_like_tx_hash_accepts_64_hex_and_0x() {
        let h = "a".repeat(64);
        assert!(looks_like_tx_hash(&h));
        assert!(looks_like_tx_hash(&format!("0x{h}")));
        assert!(!looks_like_tx_hash(&"a".repeat(63)));
        assert!(!looks_like_tx_hash("not-hex"));
    }

    #[test]
    fn classify_rp_target_splits_hash_and_classic() {
        let h = "b".repeat(64);
        assert_eq!(classify_rp_target(&h).unwrap(), RpTarget::TxHash(h.clone()));
        assert_eq!(
            classify_rp_target(&format!("0x{h}")).unwrap(),
            RpTarget::TxHash(h)
        );
        let addr = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        assert_eq!(
            classify_rp_target(addr).unwrap(),
            RpTarget::Account(addr.into())
        );
        assert!(classify_rp_target("nope").is_err());
        assert!(classify_rp_target("").is_err());
    }

    /// TC-059: invalid classic account addresses fail before RPC connect
    #[tokio::test]
    async fn cli_rejects_invalid_account_before_rpc_connect() {
        let r = execute_cli_command(
            Cmd::Account {
                address: "not-an-address".into(),
            },
            "http://127.0.0.1:1",
            &Network::Mainnet,
            None,
            false,
            false,
        )
        .await;
        let err = r.expect_err("invalid classic address must fail");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("expected valid classic address"),
            "error should reflect address validation, got: {err}"
        );
    }

    mod local_rpc_integration {
        use serde_json::{Value, json};
        use tokio::net::TcpListener;

        use super::super::execute_cli_command;
        use crate::cli::Cmd;
        use crate::network::Network;
        use crate::test_support::rpc_mock;

        const GENESIS: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        const RLUSD_ISSUER: &str = "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De";

        fn result_for(method: &str) -> Value {
            match method {
                "server_info" => json!({
                    "info": {
                        "network_id": 1,
                        "validated_ledger": {"seq": 80_000_000},
                        "hostid": "local-test",
                        "build_version": "local"
                    }
                }),
                "fee" => json!({"drops": {"open_ledger_fee": 15}}),
                "account_info" => json!({
                    "account_data": {
                        "Account": GENESIS,
                        "Balance": "10000000",
                        "Sequence": 7,
                        "OwnerCount": 1,
                        "Flags": 0
                    }
                }),
                "book_offers" => json!({
                    "offers": [{
                        "quality": "0.5",
                        "TakerGets": "1000000",
                        "TakerPays": {
                            "currency": "RLUSD",
                            "value": "2",
                            "issuer": RLUSD_ISSUER
                        }
                    }]
                }),
                "account_nfts" => json!({"account_nfts": []}),
                "account_lines" => json!({"lines": []}),
                "amm_info" => json!({
                    "amm": {
                        "lp_token": {"value": "42", "currency": "03"},
                        "trading_fee": 12,
                        "amount": "2000000",
                        "amount2": {
                            "currency": "RLUSD",
                            "value": "3",
                            "issuer": RLUSD_ISSUER
                        }
                    }
                }),
                "account_tx" => json!({"transactions": []}),
                "account_offers" => json!({
                    "offers": [{"seq": 42, "taker_gets": "1000000", "taker_pays": "2000000"}]
                }),
                "simulate" => json!({
                    "engine_result": "tesSUCCESS",
                    // rippled 3.4.1 shape: no LastLedgerSequence; filled from ledger_index (TC-200).
                    "ledger_index": 80_000_000,
                    "tx_json": {"Sequence": 7, "Fee": "12"}
                }),
                "submit" => json!({"engine_result": "tesSUCCESS", "tx_json": {"hash": "ABC"}}),
                other => panic!("unexpected local RPC method: {other}"),
            }
        }

        async fn serve(listener: TcpListener, expected: usize) -> color_eyre::Result<Vec<String>> {
            serve_with(listener, expected, result_for).await
        }

        async fn serve_with(
            listener: TcpListener,
            expected: usize,
            result: fn(&str) -> Value,
        ) -> color_eyre::Result<Vec<String>> {
            let requests = rpc_mock::serve(listener, expected, result).await?;
            Ok(rpc_mock::methods(&requests)
                .into_iter()
                .map(str::to_owned)
                .collect())
        }

        /// TC-050–057, TC-066: read-only CLI commands use a deterministic
        /// local JSON-RPC boundary instead of the public internet.
        #[tokio::test]
        async fn read_only_cli_commands_succeed_against_local_rpc() -> color_eyre::Result<()> {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
            let url = format!("http://{}", listener.local_addr()?);
            let server = tokio::spawn(serve(listener, 11));

            async fn run(step: &str, cmd: Cmd, url: &str) -> color_eyre::Result<()> {
                execute_cli_command(cmd, url, &Network::Testnet, None, false, false)
                    .await
                    .map_err(|e| color_eyre::eyre::eyre!("{step}: {e}"))
            }

            run("info (server_info)", Cmd::Info, &url).await?;
            run(
                "account (account_info #1)",
                Cmd::Account {
                    address: GENESIS.into(),
                },
                &url,
            )
            .await?;
            run(
                "book (book_offers)",
                Cmd::Book {
                    base: "XRP".into(),
                    quote: "RLUSD".into(),
                    issuer: Some(RLUSD_ISSUER.into()),
                    limit: 5,
                },
                &url,
            )
            .await?;
            run(
                "summary (server_info #2, fee)",
                Cmd::Summary {
                    account: Some(GENESIS.into()),
                },
                &url,
            )
            .await?;
            run(
                "nfts (account_nfts)",
                Cmd::Nfts {
                    address: GENESIS.into(),
                },
                &url,
            )
            .await?;
            run(
                "lines (account_lines)",
                Cmd::Lines {
                    address: GENESIS.into(),
                },
                &url,
            )
            .await?;
            run(
                "amm (amm_info)",
                Cmd::Amm {
                    asset1: "XRP".into(),
                    asset2: "RLUSD".into(),
                    issuer1: None,
                    issuer2: Some(RLUSD_ISSUER.into()),
                },
                &url,
            )
            .await?;
            run(
                "tx-history (account_tx)",
                Cmd::TxHistory {
                    address: GENESIS.into(),
                    limit: 5,
                },
                &url,
            )
            .await?;
            run(
                "account-status (account_info #2)",
                Cmd::AccountStatus {
                    address: GENESIS.into(),
                },
                &url,
            )
            .await?;

            let methods = server.await??;
            assert_eq!(
                methods,
                [
                    "server_info",
                    "account_info",
                    "book_offers",
                    "server_info",
                    "fee",
                    "account_info",
                    "account_nfts",
                    "account_lines",
                    "amm_info",
                    "account_tx",
                    "account_info",
                ]
            );
            Ok(())
        }

        /// TC-188: `-x send` checks the endpoint network, then account_info → simulate → submit
        /// (no unsimulated signing); sequence/fee/LLS come from the simulate response.
        #[tokio::test]
        async fn send_simulates_before_signing_and_submits() -> color_eyre::Result<()> {
            use secrecy::SecretString;

            let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
            let url = format!("http://{}", listener.local_addr()?);
            let server = tokio::spawn(serve(listener, 4));
            let credential = crate::signing::credential_from_secrets(
                Some(&SecretString::from("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB")),
                None,
            )?;
            execute_cli_command(
                Cmd::Send {
                    destination: GENESIS.into(),
                    amount: "0.000123".into(),
                },
                &url,
                &Network::Testnet,
                credential,
                false,
                false,
            )
            .await?;
            assert_eq!(
                server.await??,
                ["server_info", "account_info", "simulate", "submit"]
            );
            Ok(())
        }

        /// TC-203: a mainnet node (no `network_id`) behind `--network testnet` (e.g. env
        /// `XRPL_RPC_SERVER`) is refused before account_info / signing.
        #[tokio::test]
        async fn write_refuses_endpoint_on_other_network() -> color_eyre::Result<()> {
            use secrecy::SecretString;

            fn mainnet_node(method: &str) -> Value {
                match method {
                    "server_info" => json!({"info": {"validated_ledger": {"seq": 1}}}),
                    other => result_for(other),
                }
            }
            let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
            let url = format!("http://{}", listener.local_addr()?);
            let server = tokio::spawn(serve_with(listener, 1, mainnet_node));
            let credential = crate::signing::credential_from_secrets(
                Some(&SecretString::from("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB")),
                None,
            )?;
            let err = execute_cli_command(
                Cmd::Send {
                    destination: GENESIS.into(),
                    amount: "1".into(),
                },
                &url,
                &Network::Testnet,
                credential,
                false,
                false,
            )
            .await
            .expect_err("mainnet endpoint under --network testnet");
            assert!(err.to_string().contains("network_id"), "{err}");
            assert_eq!(server.await??, ["server_info"]);
            Ok(())
        }

        /// TC-212: `-x send` validates the amount (> 0) before any RPC, like the composer.
        #[tokio::test]
        async fn send_zero_amount_is_refused_before_rpc() -> color_eyre::Result<()> {
            use secrecy::SecretString;
            let credential = crate::signing::credential_from_secrets(
                Some(&SecretString::from("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB")),
                None,
            )?;
            let err = execute_cli_command(
                Cmd::Send {
                    destination: GENESIS.into(),
                    amount: "0".into(),
                },
                "http://127.0.0.1:1",
                &Network::Testnet,
                credential,
                false,
                false,
            )
            .await
            .expect_err("zero amount");
            assert!(err.to_string().contains("amount must be > 0"), "{err}");
            Ok(())
        }

        /// TC-196: `-x send` exits non-zero when submit is rejected (scripts must see failure).
        #[tokio::test]
        async fn send_submit_rejection_is_an_error() -> color_eyre::Result<()> {
            use secrecy::SecretString;

            fn rejecting(method: &str) -> Value {
                match method {
                    "submit" => json!({
                        "engine_result": "tefPAST_SEQ",
                        "engine_result_message": "This sequence number has already passed."
                    }),
                    other => result_for(other),
                }
            }
            let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
            let url = format!("http://{}", listener.local_addr()?);
            let server = tokio::spawn(serve_with(listener, 4, rejecting));
            let credential = crate::signing::credential_from_secrets(
                Some(&SecretString::from("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB")),
                None,
            )?;
            let err = execute_cli_command(
                Cmd::Send {
                    destination: GENESIS.into(),
                    amount: "0.000123".into(),
                },
                &url,
                &Network::Testnet,
                credential,
                false,
                false,
            )
            .await
            .expect_err("rejected submit must fail the command");
            assert!(err.to_string().contains("tefPAST_SEQ"), "{err}");
            assert_eq!(
                server.await??,
                ["server_info", "account_info", "simulate", "submit"]
            );
            Ok(())
        }

        /// TC-191: `-x offers` reads `account_offers`; `-x offer-cancel` / `-x offer-create` run
        /// account_info → simulate → submit and need a signing credential before any RPC;
        /// a bad offer spec fails before any RPC.
        #[tokio::test]
        async fn dex_offers_and_offer_cancel_against_local_rpc() -> color_eyre::Result<()> {
            use secrecy::SecretString;

            let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
            let url = format!("http://{}", listener.local_addr()?);
            let server = tokio::spawn(serve(listener, 9));
            execute_cli_command(
                Cmd::Offers {
                    address: GENESIS.into(),
                },
                &url,
                &Network::Testnet,
                None,
                false,
                false,
            )
            .await?;

            let no_key = execute_cli_command(
                Cmd::OfferCancel { sequence: 42 },
                &url,
                &Network::Testnet,
                None,
                false,
                false,
            )
            .await
            .expect_err("offer-cancel without credential");
            assert!(
                no_key.to_string().contains("signing credential"),
                "{no_key}"
            );

            let credential = crate::signing::credential_from_secrets(
                Some(&SecretString::from("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB")),
                None,
            )?;
            execute_cli_command(
                Cmd::OfferCancel { sequence: 42 },
                &url,
                &Network::Testnet,
                credential.clone(),
                false,
                false,
            )
            .await?;

            let bad_spec = execute_cli_command(
                Cmd::OfferCreate {
                    taker_gets: "XRP:abc".into(),
                    taker_pays: format!("USD:{GENESIS}:1"),
                },
                &url,
                &Network::Testnet,
                credential.clone(),
                false,
                false,
            )
            .await
            .expect_err("bad TakerGets spec");
            assert!(bad_spec.to_string().starts_with("TakerGets:"), "{bad_spec}");
            execute_cli_command(
                Cmd::OfferCreate {
                    taker_gets: "XRP:1000000".into(),
                    taker_pays: format!("USD:{GENESIS}:1"),
                },
                &url,
                &Network::Testnet,
                credential,
                false,
                false,
            )
            .await?;
            assert_eq!(
                server.await??,
                [
                    "account_offers",
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
            Ok(())
        }

        /// TC-201: faucet — testnet/devnet only; POSTs `destination` and reports amount + hash;
        /// a response funding a different address is rejected.
        #[tokio::test]
        async fn faucet_funds_destination_on_test_networks_only() -> color_eyre::Result<()> {
            use super::super::{faucet_url, fund_from_faucet};

            assert!(faucet_url(&Network::Testnet)?.contains("altnet"));
            assert!(faucet_url(&Network::Devnet)?.contains("devnet"));
            for network in [Network::Mainnet, Network::Xahau, Network::XahauTest] {
                assert!(faucet_url(&network).is_err(), "{network:?}");
            }

            let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
            let url = format!("http://{}/accounts", listener.local_addr()?);
            let server = tokio::spawn(rpc_mock::serve_raw(
                listener,
                2,
                |_| json!({"account": {"classicAddress": GENESIS}, "amount": 100, "transactionHash": "ABC"}),
            ));
            let funded = fund_from_faucet(&url, GENESIS).await?;
            assert_eq!(
                (funded.amount_xrp.as_str(), funded.tx_hash.as_deref()),
                ("100", Some("ABC"))
            );
            let mismatch = fund_from_faucet(&url, RLUSD_ISSUER)
                .await
                .expect_err("faucet funded another address");
            assert!(mismatch.to_string().contains(GENESIS), "{mismatch}");
            let requests = server.await??;
            assert_eq!(requests[0]["destination"], GENESIS);
            Ok(())
        }

        /// TC-202: write wait — polls `tx` until validated (tes → Ok, tec → Err naming the code);
        /// not found once the validated ledger passes LastLedgerSequence → Err (expired);
        /// a node that never advances (or omits `validated_ledger`) hits the time limit → Err.
        #[tokio::test]
        async fn wait_validated_reports_final_result() -> color_eyre::Result<()> {
            use std::sync::atomic::{AtomicUsize, Ordering};
            use std::time::Duration;

            use super::super::wait_validated;
            use crate::xrpl::client::RpcClient;

            async fn run(
                lls: u32,
                limit: Duration,
                replies: Vec<Value>,
            ) -> color_eyre::Result<(color_eyre::Result<String>, Vec<String>)> {
                let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
                let rpc = RpcClient::connect(&format!("http://{}", listener.local_addr()?))?;
                let expected = replies.len();
                let next = AtomicUsize::new(0);
                let server = tokio::spawn(rpc_mock::serve_raw(listener, expected, move |_| {
                    replies[next.fetch_add(1, Ordering::SeqCst)].clone()
                }));
                let outcome = wait_validated(&rpc, "ABC", lls, Duration::ZERO, limit).await;
                let requests = server.await??;
                let methods = rpc_mock::methods(&requests)
                    .into_iter()
                    .map(str::to_owned)
                    .collect();
                Ok((outcome, methods))
            }
            let pending = json!({"result": {"validated": false}});
            let ledger = |seq: u32| json!({"result": {"info": {"validated_ledger": {"seq": seq}}}});
            let minute = Duration::from_secs(60);
            let validated = |code: &str| json!({"result": {"validated": true, "meta": {"TransactionResult": code}}});

            let (ok, methods) = run(
                120,
                minute,
                vec![pending.clone(), ledger(100), validated("tesSUCCESS")],
            )
            .await?;
            assert_eq!(ok?, "tesSUCCESS");
            assert_eq!(methods, ["tx", "server_info", "tx"]);

            let (tec, _) = run(120, minute, vec![validated("tecUNFUNDED_OFFER")]).await?;
            assert!(
                tec.expect_err("tec is a failure")
                    .to_string()
                    .contains("tecUNFUNDED_OFFER")
            );

            let not_found = json!({"result": {"error": "txnNotFound", "status": "error"}});
            let (expired, methods) = run(
                120,
                minute,
                vec![not_found.clone(), ledger(121), not_found.clone()],
            )
            .await?;
            assert!(
                expired
                    .expect_err("past LLS")
                    .to_string()
                    .contains("expired")
            );
            assert_eq!(methods, ["tx", "server_info", "tx"]);

            // Validated between the `tx` miss and the `server_info` read: success, not "expired".
            let (raced, _) = run(
                120,
                minute,
                vec![not_found, ledger(121), validated("tesSUCCESS")],
            )
            .await?;
            assert_eq!(raced?, "tesSUCCESS");

            let no_ledger = json!({"result": {"info": {}}});
            let (stuck, methods) = run(120, Duration::ZERO, vec![pending, no_ledger]).await?;
            assert!(
                stuck
                    .expect_err("never advances")
                    .to_string()
                    .contains("timed out")
            );
            assert_eq!(methods, ["tx", "server_info"]);
            Ok(())
        }
    }

    mod integration_live_network {
        use std::time::Duration;

        use super::super::execute_cli_command;
        use crate::cli::Cmd;
        use crate::network::Network;

        /// Manual live-network check only. Read-only CLI behavior is covered
        /// by the local RPC test above; this remains opt-in for real-mainnet signing.
        #[tokio::test]
        #[ignore = "manual live-network check: requires XRPL_SEED; mainnet may send real XRP if stdin confirms"]
        async fn cli_send_live_manual_only() -> color_eyre::Result<()> {
            tokio::time::timeout(
                Duration::from_secs(90),
                execute_cli_command(
                    Cmd::Send {
                        destination: "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into(),
                        amount: "0.000123".to_string(),
                    },
                    "https://xrplcluster.com",
                    &Network::Mainnet,
                    None,
                    false,
                    false,
                ),
            )
            .await
            .map_err(|_| color_eyre::eyre::eyre!("XRPL integration test timed out"))?
        }
    }
}
