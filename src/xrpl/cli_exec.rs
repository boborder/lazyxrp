use crate::cli::Cmd;
use crate::network::Network;
use crate::signing::{self, SigningCredential, prompt_production_confirmation};

use super::client::{RpcClient, xrp_to_drops};

fn validate_cmd_account_and_issuer_addresses(cmd: &Cmd) -> color_eyre::Result<()> {
    use crate::signing::require_classic_address_shape;

    match cmd {
        Cmd::Account { address } => {
            require_classic_address_shape("account", address)?;
        }
        Cmd::Nfts { address } => {
            require_classic_address_shape("account", address)?;
        }
        Cmd::Lines { address } => {
            require_classic_address_shape("account", address)?;
        }
        Cmd::TxHistory { address, .. } => {
            require_classic_address_shape("account", address)?;
        }
        Cmd::AccountStatus { address } => {
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
        Cmd::Offers { address } => {
            require_classic_address_shape("account", address)?;
        }
        Cmd::Info | Cmd::Send { .. } | Cmd::OfferCancel { .. } => {}
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

pub async fn execute_cli_command(
    cmd: Cmd,
    rpc_url: &str,
    network: &Network,
    signing_credential: Option<SigningCredential>,
    yes: bool,
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
            let account_info = rpc.account_info(&account).await?;
            // Never sign without simulate (Sequence/Fee/LLS from simulate, fee capped).
            let tx_json = signing::build_offer_cancel_tx_json_for_simulate(
                &account,
                sequence,
                account_info.sequence,
            )?;
            let sim = super::poll::simulate_tx_requiring_tes_success(&rpc, tx_json)
                .await
                .map_err(|e| color_eyre::eyre::eyre!(e))?;
            let (tx_sequence, fee_drops, last_ledger_sequence) =
                signing::sequence_fee_ledger_from_simulate(&sim.tx_json)?;
            println!("OfferCancel #{sequence} from {account} (fee {fee_drops} drops)");
            if !prompt_production_confirmation(
                &format!("Cancel offer #{sequence} of {account}"),
                network,
                yes,
            ) {
                println!("Transaction cancelled by user.");
                return Ok(());
            }
            let blob = signing::create_and_sign_offer_cancel(
                &wallet,
                &account,
                sequence,
                tx_sequence,
                fee_drops,
                last_ledger_sequence,
            )?;
            let summary = rpc.submit_signed_tx(&blob).await?;
            println!("Transaction Hash: {}", summary.hash);
        }
        Cmd::Send {
            destination,
            amount,
        } => {
            use super::address::{ensure_xaddress_matches_network, resolve_payment_destination};

            let wallet = cli_wallet(signing_credential.as_ref())?;
            let account = wallet.classic_address.clone();

            let resolved = resolve_payment_destination(destination.trim())?;
            ensure_xaddress_matches_network(&resolved, network)?;
            let destination_classic = resolved.classic;
            let destination_tag = resolved.destination_tag;

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
            let sim = super::poll::simulate_tx_requiring_tes_success(&rpc, tx_json)
                .await
                .map_err(|e| color_eyre::eyre::eyre!(e))?;
            let (sequence, fee_drops, last_ledger_sequence) =
                signing::sequence_fee_ledger_from_simulate(&sim.tx_json)?;
            println!("Simulate: {} (fee {} drops)", sim.engine_result, fee_drops);

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

            if !prompt_production_confirmation(
                &format!("Send {} XRP to {}", amount, destination_classic),
                network,
                yes,
            ) {
                println!("Transaction cancelled by user.");
                return Ok(());
            }

            let signed_tx_blob = signing::create_and_sign_payment(
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
                network,
            )?;
            // Sign/submit failures propagate so scripts see a non-zero exit.
            let tx_summary = rpc.submit_signed_tx(&signed_tx_blob).await?;
            println!("Transaction Hash: {}", tx_summary.hash);
            println!("Transaction submitted successfully!");
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
                        "Asset": {"currency": "XRP"},
                        "Asset2": {"currency": "RLUSD", "issuer": RLUSD_ISSUER},
                        "LPToken": {"value": "42", "currency": "03"},
                        "TradingFee": 12,
                        "Amount": "2000000",
                        "Amount2": {
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
                    "tx_json": {"Sequence": 7, "Fee": "12", "LastLedgerSequence": 80_000_020}
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
                execute_cli_command(cmd, url, &Network::Testnet, None, false)
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

        /// TC-188: `-x send` runs account_info → simulate → submit (no unsimulated signing);
        /// sequence/fee/LLS come from the simulate response.
        #[tokio::test]
        async fn send_simulates_before_signing_and_submits() -> color_eyre::Result<()> {
            use secrecy::SecretString;

            let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
            let url = format!("http://{}", listener.local_addr()?);
            let server = tokio::spawn(serve(listener, 3));
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
            )
            .await?;
            assert_eq!(server.await??, ["account_info", "simulate", "submit"]);
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
            let server = tokio::spawn(serve_with(listener, 3, rejecting));
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
            )
            .await
            .expect_err("rejected submit must fail the command");
            assert!(err.to_string().contains("tefPAST_SEQ"), "{err}");
            assert_eq!(server.await??, ["account_info", "simulate", "submit"]);
            Ok(())
        }

        /// TC-191: `-x offers` reads `account_offers`; `-x offer-cancel` runs
        /// account_info → simulate → submit and needs a signing credential before any RPC.
        #[tokio::test]
        async fn dex_offers_and_offer_cancel_against_local_rpc() -> color_eyre::Result<()> {
            use secrecy::SecretString;

            let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
            let url = format!("http://{}", listener.local_addr()?);
            let server = tokio::spawn(serve(listener, 4));
            execute_cli_command(
                Cmd::Offers {
                    address: GENESIS.into(),
                },
                &url,
                &Network::Testnet,
                None,
                false,
            )
            .await?;

            let no_key = execute_cli_command(
                Cmd::OfferCancel { sequence: 42 },
                &url,
                &Network::Testnet,
                None,
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
                credential,
                false,
            )
            .await?;
            assert_eq!(
                server.await??,
                ["account_offers", "account_info", "simulate", "submit"]
            );
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
                ),
            )
            .await
            .map_err(|_| color_eyre::eyre::eyre!("XRPL integration test timed out"))?
        }
    }
}
