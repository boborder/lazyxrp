use std::{borrow::Cow, time::Duration};

const RPC_TIMEOUT: Duration = Duration::from_secs(20);

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use xrpl::{
    asynch::clients::{
        AsyncJsonRpcClient, AsyncWebSocketClient, MultiExecutorMutex, XRPLAsyncWebsocketIO,
        XRPLClient,
    },
    models::requests::subscribe::{StreamParameter, Subscribe},
};

use crate::{
    action::Action,
    cli::Cmd,
    config::FALLBACK_CURRENCY_CODE,
    network::Network,
    signing::{self, SigningConfig, prompt_mainnet_confirmation},
};
use secrecy::ExposeSecret;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerInfoSummary {
    pub ledger_index: u32,
    pub hostid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeeSummary {
    pub open_ledger_fee_drops: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountSummary {
    pub account: String,
    pub balance_xrp: String,
    pub sequence: u32,
    pub owner_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OfferRow {
    pub quality: String,
    pub price: String,
    pub taker_gets: String,
    pub taker_pays: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TxSummary {
    pub hash: String,
}

/// NFToken mint flag: URI may be updated via `NFTokenModify` (dynamic NFT / dNFT).
pub const NFTOKEN_FLAG_MUTABLE: u32 = 0x0000_0010;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NftRow {
    pub nft_id: String,
    pub taxon: u32,
    pub serial: u32,
    pub transfer_fee: u16,
    pub uri: String,
    /// `true` when `Flags` includes [`NFTOKEN_FLAG_MUTABLE`] (tfMutable).
    #[serde(default)]
    pub is_mutable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustLineRow {
    pub currency: String,
    pub account: String,
    pub balance: String,
    pub limit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AmmSummary {
    pub asset1: String,
    pub asset2: String,
    pub lp_token: String,
    pub trading_fee: u16,
    pub pool1: String,
    pub pool2: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct XrplRlusdPrice {
    pub bid: String,
    pub ask: String,
    pub mid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TxRow {
    pub hash: String,
    pub tx_type: String,
    pub ledger_index: u32,
    pub result: String,
}

/// One row from `account_objects` (Check, Ticket, MPToken, PayChannel, Escrow, …).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerObjectRow {
    pub ledger_type: String,
    pub index: String,
    pub detail: String,
}

/// True for ledger types shown in the upper «misc objects» panel (excludes PayChannel / Escrow).
#[must_use]
pub fn is_objects_tab_ledger_type(t: &str) -> bool {
    matches!(
        t,
        "Check"
            | "Ticket"
            | "MPToken"
            | "MPTokenIssuance"
            | "DepositPreauth"
            | "SignerList"
            | "DID"
    )
}

#[must_use]
pub fn is_pay_channel_type(t: &str) -> bool {
    t == "PayChannel"
}

#[must_use]
pub fn is_escrow_type(t: &str) -> bool {
    t == "Escrow"
}

#[derive(Debug, Clone)]
pub struct BookPair {
    pub base: String,
    pub quote: String,
    pub quote_code: String,
    pub issuer: String,
    pub limit: u16,
}

impl BookPair {
    /// Base currency is what the taker gets (offer maker pays).
    pub fn gets_currency(&self) -> &str {
        &self.base
    }
    pub fn gets_issuer(&self) -> Option<&str> {
        if self.base.eq_ignore_ascii_case("XRP") {
            None
        } else {
            Some(&self.issuer)
        }
    }
    /// Quote currency is what the taker pays (offer maker gets).
    pub fn pays_currency(&self) -> &str {
        if self.quote.eq_ignore_ascii_case("XRP") {
            &self.quote
        } else if self.quote_code.trim().is_empty() {
            FALLBACK_CURRENCY_CODE
        } else {
            &self.quote_code
        }
    }
    pub fn pays_issuer(&self) -> Option<&str> {
        if self.quote.eq_ignore_ascii_case("XRP") {
            None
        } else {
            Some(&self.issuer)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSetSubmitParams {
    /// Uppercase flag name (`RequireDest`, `DefaultRipple`, …) or empty / `(none)`.
    pub set_flag: Option<String>,
    pub clear_flag: Option<String>,
    pub domain_ascii: String,
    pub tick_size: String,
    pub transfer_rate: String,
    /// From CLI `--yes` via Wallet panel.
    pub skip_mainnet_prompt: bool,
    pub config_seed: Option<String>,
}

/// XRP Payment from the Wallet modal (classic or X-destination).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentSubmitParams {
    pub destination: String,
    pub amount_xrp: String,
    pub skip_mainnet_prompt: bool,
    pub config_seed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollCommand {
    Account,
    Book,
    Nfts,
    Lines,
    TxHistory,
    /// `account_objects` (limit 200); UI filters by ledger type per tab.
    LedgerObjects,
    /// Sign and submit AccountSet (wallet form).
    AccountSetSubmit(AccountSetSubmitParams),
    /// Sign and submit Payment (wallet modal).
    PaymentSubmit(PaymentSubmitParams),
}

pub struct RpcClient {
    client: AsyncJsonRpcClient,
    http: reqwest::Client,
}

impl RpcClient {
    pub fn connect(rpc_url: &str) -> color_eyre::Result<Self> {
        Ok(Self {
            client: AsyncJsonRpcClient::connect(rpc_url.parse()?),
            http: reqwest::Client::builder().http1_only().build()?,
        })
    }

    async fn rpc_value(&self, method: &str, params: Value) -> color_eyre::Result<Value> {
        let url = self.client.get_host().to_string();
        let mut last_error = None;
        for attempt in 0..3 {
            let req_json = json!({ "method": method, "params": [params.clone()] });
            let result = async {
                let resp =
                    tokio::time::timeout(RPC_TIMEOUT, self.http.post(&url).json(&req_json).send())
                        .await
                        .map_err(|_| color_eyre::eyre::eyre!("{method} timeout"))?
                        .map_err(|e| color_eyre::eyre::eyre!("{method} request error: {e}"))?;
                let text = tokio::time::timeout(RPC_TIMEOUT, resp.text())
                    .await
                    .map_err(|_| color_eyre::eyre::eyre!("{method} response timeout"))?
                    .map_err(|e| color_eyre::eyre::eyre!("{method} response error: {e}"))?;
                serde_json::from_str::<Value>(&text).map_err(|e| {
                    let preview: String = text.chars().take(120).collect();
                    color_eyre::eyre::eyre!("{method} JSON parse error: {e}; body={preview:?}")
                })
            }
            .await;

            match result {
                Ok(value) => {
                    check_xrpl_error(&value)?;
                    return Ok(value);
                }
                Err(e) if attempt < 2 => {
                    let message = format!("{e}");
                    tracing::warn!("{method} attempt {} failed: {message}", attempt + 1);
                    last_error = Some(e);
                    let delay = if message.contains("Rate limited") {
                        Duration::from_secs(2 * (attempt + 1) as u64)
                    } else {
                        Duration::from_millis(100 * (attempt + 1) as u64)
                    };
                    tokio::time::sleep(delay).await;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_error.unwrap_or_else(|| color_eyre::eyre::eyre!("{method} failed")))
    }

    pub async fn server_info(&self) -> color_eyre::Result<ServerInfoSummary> {
        let value = self.rpc_value("server_info", json!({})).await?;
        Ok(parse_server_info_value(&value))
    }

    pub async fn fee(&self) -> color_eyre::Result<FeeSummary> {
        let value = self.rpc_value("fee", json!({})).await?;
        Ok(parse_fee_value(&value))
    }

    pub async fn account_info(&self, watch_address: &str) -> color_eyre::Result<AccountSummary> {
        let value = self
            .rpc_value("account_info", json!({ "account": watch_address }))
            .await?;
        Ok(AccountSummary {
            account: json_str(&value, &["result", "account_data", "Account"]).to_string(),
            balance_xrp: drops_to_xrp(json_str(&value, &["result", "account_data", "Balance"])),
            sequence: extract_json_u32(&value, &["result", "account_data", "Sequence"]),
            owner_count: extract_json_u32(&value, &["result", "account_data", "OwnerCount"]),
        })
    }

    pub async fn account_nfts(&self, watch_address: &str) -> color_eyre::Result<Vec<NftRow>> {
        let value = match self
            .rpc_value(
                "account_nfts",
                json!({ "account": watch_address, "limit": 100 }),
            )
            .await
        {
            Ok(value) => value,
            Err(e) if is_not_found_error(&format!("{e}")) => return Ok(vec![]),
            Err(e) => return Err(e),
        };
        Ok(parse_account_nfts_value(&value))
    }

    pub async fn account_lines(
        &self,
        watch_address: &str,
    ) -> color_eyre::Result<Vec<TrustLineRow>> {
        let value = match self
            .rpc_value("account_lines", json!({ "account": watch_address }))
            .await
        {
            Ok(value) => value,
            Err(e) if is_not_found_error(&format!("{e}")) => return Ok(vec![]),
            Err(e) => return Err(e),
        };
        Ok(parse_account_lines_value(&value))
    }

    pub async fn amm_info(
        &self,
        asset1_currency: &str,
        asset1_issuer: Option<&str>,
        asset2_currency: &str,
        asset2_issuer: Option<&str>,
    ) -> color_eyre::Result<AmmSummary> {
        let value = self
            .rpc_value(
                "amm_info",
                json!({
                    "asset": book_currency(asset1_currency, asset1_issuer),
                    "asset2": book_currency(asset2_currency, asset2_issuer)
                }),
            )
            .await?;
        Ok(parse_amm_info_value(&value))
    }

    pub async fn account_tx(
        &self,
        watch_address: &str,
        limit: u32,
    ) -> color_eyre::Result<Vec<TxRow>> {
        // Bypass xrpl-rust AccountTx deserialization bug (tx_json vs tx field).
        let value = match self
            .rpc_value(
                "account_tx",
                json!({ "account": watch_address, "limit": limit }),
            )
            .await
        {
            Ok(value) => value,
            Err(e) if is_not_found_error(&format!("{e}")) => return Ok(vec![]),
            Err(e) => return Err(e),
        };
        Ok(parse_account_tx_value(&value))
    }

    pub async fn account_overview(
        &self,
        watch_address: &str,
    ) -> color_eyre::Result<(Option<AccountSummary>, Vec<TxRow>)> {
        let (info_res, tx_res) = tokio::join!(
            tokio::time::timeout(RPC_TIMEOUT, self.account_info(watch_address)),
            tokio::time::timeout(RPC_TIMEOUT, self.account_tx(watch_address, 20)),
        );

        let account = match info_res {
            Ok(Ok(acc)) => Some(acc),
            Ok(Err(e)) => {
                let msg = format!("{e}");
                if is_not_found_error(&msg) {
                    None
                } else {
                    return Err(e);
                }
            }
            Err(_) => return Err(color_eyre::eyre::eyre!("account_info timeout")),
        };

        let txs = match tx_res {
            Ok(Ok(txs)) => txs,
            Ok(Err(e)) => {
                let msg = format!("{e}");
                if is_not_found_error(&msg) {
                    vec![]
                } else {
                    return Err(e);
                }
            }
            Err(_) => return Err(color_eyre::eyre::eyre!("account_tx timeout")),
        };

        Ok((account, txs))
    }

    /// All `account_objects` for the account (no type filter); caller filters by `LedgerEntryType`.
    pub async fn account_objects(
        &self,
        watch_address: &str,
    ) -> color_eyre::Result<Vec<LedgerObjectRow>> {
        let value = match self
            .rpc_value(
                "account_objects",
                json!({ "account": watch_address, "limit": 200 }),
            )
            .await
        {
            Ok(value) => value,
            Err(e) if is_not_found_error(&format!("{e}")) => return Ok(vec![]),
            Err(e) => return Err(e),
        };
        Ok(parse_account_objects_value(&value))
    }

    pub async fn book_offers(
        &self,
        taker_gets_currency: &str,
        taker_gets_issuer: Option<&str>,
        taker_pays_currency: &str,
        taker_pays_issuer: Option<&str>,
        limit: u16,
    ) -> color_eyre::Result<Vec<OfferRow>> {
        let value = match self
            .rpc_value(
                "book_offers",
                json!({
                    "taker_gets": book_currency(taker_gets_currency, taker_gets_issuer),
                    "taker_pays": book_currency(taker_pays_currency, taker_pays_issuer),
                    "limit": limit
                }),
            )
            .await
        {
            Ok(value) => value,
            Err(e) if is_not_found_error(&format!("{e}")) => return Ok(vec![]),
            Err(e) => return Err(e),
        };
        Ok(parse_book_offers_value(&value))
    }

    pub async fn xrp_rlusd_price(
        &self,
        rlusd_currency: &str,
        rlusd_issuer: &str,
    ) -> color_eyre::Result<XrplRlusdPrice> {
        // Bid: taker gets XRP, pays RLUSD → how much RLUSD per 1 XRP
        let bid_params = json!({
            "taker_gets": book_currency("XRP", None),
            "taker_pays": book_currency(rlusd_currency, Some(rlusd_issuer)),
            "limit": 1
        });
        // Ask: taker gets RLUSD, pays XRP → inverse to get RLUSD per XRP
        let ask_params = json!({
            "taker_gets": book_currency(rlusd_currency, Some(rlusd_issuer)),
            "taker_pays": book_currency("XRP", None),
            "limit": 1
        });

        let (bid_resp, ask_resp) = tokio::join!(
            self.rpc_value("book_offers", bid_params),
            self.rpc_value("book_offers", ask_params),
        );

        let bid_value = match bid_resp {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!("book_offers bid error: {e}");
                Value::Null
            }
        };
        let ask_value = match ask_resp {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!("book_offers ask error: {e}");
                Value::Null
            }
        };

        let bid_price = book_offer_best_price(&bid_value, false);
        let ask_price = book_offer_best_price(&ask_value, true);

        let bid_str = bid_price
            .map(|p| format!("{:.4}", p))
            .unwrap_or_else(|| "-".into());
        let ask_str = ask_price
            .map(|p| format!("{:.4}", p))
            .unwrap_or_else(|| "-".into());
        let mid_str = match (bid_price, ask_price) {
            (Some(b), Some(a)) => format!("{:.4}", (b + a) / 2.0),
            (Some(p), None) | (None, Some(p)) => format!("{:.4}", p),
            _ => "-".into(),
        };

        Ok(XrplRlusdPrice {
            bid: bid_str,
            ask: ask_str,
            mid: mid_str,
        })
    }

    /// Check if account is activated (has XRP balance >= 10 XRP)
    pub async fn is_account_activated(&self, address: &str) -> color_eyre::Result<bool> {
        match self.account_info(address).await {
            Ok(account) => Ok(account.balance_xrp.parse::<f64>().unwrap_or(0.0) >= 10.0),
            Err(e) if is_not_found_error(&format!("{e}")) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Submit a signed transaction blob to the network.
    pub async fn submit_signed_tx(&self, signed_tx_blob: &str) -> color_eyre::Result<TxSummary> {
        let value = self
            .rpc_value("submit", json!({ "tx_blob": signed_tx_blob }))
            .await?;
        parse_submit_success(&value)
    }
}

/// Check if XRPL response contains an error and return it with code.
fn check_xrpl_error(value: &Value) -> color_eyre::Result<()> {
    if let Some(result) = value.get("result")
        && let Some(error) = result.get("error").and_then(Value::as_str)
    {
        let code = result
            .get("error_code")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let msg = result
            .get("error_message")
            .and_then(Value::as_str)
            .unwrap_or(error);
        let kind = if is_not_found_error(error) {
            "NOT_FOUND".to_string()
        } else {
            code.to_string()
        };
        return Err(color_eyre::eyre::eyre!("[XRPL-{kind}] {msg}"));
    }
    Ok(())
}

fn parse_submit_success(value: &Value) -> color_eyre::Result<TxSummary> {
    let result = value.get("result").unwrap_or(&Value::Null);
    let engine_result = result
        .get("engine_result")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if engine_result != "tesSUCCESS" {
        let message = result
            .get("engine_result_message")
            .and_then(Value::as_str)
            .unwrap_or("transaction submission failed");
        return Err(color_eyre::eyre::eyre!(
            "submit failed: {engine_result} {message}"
        ));
    }

    let tx_hash = result
        .get("tx_json")
        .and_then(|v| v.get("hash"))
        .and_then(Value::as_str)
        .filter(|hash| !hash.is_empty())
        .ok_or_else(|| color_eyre::eyre::eyre!("submit response missing transaction hash"))?
        .to_string();

    Ok(TxSummary { hash: tx_hash })
}

fn parse_server_info_value(value: &Value) -> ServerInfoSummary {
    ServerInfoSummary {
        ledger_index: extract_json_u32(value, &["result", "info", "validated_ledger", "seq"]),
        hostid: json_str(value, &["result", "info", "hostid"]).to_string(),
    }
}

fn parse_fee_value(value: &Value) -> FeeSummary {
    FeeSummary {
        open_ledger_fee_drops: extract_json_u32(value, &["result", "drops", "open_ledger_fee"]),
    }
}

fn parse_account_nfts_value(value: &Value) -> Vec<NftRow> {
    let nfts = value
        .get("result")
        .and_then(|v| v.get("account_nfts"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    nfts.into_iter()
        .map(|n| {
            let flags = extract_json_u32(&n, &["Flags"]);
            NftRow {
                nft_id: json_str(&n, &["NFTokenID"]).to_string(),
                taxon: extract_json_u32(&n, &["NFTokenTaxon"]),
                serial: extract_json_u32(&n, &["nft_serial"]),
                transfer_fee: extract_json_u32(&n, &["TransferFee"]) as u16,
                uri: decode_uri(json_str(&n, &["URI"])),
                is_mutable: (flags & NFTOKEN_FLAG_MUTABLE) != 0,
            }
        })
        .collect()
}

fn parse_account_lines_value(value: &Value) -> Vec<TrustLineRow> {
    let lines = value
        .get("result")
        .and_then(|v| v.get("lines"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    lines
        .into_iter()
        .map(|l| TrustLineRow {
            currency: json_str(&l, &["currency"]).to_string(),
            account: json_str(&l, &["account"]).to_string(),
            balance: json_str(&l, &["balance"]).to_string(),
            limit: json_str(&l, &["limit"]).to_string(),
        })
        .collect()
}

fn parse_amm_info_value(value: &Value) -> AmmSummary {
    let amm = value
        .get("result")
        .and_then(|v| v.get("amm"))
        .cloned()
        .unwrap_or_default();
    AmmSummary {
        asset1: format_asset(amm.get("Asset")),
        asset2: format_asset(amm.get("Asset2")),
        lp_token: format!(
            "{} {}",
            amm.get("LPToken")
                .and_then(|t| t.get("value"))
                .and_then(Value::as_str)
                .unwrap_or("0"),
            amm.get("LPToken")
                .and_then(|t| t.get("currency"))
                .and_then(Value::as_str)
                .unwrap_or("?")
        ),
        trading_fee: amm.get("TradingFee").and_then(Value::as_u64).unwrap_or(0) as u16,
        pool1: format_amount(amm.get("Amount")),
        pool2: format_amount(amm.get("Amount2")),
    }
}

fn parse_account_tx_value(value: &Value) -> Vec<TxRow> {
    let txns = value
        .get("result")
        .and_then(|v| v.get("transactions"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    txns.into_iter()
        .map(|t| {
            let tx = t
                .get("tx")
                .or_else(|| t.get("tx_json"))
                .cloned()
                .unwrap_or_default();
            TxRow {
                hash: t
                    .get("hash")
                    .and_then(Value::as_str)
                    .or_else(|| tx.get("hash").and_then(Value::as_str))
                    .unwrap_or("-")
                    .to_string(),
                tx_type: tx
                    .get("TransactionType")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .to_string(),
                ledger_index: t
                    .get("ledger_index")
                    .and_then(Value::as_u64)
                    .or_else(|| tx.get("ledger_index").and_then(Value::as_u64))
                    .unwrap_or(0) as u32,
                result: t
                    .get("meta")
                    .and_then(|m| m.get("TransactionResult"))
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .to_string(),
            }
        })
        .collect()
}

fn parse_book_offers_value(value: &Value) -> Vec<OfferRow> {
    let offers = value
        .get("result")
        .and_then(|v| v.get("offers"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    offers
        .into_iter()
        .map(|offer| {
            let quality = offer
                .get("quality")
                .and_then(Value::as_str)
                .unwrap_or("-")
                .to_string();
            // quality * 1_000_000 = price per 1 XRP in currency units
            let price = if let Ok(q) = quality.parse::<f64>() {
                format!("{:.6}", q * 1_000_000.0)
            } else {
                "-".to_string()
            };
            OfferRow {
                quality,
                price,
                taker_gets: format_amount(offer.get("TakerGets")),
                taker_pays: format_amount(offer.get("TakerPays")),
            }
        })
        .collect()
}

fn is_not_found_error(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("actnotfound")
        || lower.contains("actmalformed")
        || lower.contains("entrynotfound")
        || lower.contains("lgrnotfound")
        || lower.contains("object not found")
        || lower.contains("bad issuer")
        || lower.contains("account not found")
}

/// Extract best price from book_offers response.
/// `invert=true` for asks (taker gets issued, pays XRP) to convert XRP-per-currency → currency-per-XRP.
fn book_offer_best_price(value: &Value, invert: bool) -> Option<f64> {
    let offers = value
        .get("result")
        .and_then(|v| v.get("offers"))
        .and_then(Value::as_array)?;
    let first = offers.first()?;
    let quality = first
        .get("quality")
        .and_then(|v| v.as_str().and_then(|s| s.parse::<f64>().ok()))
        .or_else(|| first.get("quality").and_then(Value::as_f64))?;
    let price = quality * 1_000_000.0;
    if invert {
        Some(1.0 / price)
    } else {
        Some(price)
    }
}

pub fn start_ws_task(
    ws_url: String,
    watch_address: Option<String>,
    action_tx: UnboundedSender<Action>,
    poll_trigger_tx: UnboundedSender<()>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run_ws_loop(
        ws_url,
        watch_address,
        action_tx,
        poll_trigger_tx,
        cancel,
    ))
}

fn next_backoff(current: u64) -> u64 {
    if current == 0 {
        2
    } else {
        (current * 2).min(60)
    }
}

async fn run_ws_loop(
    ws_url: String,
    watch_address: Option<String>,
    action_tx: UnboundedSender<Action>,
    poll_trigger_tx: UnboundedSender<()>,
    cancel: CancellationToken,
) {
    let mut backoff_secs: u64 = 0;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {
                match connect_and_subscribe(&ws_url, &watch_address, &action_tx, &poll_trigger_tx, &cancel).await {
                    Ok(()) => return,
                    Err(e) => {
                        tracing::error!("ws error: {e}");
                        backoff_secs = next_backoff(backoff_secs);
                    }
                }
            }
        }
    }
}

async fn connect_and_subscribe(
    ws_url: &str,
    watch_address: &Option<String>,
    action_tx: &UnboundedSender<Action>,
    poll_trigger_tx: &UnboundedSender<()>,
    cancel: &CancellationToken,
) -> color_eyre::Result<()> {
    let parsed_url = ws_url
        .parse()
        .map_err(|e| color_eyre::eyre::eyre!("invalid ws url: {e}"))?;
    let mut ws: AsyncWebSocketClient<MultiExecutorMutex, _> =
        AsyncWebSocketClient::open(parsed_url).await?;

    let accounts = watch_address
        .as_deref()
        .map(|a| vec![Cow::Owned(a.to_string())])
        .unwrap_or_default();
    let sub = Subscribe::new(
        None,
        Some(accounts),
        None,
        None,
        Some(vec![StreamParameter::Ledger]),
        None,
        None,
        None,
    );
    ws.xrpl_send(sub.into()).await?;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            msg = ws.xrpl_receive() => {
                match msg {
                    Ok(Some(ws_msg)) => {
                        let value = serde_json::to_value(ws_msg)?;
                        let event_type = value.get("type").and_then(Value::as_str).unwrap_or_default();
                        if event_type == "ledgerClosed" {
                            let _ = action_tx.send(Action::XrplLedgerClose {
                                ledger_index: extract_json_u32(&value, &["ledger_index"]),
                                base_fee: extract_json_u32(&value, &["fee_base"]),
                                reserve_base: extract_json_u32(&value, &["reserve_base"]),
                                reserve_inc: extract_json_u32(&value, &["reserve_inc"]),
                            });
                            let _ = poll_trigger_tx.send(());
                        } else if value.get("transaction").is_some() {
                            let hash = value
                                .get("transaction")
                                .and_then(|v| v.get("hash"))
                                .and_then(Value::as_str)
                                .unwrap_or("-")
                                .to_string();
                            let _ = action_tx.send(Action::XrplAccountTx(Box::new(TxSummary { hash })));
                        }
                    }
                    Ok(None) => {
                        tracing::warn!("ws stream ended");
                        return Err(color_eyre::eyre::eyre!("websocket closed"));
                    }
                    Err(e) => {
                        tracing::error!("ws receive error: {e}");
                        return Err(e.into());
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct PollContext {
    pub rpc_url: String,
    pub watch_address: String,
    pub book_pair: BookPair,
    pub poll_interval: Duration,
    pub seed_address: Option<String>,
    pub network_watch: watch::Receiver<Network>,
}

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

async fn poll_batch(
    rpc: &RpcClient,
    watch_address: &str,
    book_pair: &BookPair,
    action_tx: &UnboundedSender<Action>,
) -> bool {
    let (r_srv, r_fee, r_acc, r_book) = tokio::join!(
        tokio::time::timeout(RPC_TIMEOUT, rpc.server_info()),
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
    );
    // Delay between batches to avoid rate-limiting public nodes
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (r_nfts, r_lines, r_tx) = tokio::join!(
        tokio::time::timeout(RPC_TIMEOUT, rpc.account_nfts(watch_address)),
        tokio::time::timeout(RPC_TIMEOUT, rpc.account_lines(watch_address)),
        tokio::time::timeout(RPC_TIMEOUT, rpc.account_tx(watch_address, 20)),
    );
    let mut any_ok = false;
    macro_rules! dispatch {
        ($result:expr, $ok_action:expr, $label:literal) => {
            match $result {
                Ok(Ok(v)) => {
                    any_ok = true;
                    let _ = action_tx.send($ok_action(v));
                }
                Ok(Err(e)) => {
                    let _ = action_tx.send(Action::XrplError(format!("{}: {e}", $label)));
                }
                Err(_) => {
                    let _ = action_tx.send(Action::XrplError(format!("{}: timeout", $label)));
                }
            }
        };
    }
    dispatch!(
        r_srv,
        |v| Action::XrplServerInfo(Box::new(v)),
        "server_info"
    );
    dispatch!(r_fee, Action::XrplFee, "fee");
    dispatch!(r_acc, |v| Action::XrplAccount(Box::new(v)), "account_info");
    dispatch!(r_book, Action::XrplBookOffers, "book_offers");
    dispatch!(r_nfts, Action::XrplAccountNfts, "account_nfts");
    dispatch!(r_lines, Action::XrplTrustLines, "account_lines");
    match r_tx {
        Ok(Ok(v)) => {
            any_ok = true;
            let _ = action_tx.send(Action::XrplTxHistory(v));
        }
        Ok(Err(e)) => {
            let msg = format!("account_tx: {e}");
            if is_not_found_error(&msg) {
                any_ok = true;
                let _ = action_tx.send(Action::XrplTxHistory(vec![]));
            } else {
                let _ = action_tx.send(Action::XrplError(msg));
            }
        }
        Err(_) => {
            let _ = action_tx.send(Action::XrplError("account_tx: timeout".into()));
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
        Ok(Ok((acc, txs))) => {
            let _ = action_tx.send(Action::XrplWalletOverview(acc, txs));
            true
        }
        Ok(Err(e)) => {
            let _ = action_tx.send(Action::XrplError(format!("wallet_overview: {e}")));
            false
        }
        Err(_) => {
            let _ = action_tx.send(Action::XrplError("wallet_overview: timeout".into()));
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
        let _ = action_tx.send(Action::AccountSetSubmitErr(
            "nothing to change — pick a flag and/or fill domain, tick size, transfer rate".into(),
        ));
        return;
    }
    if network.is_mainnet() && !params.skip_mainnet_prompt {
        let _ = action_tx.send(Action::AccountSetSubmitErr(
            "mainnet: restart lazyxrp with --yes to allow AccountSet writes".into(),
        ));
        return;
    }
    let signing_config = SigningConfig::prime_seed_source(params.config_seed.clone());
    let Some(seed) = signing_config.seed.as_ref() else {
        let _ = action_tx.send(Action::AccountSetSubmitErr(
            "no signing seed — set XRPL_SEED or config [xrpl.signing] seed".into(),
        ));
        return;
    };
    let wallet = match signing::wallet_from_family_seed(seed.expose_secret(), 0) {
        Ok(w) => w,
        Err(e) => {
            let _ = action_tx.send(Action::AccountSetSubmitErr(format!("wallet: {e:?}")));
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
                let _ = action_tx.send(Action::AccountSetSubmitErr(
                    "tick size: invalid number (use 0 or 3–15)".into(),
                ));
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
                let _ = action_tx.send(Action::AccountSetSubmitErr(
                    "transfer rate: invalid number".into(),
                ));
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
            let _ = action_tx.send(Action::AccountSetSubmitErr(format!("account_info: {e}")));
            return;
        }
        Err(_) => {
            let _ = action_tx.send(Action::AccountSetSubmitErr("account_info: timeout".into()));
            return;
        }
    };

    let fee_info = match tokio::time::timeout(RPC_TIMEOUT, rpc.fee()).await {
        Ok(Ok(f)) => f,
        Ok(Err(e)) => {
            let _ = action_tx.send(Action::AccountSetSubmitErr(format!("fee: {e}")));
            return;
        }
        Err(_) => {
            let _ = action_tx.send(Action::AccountSetSubmitErr("fee: timeout".into()));
            return;
        }
    };

    let server_info = match tokio::time::timeout(RPC_TIMEOUT, rpc.server_info()).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            let _ = action_tx.send(Action::AccountSetSubmitErr(format!("server_info: {e}")));
            return;
        }
        Err(_) => {
            let _ = action_tx.send(Action::AccountSetSubmitErr("server_info: timeout".into()));
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
            let _ = action_tx.send(Action::AccountSetSubmitErr(format!("sign: {e}")));
            return;
        }
    };

    match tokio::time::timeout(RPC_TIMEOUT, rpc.submit_signed_tx(&blob)).await {
        Ok(Ok(tx)) => {
            let _ = action_tx.send(Action::AccountSetSubmitOk(tx.hash.clone()));
            let _ = action_tx.send(Action::RefreshAccount);
            let _ = action_tx.send(Action::RefreshTxHistory);
        }
        Ok(Err(e)) => {
            let _ = action_tx.send(Action::AccountSetSubmitErr(format!("submit: {e}")));
        }
        Err(_) => {
            let _ = action_tx.send(Action::AccountSetSubmitErr("submit: timeout".into()));
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
    if params.amount_xrp.trim().is_empty() {
        let _ = action_tx.send(Action::PaymentSubmitErr(
            "amount is empty — enter XRP to send".into(),
        ));
        return;
    }
    let destination_resolved = match resolve_wallet_payment_destination(params.destination.trim()) {
        Ok(d) => d,
        Err(e) => {
            let _ = action_tx.send(Action::PaymentSubmitErr(format!("{e}")));
            return;
        }
    };
    if network.is_mainnet() && !params.skip_mainnet_prompt {
        let _ = action_tx.send(Action::PaymentSubmitErr(
            "mainnet: restart lazyxrp with --yes to allow Payment writes".into(),
        ));
        return;
    }
    let signing_config = SigningConfig::prime_seed_source(params.config_seed.clone());
    let Some(seed) = signing_config.seed.as_ref() else {
        let _ = action_tx.send(Action::PaymentSubmitErr(
            "no signing seed — set XRPL_SEED or config [xrpl.signing] seed".into(),
        ));
        return;
    };
    let wallet = match signing::wallet_from_family_seed(seed.expose_secret(), 0) {
        Ok(w) => w,
        Err(e) => {
            let _ = action_tx.send(Action::PaymentSubmitErr(format!("wallet: {e:?}")));
            return;
        }
    };
    let account = wallet.classic_address.clone();
    if account == destination_resolved {
        let _ = action_tx.send(Action::PaymentSubmitErr(
            "destination matches source account".into(),
        ));
        return;
    }

    let amount_drops = match xrp_to_drops(params.amount_xrp.trim()) {
        Ok(d) => d,
        Err(e) => {
            let _ = action_tx.send(Action::PaymentSubmitErr(format!("amount: {e}")));
            return;
        }
    };
    if amount_drops == 0 {
        let _ = action_tx.send(Action::PaymentSubmitErr(
            "amount must be greater than zero".into(),
        ));
        return;
    }

    let account_info = match tokio::time::timeout(RPC_TIMEOUT, rpc.account_info(&account)).await {
        Ok(Ok(a)) => a,
        Ok(Err(e)) => {
            let _ = action_tx.send(Action::PaymentSubmitErr(format!("account_info: {e}")));
            return;
        }
        Err(_) => {
            let _ = action_tx.send(Action::PaymentSubmitErr("account_info: timeout".into()));
            return;
        }
    };

    let balance_drops = xrp_to_drops(&account_info.balance_xrp).unwrap_or(0);
    let fee_info = match tokio::time::timeout(RPC_TIMEOUT, rpc.fee()).await {
        Ok(Ok(f)) => f,
        Ok(Err(e)) => {
            let _ = action_tx.send(Action::PaymentSubmitErr(format!("fee: {e}")));
            return;
        }
        Err(_) => {
            let _ = action_tx.send(Action::PaymentSubmitErr("fee: timeout".into()));
            return;
        }
    };
    let fee_drops = fee_info.open_ledger_fee_drops;
    if balance_drops < amount_drops + u64::from(fee_drops) {
        let total_need = amount_drops.saturating_add(u64::from(fee_drops));
        let _ = action_tx.send(Action::PaymentSubmitErr(format!(
            "insufficient balance: have {balance_drops} drops, need {total_need} (amount {amount_drops} + fee {fee_drops})"
        )));
        return;
    }

    let server_info = match tokio::time::timeout(RPC_TIMEOUT, rpc.server_info()).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            let _ = action_tx.send(Action::PaymentSubmitErr(format!("server_info: {e}")));
            return;
        }
        Err(_) => {
            let _ = action_tx.send(Action::PaymentSubmitErr("server_info: timeout".into()));
            return;
        }
    };
    let last_ledger_sequence = server_info.ledger_index.saturating_add(20);

    let blob = match signing::create_and_sign_payment(
        seed,
        &account,
        &destination_resolved,
        params.amount_xrp.trim(),
        account_info.sequence,
        fee_drops,
        last_ledger_sequence,
        network,
    ) {
        Ok(b) => b,
        Err(e) => {
            let _ = action_tx.send(Action::PaymentSubmitErr(format!("sign: {e}")));
            return;
        }
    };

    match tokio::time::timeout(RPC_TIMEOUT, rpc.submit_signed_tx(&blob)).await {
        Ok(Ok(tx)) => {
            let _ = action_tx.send(Action::PaymentSubmitOk(tx.hash.clone()));
            let _ = action_tx.send(Action::RefreshAccount);
            let _ = action_tx.send(Action::RefreshTxHistory);
        }
        Ok(Err(e)) => {
            let _ = action_tx.send(Action::PaymentSubmitErr(format!("submit: {e}")));
        }
        Err(_) => {
            let _ = action_tx.send(Action::PaymentSubmitErr("submit: timeout".into()));
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
            let _ = action_tx.send(ok_action(value));
        }
        Ok(Err(e)) => {
            let _ = action_tx.send(Action::XrplError(format!("{label}: {e}")));
        }
        Err(_) => {
            let _ = action_tx.send(Action::XrplError(format!("{label}: timeout")));
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
    } = ctx;
    let rpc = match RpcClient::connect(&rpc_url) {
        Ok(rpc) => rpc,
        Err(err) => {
            let _ = action_tx.send(Action::XrplError(format!("rpc init failed: {err}")));
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
                let batch_ok = poll_batch(&rpc, &watch_address, &book_pair, &action_tx).await;
                if let Some(ref addr) = seed_address {
                    poll_wallet_overview(&rpc, addr, &action_tx).await;
                }
                if batch_ok {
                    backoff_secs = 0;
                } else {
                    backoff_secs = next_backoff(backoff_secs);
                }
                last_poll = Some(std::time::Instant::now());
            }
            Some(()) = poll_trigger_rx.recv() => {
                if let Some(last) = last_poll
                    && last.elapsed() < MIN_POLL_INTERVAL
                {
                    continue;
                }
                if backoff_secs > 0 {
                    tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                }
                let batch_ok = poll_batch(&rpc, &watch_address, &book_pair, &action_tx).await;
                if let Some(ref addr) = seed_address {
                    poll_wallet_overview(&rpc, addr, &action_tx).await;
                }
                if batch_ok {
                    backoff_secs = 0;
                } else {
                    backoff_secs = next_backoff(backoff_secs);
                }
                last_poll = Some(std::time::Instant::now());
            }
            _ = price_tick.tick() => {
                match tokio::time::timeout(RPC_TIMEOUT, rpc.xrp_rlusd_price(book_pair.pays_currency(), &book_pair.issuer)).await {
                    Ok(Ok(p)) => { let _ = action_tx.send(Action::XrplRlusdPrice(p)); }
                    Ok(Err(e)) => { let _ = action_tx.send(Action::XrplError(format!("price: {e}"))); }
                    Err(_) => { let _ = action_tx.send(Action::XrplError("price: timeout".into())); }
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
                        tokio::time::timeout(RPC_TIMEOUT, rpc.account_tx(&watch_address, 20)).await,
                        Action::XrplTxHistory,
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
                }
            }
        }
    }
}

pub async fn execute_cli_command(
    cmd: Cmd,
    rpc_url: &str,
    network: &Network,
    signing_seed: Option<String>,
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
            let info = rpc.server_info().await?;
            let fee = rpc.fee().await?;
            println!("LedgerIndex: {}", info.ledger_index);
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
                serde_json::to_string_pretty(&rpc.account_tx(&address, limit).await?)?
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
            // Load signing config first so we can derive the source address from the seed.
            let signing_config = SigningConfig::prime_seed_source(signing_seed.clone());
            let Some(seed) = signing_config.seed.as_ref() else {
                return Err(color_eyre::eyre::eyre!(
                    "No signing seed: set XRPL_SEED, put seed in config [xrpl.signing] seed, or use --seed (family seed s... or sEd...)."
                ));
            };
            let wallet = signing::wallet_from_family_seed(seed.expose_secret(), 0)
                .map_err(|e| color_eyre::eyre::eyre!(e))?;
            let account = wallet.classic_address.clone();

            // Get account info to check balance and sequence
            let account_info = rpc.account_info(&account).await?;
            let balance_xrp_str = account_info.balance_xrp;
            let balance_drops = xrp_to_drops(&balance_xrp_str).unwrap_or(0);
            let sequence = account_info.sequence;

            println!("From: {}", account);
            println!("To: {}", destination);
            println!("Amount: {} XRP", amount);
            println!("Current Balance: {} XRP", balance_xrp_str);
            println!("Account Sequence: {}", sequence);

            let amount_drops = xrp_to_drops(&amount)?;
            // Add 10 drops for basic fee buffer
            if balance_drops < amount_drops + 10 {
                return Err(color_eyre::eyre::eyre!(
                    "Insufficient balance: current {} drops, need {} drops",
                    balance_drops,
                    amount_drops + 10
                ));
            }

            // Get current fee and ledger index
            let fee_info = rpc.fee().await?;
            let fee_drops = fee_info.open_ledger_fee_drops;
            let server_info = rpc.server_info().await?;
            let last_ledger_sequence = server_info.ledger_index + 20;

            // Prompt for mainnet confirmation
            if !prompt_mainnet_confirmation(
                &format!("Send {} XRP to {}", amount, destination),
                network,
                false,
            ) {
                println!("Transaction cancelled by user.");
                return Ok(());
            }

            // Create and sign the transaction
            match signing::create_and_sign_payment(
                seed,
                &account,
                &destination,
                &amount,
                sequence,
                fee_drops,
                last_ledger_sequence,
                network,
            ) {
                Ok(signed_tx_blob) => {
                    println!("\n=== Transaction Created ===");
                    println!("Signed transaction blob:");
                    println!("{}", signed_tx_blob);

                    // Submit to network
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

/// Build a book_offers Currency parameter.
/// XRP is {currency: "XRP"}; issued currencies need {currency, issuer}.
fn book_currency(currency: &str, issuer: Option<&str>) -> Value {
    if currency.eq_ignore_ascii_case("XRP") {
        json!({ "currency": "XRP" })
    } else {
        json!({
            "currency": currency,
            "issuer": issuer.unwrap_or("")
        })
    }
}

// parse_currency removed — book_currency/amm_info now build JSON directly

fn json_str<'a>(value: &'a Value, path: &[&str]) -> &'a str {
    let mut node = value;
    for key in path {
        node = node.get(*key).unwrap_or(&Value::Null);
    }
    node.as_str().unwrap_or_default()
}

fn extract_json_u32(value: &Value, path: &[&str]) -> u32 {
    let mut node = value;
    for key in path {
        node = node.get(*key).unwrap_or(&Value::Null);
    }
    node.as_u64()
        .or_else(|| node.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or_default() as u32
}

pub fn xrp_to_drops(xrp: &str) -> color_eyre::Result<u64> {
    let parts: Vec<&str> = xrp.split('.').collect();
    match parts.len() {
        1 => {
            let whole: u64 = parts[0].parse()?;
            Ok(whole * 1_000_000)
        }
        2 => {
            let whole: u64 = parts[0].parse()?;
            let frac_str = format!("{:0<6}", parts[1]);
            if frac_str.len() > 6 {
                return Err(color_eyre::eyre::eyre!(
                    "XRP amount can only have up to 6 decimal places"
                ));
            }
            let frac: u64 = frac_str.parse()?;
            Ok(whole * 1_000_000 + frac)
        }
        _ => Err(color_eyre::eyre::eyre!("Invalid XRP amount format")),
    }
}

fn drops_to_xrp(drops: &str) -> String {
    let drops_num = drops.parse::<f64>().unwrap_or_default();
    format!("{:.6}", drops_num / 1_000_000.0)
}

fn decode_uri(hex: &str) -> String {
    if hex.is_empty() {
        return String::new();
    }
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect();
    String::from_utf8(bytes).unwrap_or_else(|_| hex.to_string())
}

fn format_asset(v: Option<&Value>) -> String {
    match v {
        Some(Value::Object(m)) => m
            .get("currency")
            .and_then(Value::as_str)
            .unwrap_or("XRP")
            .to_string(),
        _ => "XRP".to_string(),
    }
}

fn format_amount(value: Option<&Value>) -> String {
    match value {
        Some(v) if v.is_string() => drops_to_xrp(v.as_str().unwrap_or_default()),
        Some(v) => {
            let currency = v.get("currency").and_then(Value::as_str).unwrap_or("?");
            let amount = v.get("value").and_then(Value::as_str).unwrap_or("0");
            format!("{amount} {currency}")
        }
        None => "-".to_string(),
    }
}

fn parse_account_objects_value(value: &Value) -> Vec<LedgerObjectRow> {
    let Some(arr) = value
        .get("result")
        .and_then(|r| r.get("account_objects"))
        .and_then(Value::as_array)
    else {
        return vec![];
    };
    arr.iter().filter_map(parse_one_ledger_object_row).collect()
}

fn parse_one_ledger_object_row(obj: &Value) -> Option<LedgerObjectRow> {
    let ledger_type = obj.get("LedgerEntryType")?.as_str()?.to_string();
    let index = obj
        .get("index")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| obj.get("hash").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    let detail = summarize_ledger_object(obj);
    Some(LedgerObjectRow {
        ledger_type,
        index,
        detail,
    })
}

fn summarize_ledger_object(obj: &Value) -> String {
    let t = json_str(obj, &["LedgerEntryType"]);
    let dest = json_str(obj, &["Destination"]);
    match t {
        "Check" => {
            let amt = format_amount(obj.get("SendMax"));
            if dest.is_empty() {
                format!("Check · {amt}")
            } else {
                format!("→ {dest} · {amt}")
            }
        }
        "Ticket" => format!("seq {}", extract_json_u32(obj, &["TicketSequence"])),
        "MPToken" | "MPTokenIssuance" => {
            let mid = json_str(obj, &["MPTokenIssuanceID"]);
            let raw_amt = obj.get("Amount").map(|v| v.to_string()).unwrap_or_default();
            format!("MPT · {mid} · {raw_amt}")
        }
        "PayChannel" => {
            let amt = format_amount(obj.get("Amount"));
            if dest.is_empty() {
                format!("PayChan · {amt}")
            } else {
                format!("→ {dest} · {amt}")
            }
        }
        "Escrow" => {
            let amt = format_amount(obj.get("Amount"));
            if dest.is_empty() {
                format!("Escrow · {amt}")
            } else {
                format!("→ {dest} · {amt}")
            }
        }
        "DepositPreauth" => {
            let a = json_str(obj, &["Authorize"]);
            format!("auth {a}")
        }
        "SignerList" => format!("quorum {}", extract_json_u32(obj, &["SignerQuorum"])),
        _ => {
            let s = obj.to_string();
            let t = s.chars().take(88).collect::<String>();
            if s.len() > 88 { format!("{t}…") } else { t }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_str_returns_nested_string() {
        let v = json!({"a": {"b": "hello"}});
        assert_eq!(json_str(&v, &["a", "b"]), "hello");
    }

    #[test]
    fn json_str_missing_path_returns_empty() {
        let v = json!({"a": {}});
        assert_eq!(json_str(&v, &["a", "b"]), "");
        assert_eq!(json_str(&v, &["x"]), "");
    }

    #[test]
    fn extract_json_u32_returns_number() {
        let v = json!({"a": 42});
        assert_eq!(extract_json_u32(&v, &["a"]), 42);
    }

    #[test]
    fn extract_json_u32_missing_or_non_numeric_returns_zero() {
        let v = json!({"a": "foo"});
        assert_eq!(extract_json_u32(&v, &["a"]), 0);
        assert_eq!(extract_json_u32(&v, &["x"]), 0);
    }

    #[test]
    fn extract_json_u32_parses_string_number() {
        let v = json!({"a": "42"});
        assert_eq!(extract_json_u32(&v, &["a"]), 42);
    }

    #[test]
    fn drops_to_xrp_basic() {
        assert_eq!(drops_to_xrp("1000000"), "1.000000");
        assert_eq!(drops_to_xrp("250000"), "0.250000");
    }

    #[test]
    fn drops_to_xrp_invalid_returns_zero() {
        assert_eq!(drops_to_xrp("not-a-number"), "0.000000");
    }

    #[test]
    fn format_amount_none() {
        assert_eq!(format_amount(None), "-");
    }

    #[test]
    fn format_amount_xrp_drops_string() {
        let v = json!("1000000");
        assert_eq!(format_amount(Some(&v)), "1.000000");
    }

    #[test]
    fn format_amount_issued_currency() {
        let v = json!({"currency": "USD", "value": "1.5", "issuer": "rXyz"});
        assert_eq!(format_amount(Some(&v)), "1.5 USD");
    }

    #[test]
    fn book_pair_uses_currency_code_for_issued_quote() {
        let pair = BookPair {
            base: "XRP".into(),
            quote: "RLUSD".into(),
            quote_code: "524C555344000000000000000000000000000000".into(),
            issuer: "rIssuer".into(),
            limit: 5,
        };
        assert_eq!(
            pair.pays_currency(),
            "524C555344000000000000000000000000000000"
        );
        assert_eq!(pair.pays_issuer(), Some("rIssuer"));
    }

    /// TC-013
    #[test]
    fn parse_account_nfts_fixture() {
        let v = json!({
            "result": {
                "account_nfts": [{
                    "NFTokenID": "000B013ADCD5",
                    "NFTokenTaxon": 7,
                    "nft_serial": 3,
                    "TransferFee": 100,
                    "URI": "48656C6C6F"
                }]
            }
        });
        let rows = super::parse_account_nfts_value(&v);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].nft_id, "000B013ADCD5");
        assert_eq!(rows[0].taxon, 7);
        assert_eq!(rows[0].serial, 3);
        assert_eq!(rows[0].transfer_fee, 100);
        assert_eq!(rows[0].uri, "Hello");
        assert!(!rows[0].is_mutable);
    }

    /// TC-074: account_nfts — Flags tfMutable (dNFT)
    #[test]
    fn parse_account_nfts_mutable_flag() {
        let v = json!({
            "result": {
                "account_nfts": [{
                    "NFTokenID": "abc",
                    "NFTokenTaxon": 0,
                    "nft_serial": 0,
                    "TransferFee": 0,
                    "URI": "",
                    "Flags": 16
                }]
            }
        });
        let rows = super::parse_account_nfts_value(&v);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_mutable);
    }

    /// TC-014
    #[test]
    fn parse_account_lines_fixture() {
        let v = json!({
            "result": {
                "lines": [{
                    "currency": "USD",
                    "account": "rIssuer1",
                    "balance": "10",
                    "limit": "1000"
                }]
            }
        });
        let rows = super::parse_account_lines_value(&v);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].currency, "USD");
        assert_eq!(rows[0].account, "rIssuer1");
        assert_eq!(rows[0].balance, "10");
        assert_eq!(rows[0].limit, "1000");
    }

    /// TC-015
    #[test]
    fn parse_amm_info_fixture() {
        let v = json!({
            "result": {
                "amm": {
                    "Asset": {"currency": "XRP"},
                    "Asset2": {"currency": "USD", "issuer": "rIssuer2"},
                    "LPToken": {"value": "42", "currency": "03"},
                    "TradingFee": 12,
                    "Amount": "2000000",
                    "Amount2": {"currency": "USD", "value": "3", "issuer": "rIssuer2"}
                }
            }
        });
        let s = super::parse_amm_info_value(&v);
        assert_eq!(s.asset1, "XRP");
        assert_eq!(s.asset2, "USD");
        assert_eq!(s.lp_token, "42 03");
        assert_eq!(s.trading_fee, 12);
        assert_eq!(s.pool1, "2.000000");
        assert_eq!(s.pool2, "3 USD");
    }

    /// TC-016
    #[test]
    fn parse_account_tx_fixture() {
        let v = json!({
            "result": {
                "transactions": [{
                    "hash": "ABC123",
                    "ledger_index": 55,
                    "tx": {"TransactionType": "Payment", "hash": "ABC123"},
                    "meta": {"TransactionResult": "tesSUCCESS"}
                }]
            }
        });
        let rows = super::parse_account_tx_value(&v);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].hash, "ABC123");
        assert_eq!(rows[0].tx_type, "Payment");
        assert_eq!(rows[0].ledger_index, 55);
        assert_eq!(rows[0].result, "tesSUCCESS");
    }

    /// TC-017
    #[test]
    fn parse_book_offers_fixture() {
        let v = json!({
            "result": {
                "offers": [{
                    "quality": "0.5",
                    "TakerGets": "1000000",
                    "TakerPays": {"currency": "USD", "value": "2", "issuer": "rI"}
                }]
            }
        });
        let rows = super::parse_book_offers_value(&v);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].quality, "0.5");
        assert_eq!(rows[0].price, "500000.000000");
        assert_eq!(rows[0].taker_gets, "1.000000");
        assert_eq!(rows[0].taker_pays, "2 USD");
    }

    /// TC-018
    #[test]
    fn parse_server_info_and_fee_fixtures() {
        let si = json!({
            "result": {
                "info": {
                    "validated_ledger": {"seq": 80000000},
                    "hostid": "test-host"
                }
            }
        });
        let sum = super::parse_server_info_value(&si);
        assert_eq!(sum.ledger_index, 80000000);
        assert_eq!(sum.hostid, "test-host");

        let fee = json!({"result": {"drops": {"open_ledger_fee": 15}}});
        let fs = super::parse_fee_value(&fee);
        assert_eq!(fs.open_ledger_fee_drops, 15);
    }

    #[test]
    fn check_xrpl_error_preserves_not_found_as_error() {
        let value = json!({
            "result": {
                "error": "actNotFound",
                "error_code": 19,
                "error_message": "Account not found."
            }
        });
        let err = super::check_xrpl_error(&value).expect_err("not found must not be swallowed");
        assert!(super::is_not_found_error(&format!("{err}")));
    }

    #[test]
    fn parse_submit_success_requires_tes_success_and_hash() {
        let value = json!({
            "result": {
                "engine_result": "tesSUCCESS",
                "tx_json": {"hash": "ABC123"}
            }
        });
        let summary = super::parse_submit_success(&value).expect("submit should parse");
        assert_eq!(summary.hash, "ABC123");

        let failed = json!({
            "result": {
                "engine_result": "tecNO_DST_INSUF_XRP",
                "engine_result_message": "Destination does not exist."
            }
        });
        assert!(super::parse_submit_success(&failed).is_err());
    }

    /// TC-071 account_objects parse (empty)
    #[test]
    fn parse_account_objects_empty() {
        let v = json!({ "result": { "account_objects": [] } });
        assert!(super::parse_account_objects_value(&v).is_empty());
    }

    /// TC-072
    #[test]
    fn parse_account_objects_mixed_types() {
        let v = json!({
            "result": {
                "account_objects": [
                    {
                        "LedgerEntryType": "Check",
                        "index": "CHK1",
                        "Destination": "rDest1",
                        "SendMax": "2000000"
                    },
                    {
                        "LedgerEntryType": "Ticket",
                        "index": "TIK1",
                        "TicketSequence": 7
                    },
                    {
                        "LedgerEntryType": "MPToken",
                        "index": "MPT1",
                        "MPTokenIssuanceID": "001122",
                        "Amount": "99"
                    },
                    {
                        "LedgerEntryType": "PayChannel",
                        "index": "PC1",
                        "Destination": "rDestPC",
                        "Amount": "5000000"
                    },
                    {
                        "LedgerEntryType": "Escrow",
                        "index": "ES1",
                        "Destination": "rDestE",
                        "Amount": "1000000"
                    }
                ]
            }
        });
        let rows = super::parse_account_objects_value(&v);
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].ledger_type, "Check");
        assert_eq!(rows[0].index, "CHK1");
        assert!(rows[0].detail.contains("rDest1"));

        assert_eq!(rows[1].ledger_type, "Ticket");
        assert!(rows[1].detail.contains('7'));

        assert_eq!(rows[3].ledger_type, "PayChannel");
        assert!(rows[3].detail.contains("rDestPC"));

        assert_eq!(rows[4].ledger_type, "Escrow");
    }

    /// TC-073
    #[test]
    fn ledger_object_tab_filters() {
        assert!(super::is_objects_tab_ledger_type("Check"));
        assert!(super::is_objects_tab_ledger_type("Ticket"));
        assert!(super::is_objects_tab_ledger_type("MPToken"));
        assert!(super::is_objects_tab_ledger_type("DID"));
        assert!(!super::is_objects_tab_ledger_type("PayChannel"));
        assert!(super::is_pay_channel_type("PayChannel"));
        assert!(super::is_escrow_type("Escrow"));
    }

    /// Live XRPL JSON-RPC (mainnet public cluster). Serialized to avoid connection pile-up.
    mod integration_live_network {
        use std::time::Duration;

        use super::*;
        use crate::cli::Cmd;

        const RPC: &str = "https://xrplcluster.com";
        const GENESIS: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
        const RLUSD_ISSUER: &str = "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De";
        static LIVE_RPC_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

        async fn run(cmd: Cmd) -> color_eyre::Result<()> {
            let _guard = LIVE_RPC_LOCK.lock().await;
            tokio::time::sleep(Duration::from_millis(250)).await;
            tokio::time::timeout(
                Duration::from_secs(90),
                execute_cli_command(cmd, RPC, &Network::Mainnet, None),
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
