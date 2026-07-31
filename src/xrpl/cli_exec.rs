use secrecy::ExposeSecret;

use crate::cli::Cmd;
use crate::network::Network;
use crate::signing::{self, SigningConfig, prompt_mainnet_confirmation};

use super::client::{RpcClient, xrp_to_drops};

pub async fn execute_cli_command(
    cmd: Cmd,
    rpc_url: &str,
    network: &Network,
    signing_seed: Option<String>,
    yes: bool,
) -> color_eyre::Result<()> {
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
            let is_activated = rpc.is_account_activated(&address).await?;
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
        Cmd::Send {
            destination,
            amount,
        } => {
            use super::address::{ensure_xaddress_matches_network, resolve_payment_destination};

            let signing_config = SigningConfig::prime_seed_source(signing_seed.clone());
            let Some(seed) = signing_config.seed.as_ref() else {
                return Err(color_eyre::eyre::eyre!(
                    "No signing seed: set XRPL_SEED, put seed in config [xrpl.signing] seed, or use --seed (family seed s... or sEd...)."
                ));
            };
            let wallet = signing::wallet_from_family_seed(seed.expose_secret(), 0)
                .map_err(|e| color_eyre::eyre::eyre!(e))?;
            let account = wallet.classic_address.clone();

            let resolved = resolve_payment_destination(destination.trim())?;
            ensure_xaddress_matches_network(&resolved, network)?;
            let destination_classic = resolved.classic;
            let destination_tag = resolved.destination_tag;

            let account_info = rpc.account_info(&account).await?;
            let balance_xrp_str = account_info.balance_xrp;
            let balance_drops = xrp_to_drops(&balance_xrp_str).unwrap_or(0);
            let sequence = account_info.sequence;

            println!("From: {}", account);
            println!("To: {}", destination_classic);
            if let Some(tag) = destination_tag {
                println!("Destination Tag: {}", tag);
            }
            println!("Amount: {} XRP", amount);
            println!("Current Balance: {} XRP", balance_xrp_str);
            println!("Account Sequence: {}", sequence);

            let amount_drops = xrp_to_drops(&amount)?;
            if balance_drops < amount_drops + 10 {
                return Err(color_eyre::eyre::eyre!(
                    "Insufficient balance: current {} drops, need {} drops",
                    balance_drops,
                    amount_drops + 10
                ));
            }

            let fee_info = rpc.fee().await?;
            let fee_drops = fee_info.open_ledger_fee_drops;
            let server_info = rpc.server_info().await?;
            let last_ledger_sequence = server_info.ledger_index + 20;

            if !prompt_mainnet_confirmation(
                &format!("Send {} XRP to {}", amount, destination_classic),
                network,
                yes,
            ) {
                println!("Transaction cancelled by user.");
                return Ok(());
            }

            match signing::create_and_sign_payment(
                seed,
                &account,
                &destination_classic,
                &amount,
                None, // iou_currency: XRP-only
                None, // iou_issuer: XRP-only
                destination_tag,
                sequence,
                fee_drops,
                last_ledger_sequence,
                network,
            ) {
                Ok(signed_tx_blob) => {
                    println!("\n=== Transaction Created ===");
                    println!("Signed transaction blob:");
                    println!("{}", signed_tx_blob);

                    match rpc.submit_signed_tx(&signed_tx_blob).await {
                        Ok(tx_summary) => {
                            println!("\n=== Transaction Submitted ===");
                            println!("Transaction Hash: {}", tx_summary.hash);
                            println!("Transaction submitted successfully!");
                        }
                        Err(e) => {
                            println!("\n=== Submission Failed ===");
                            println!("Error: {}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("\n=== Signing Failed ===");
                    println!("Error: {}", e);
                }
            }
        }
        Cmd::Watch { .. } => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Live XRPL JSON-RPC (mainnet public cluster). Serialized to avoid connection pile-up.
    mod integration_live_network {
        use std::time::Duration;

        use super::super::execute_cli_command;
        use crate::cli::Cmd;
        use crate::network::Network;

        const RPC: &str = "https://xrplcluster.com";
        const GENESIS: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        const RLUSD_ISSUER: &str = "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De";
        static LIVE_RPC_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

        async fn run(cmd: Cmd) -> color_eyre::Result<()> {
            let _guard = LIVE_RPC_LOCK.lock().await;
            tokio::time::sleep(Duration::from_millis(250)).await;
            tokio::time::timeout(
                Duration::from_secs(90),
                execute_cli_command(cmd, RPC, &Network::Mainnet, None, false),
            )
            .await
            .map_err(|_| color_eyre::eyre::eyre!("XRPL integration test timed out"))?
        }

        /// TC-050
        #[tokio::test]
        async fn cli_info_ok() -> color_eyre::Result<()> {
            run(Cmd::Info).await
        }

        /// TC-051
        #[tokio::test]
        async fn cli_account_ok() -> color_eyre::Result<()> {
            run(Cmd::Account {
                address: GENESIS.into(),
            })
            .await
        }

        /// TC-052
        #[tokio::test]
        #[ignore = "live network dependency: RLUSD 4-char code unsupported on public nodes"]
        async fn cli_book_ok() -> color_eyre::Result<()> {
            run(Cmd::Book {
                base: "XRP".into(),
                quote: "RLUSD".into(),
                issuer: Some(RLUSD_ISSUER.into()),
                limit: 5,
            })
            .await
        }

        /// TC-053
        #[tokio::test]
        async fn cli_summary_ok() -> color_eyre::Result<()> {
            run(Cmd::Summary {
                account: Some(GENESIS.into()),
            })
            .await
        }

        /// TC-054
        #[tokio::test]
        async fn cli_nfts_ok() -> color_eyre::Result<()> {
            run(Cmd::Nfts {
                address: GENESIS.into(),
            })
            .await
        }

        /// TC-055
        #[tokio::test]
        async fn cli_lines_ok() -> color_eyre::Result<()> {
            run(Cmd::Lines {
                address: GENESIS.into(),
            })
            .await
        }

        /// TC-056
        #[tokio::test]
        #[ignore = "live network dependency: AMM support varies by public node"]
        async fn cli_amm_ok() -> color_eyre::Result<()> {
            run(Cmd::Amm {
                asset1: "XRP".into(),
                asset2: "RLUSD".into(),
                issuer1: None,
                issuer2: Some(RLUSD_ISSUER.into()),
            })
            .await
        }

        /// TC-057
        #[tokio::test]
        async fn cli_txhistory_ok() -> color_eyre::Result<()> {
            run(Cmd::TxHistory {
                address: GENESIS.into(),
                limit: 5,
            })
            .await
        }

        /// TC-059
        #[tokio::test]
        async fn cli_invalid_account_errors() {
            let _guard = LIVE_RPC_LOCK.lock().await;
            tokio::time::sleep(Duration::from_millis(250)).await;
            let r = execute_cli_command(
                Cmd::Account {
                    address: "not-an-address".into(),
                },
                RPC,
                &Network::Mainnet,
                None,
                false,
            )
            .await;
            assert!(r.is_err());
        }

        /// TC-066
        #[tokio::test]
        async fn cli_account_status_ok() -> color_eyre::Result<()> {
            run(Cmd::AccountStatus {
                address: GENESIS.into(),
            })
            .await
        }

        /// TC-067
        #[tokio::test]
        #[ignore = "requires XRPL_SEED environment variable"]
        async fn cli_send_simulation_ok() -> color_eyre::Result<()> {
            run(Cmd::Send {
                destination: "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into(),
                amount: "0.000123".to_string(),
            })
            .await
        }
    }
}
