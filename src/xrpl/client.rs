use std::time::Duration;

use serde_json::{Value, json};
use xrpl::asynch::clients::{AsyncJsonRpcClient, XRPLClient};

use super::json_util::{extract_json_u32, json_str};
use super::types::{
    AccountSummary, AccountTxPage, AmmSummary, ArcValue, FeeSummary, LedgerObjectRow,
    NFTOKEN_FLAG_MUTABLE, NftRow, OfferRow, PathAlternative, RipplePathFindResult,
    ServerInfoSummary, SimulateResult, TrustLineRow, TxRow, TxSummary, WalletProposeResult,
    XrplRlusdPrice,
};

pub(crate) const RPC_TIMEOUT: Duration = Duration::from_secs(20);

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
            flags: extract_json_u32(&value, &["result", "account_data", "Flags"]),
            regular_key: value
                .pointer("/result/account_data/RegularKey")
                .and_then(|v| v.as_str())
                .map(String::from),
            domain_hex: value
                .pointer("/result/account_data/Domain")
                .and_then(|v| v.as_str())
                .map(String::from),
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
        marker: Option<serde_json::Value>,
    ) -> color_eyre::Result<AccountTxPage> {
        let mut req = json!({ "account": watch_address, "limit": limit });
        if let Some(m) = marker {
            req["marker"] = m;
        }
        let value = match self.rpc_value("account_tx", req).await {
            Ok(value) => value,
            Err(e) if is_not_found_error(&format!("{e}")) => {
                return Ok(AccountTxPage {
                    rows: vec![],
                    marker: None,
                });
            }
            Err(e) => return Err(e),
        };
        Ok(parse_account_tx_page(&value, watch_address))
    }

    pub async fn account_overview(
        &self,
        watch_address: &str,
    ) -> color_eyre::Result<(
        Option<AccountSummary>,
        Vec<TxRow>,
        Option<serde_json::Value>,
    )> {
        let (info_res, tx_res) = tokio::join!(
            tokio::time::timeout(RPC_TIMEOUT, self.account_info(watch_address)),
            tokio::time::timeout(RPC_TIMEOUT, self.account_tx(watch_address, 20, None)),
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

        let (txs, marker) = match tx_res {
            Ok(Ok(page)) => (page.rows, page.marker),
            Ok(Err(e)) => {
                let msg = format!("{e}");
                if is_not_found_error(&msg) {
                    (vec![], None)
                } else {
                    return Err(e);
                }
            }
            Err(_) => return Err(color_eyre::eyre::eyre!("account_tx timeout")),
        };

        Ok((account, txs, marker))
    }

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
        let bid_params = json!({
            "taker_gets": book_currency("XRP", None),
            "taker_pays": book_currency(rlusd_currency, Some(rlusd_issuer)),
            "limit": 1
        });
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

    pub async fn is_account_activated(&self, address: &str) -> color_eyre::Result<bool> {
        match self.account_info(address).await {
            Ok(account) => Ok(account.balance_xrp.parse::<f64>().unwrap_or(0.0) >= 10.0),
            Err(e) if is_not_found_error(&format!("{e}")) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub async fn submit_signed_tx(&self, signed_tx_blob: &str) -> color_eyre::Result<TxSummary> {
        let value = self
            .rpc_value("submit", json!({ "tx_blob": signed_tx_blob }))
            .await?;
        parse_submit_success(&value)
    }

    /// Dry-run an unsigned transaction via the `simulate` RPC.
    ///
    /// Returns the auto-filled `tx_json` (Fee, Sequence, etc.) and metadata
    /// without committing to the ledger.
    pub async fn simulate_tx(&self, tx_json: Value) -> color_eyre::Result<SimulateResult> {
        let value = self
            .rpc_value("simulate", json!({ "tx_json": tx_json }))
            .await?;
        parse_simulate_result(&value)
    }

    /// `ripple_path_find` – find payment paths between two accounts for a given
    /// destination currency amount.  Use this to preview swap rates before
    /// submitting an IOU payment (especially self-pay → DEX swap).
    ///
    /// `destination_amount` should be an object like
    /// `{"currency":"USD","issuer":"r...","value":"100"}` or
    /// a numeric-string for XRP drops (`.0` appended to canonicalise drops).
    pub async fn ripple_path_find(
        &self,
        source_account: &str,
        destination_account: &str,
        destination_amount: &Value,
    ) -> color_eyre::Result<RipplePathFindResult> {
        let value = self
            .rpc_value(
                "ripple_path_find",
                json!({
                    "source_account": source_account,
                    "destination_account": destination_account,
                    "destination_amount": destination_amount,
                }),
            )
            .await?;
        parse_ripple_path_find(&value)
    }

    /// Generate a new XRPL wallet via `wallet_propose`.
    pub async fn wallet_propose(&self, key_type: &str) -> color_eyre::Result<WalletProposeResult> {
        let params = json!({ "key_type": key_type });
        let value = self.rpc_value("wallet_propose", params).await?;
        parse_wallet_propose(&value)
    }
}

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

fn parse_simulate_result(value: &Value) -> color_eyre::Result<SimulateResult> {
    let result = value.get("result").unwrap_or(&Value::Null);
    let tx_json = result
        .get("tx_json")
        .cloned()
        .ok_or_else(|| color_eyre::eyre::eyre!("simulate response missing tx_json"))?;

    let engine_result = result
        .get("engine_result")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let engine_result_message = result
        .get("engine_result_message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let ledger_index = extract_json_u32(value, &["result", "ledger_index"]);
    let meta = result.get("meta").cloned();

    Ok(SimulateResult {
        tx_json,
        engine_result,
        engine_result_message,
        ledger_index,
        meta,
    })
}

fn parse_ripple_path_find(value: &Value) -> color_eyre::Result<RipplePathFindResult> {
    let result = value.get("result").unwrap_or(&Value::Null);

    let alternatives = result
        .get("alternatives")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|alt| PathAlternative {
                    paths_computed: alt.get("paths_computed").cloned().unwrap_or(Value::Null),
                    source_amount: alt.get("source_amount").cloned().unwrap_or(Value::Null),
                })
                .collect()
        })
        .unwrap_or_default();

    let destination_account = json_str(value, &["result", "destination_account"]).to_string();
    let destination_amount = result
        .get("destination_amount")
        .cloned()
        .unwrap_or(Value::Null);
    let source_account = json_str(value, &["result", "source_account"]).to_string();

    Ok(RipplePathFindResult {
        alternatives,
        destination_account,
        destination_amount,
        source_account,
    })
}

fn parse_wallet_propose(value: &Value) -> color_eyre::Result<WalletProposeResult> {
    let result = value.get("result").unwrap_or(&Value::Null);

    let master_seed = result
        .get("master_seed")
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| color_eyre::eyre::eyre!("wallet_propose: missing master_seed"))?;

    let account_id = result
        .get("account_id")
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| color_eyre::eyre::eyre!("wallet_propose: missing account_id"))?;

    Ok(WalletProposeResult {
        master_seed,
        master_seed_hex: json_str(value, &["result", "master_seed_hex"]).to_string(),
        account_id,
        public_key: json_str(value, &["result", "public_key"]).to_string(),
        public_key_hex: json_str(value, &["result", "public_key_hex"]).to_string(),
        key_type: json_str(value, &["result", "key_type"]).to_string(),
    })
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
                raw_json: ArcValue::new(n),
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
            raw_json: ArcValue::new(l),
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

fn parse_account_tx_page(value: &Value, watch_address: &str) -> AccountTxPage {
    let txns = value
        .get("result")
        .and_then(|v| v.get("transactions"))
        .and_then(Value::as_array)
        .map(|a| a.as_slice())
        .unwrap_or(&[]);
    let rows: Vec<TxRow> = txns
        .iter()
        .map(|t| {
            let tx = t
                .get("tx")
                .or_else(|| t.get("tx_json"))
                .unwrap_or(&Value::Null);
            let meta = t.get("meta").unwrap_or(&Value::Null);
            let account = tx.get("Account").and_then(Value::as_str).unwrap_or("");
            let destination = tx.get("Destination").and_then(Value::as_str);
            let direction = if account.eq_ignore_ascii_case(watch_address) {
                if destination.is_some_and(|d| d.eq_ignore_ascii_case(watch_address)) {
                    "·"
                } else {
                    "▼"
                }
            } else if destination.is_some_and(|d| d.eq_ignore_ascii_case(watch_address)) {
                "▲"
            } else {
                "·"
            }
            .to_string();
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
                result: meta
                    .get("TransactionResult")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .to_string(),
                direction,
                tx_json: crate::xrpl::types::ArcValue(std::sync::Arc::new(tx.clone())),
                meta_json: crate::xrpl::types::ArcValue(std::sync::Arc::new(meta.clone())),
            }
        })
        .collect();
    let marker = value.get("result").and_then(|v| v.get("marker")).cloned();
    AccountTxPage { rows, marker }
}

#[cfg(test)]
fn parse_account_tx_value(value: &Value, watch_address: &str) -> Vec<TxRow> {
    parse_account_tx_page(value, watch_address).rows
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
                raw_json: ArcValue::new(offer),
            }
        })
        .collect()
}

pub(crate) fn is_not_found_error(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("actnotfound")
        || lower.contains("actmalformed")
        || lower.contains("entrynotfound")
        || lower.contains("lgrnotfound")
        || lower.contains("object not found")
        || lower.contains("bad issuer")
        || lower.contains("account not found")
}

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

pub fn drops_to_xrp(drops: &str) -> String {
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

pub fn format_amount(value: Option<&Value>) -> String {
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
        raw_json: ArcValue::new(obj.clone()),
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
        let rows = parse_account_nfts_value(&v);
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
        let rows = parse_account_nfts_value(&v);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_mutable);
    }

    /// TC-014
    /// TC-075: xrpl-rust Payment<'static> deserialize from JSON
    #[test]
    fn payment_static_deserialize() {
        use xrpl::models::transactions::payment::Payment;
        let v = json!({
            "TransactionType": "Payment",
            "Account": "rN7n7otQDd6FczFgLdlqtyMVrn3HMfHgFj",
            "Destination": "rf1BiGeXwwQoi8Z2ueFYTEXSwuJYfV2Jpn",
            "Amount": "1000000",
            "Fee": "12",
            "Sequence": 1,
            "Flags": 0
        });
        let payment: Payment<'static> = serde_json::from_value(v).unwrap();
        assert_eq!(
            payment.common_fields.account,
            "rN7n7otQDd6FczFgLdlqtyMVrn3HMfHgFj"
        );
        assert_eq!(payment.destination, "rf1BiGeXwwQoi8Z2ueFYTEXSwuJYfV2Jpn");
    }

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
        let rows = parse_account_lines_value(&v);
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
        let s = parse_amm_info_value(&v);
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
                "transactions": [
                    {
                        "hash": "ABC123",
                        "ledger_index": 55,
                        "tx": {
                            "TransactionType": "Payment",
                            "hash": "ABC123",
                            "Account": "rWatch",
                            "Destination": "rDest"
                        },
                        "meta": {"TransactionResult": "tesSUCCESS"}
                    },
                    {
                        "hash": "DEF456",
                        "ledger_index": 56,
                        "tx": {
                            "TransactionType": "Payment",
                            "hash": "DEF456",
                            "Account": "rSrc",
                            "Destination": "rWatch"
                        },
                        "meta": {"TransactionResult": "tesSUCCESS"}
                    },
                    {
                        "hash": "GHI789",
                        "ledger_index": 57,
                        "tx": {
                            "TransactionType": "OfferCreate",
                            "hash": "GHI789",
                            "Account": "rOther"
                        },
                        "meta": {"TransactionResult": "tesSUCCESS"}
                    }
                ]
            }
        });
        let rows = parse_account_tx_value(&v, "rWatch");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].hash, "ABC123");
        assert_eq!(rows[0].tx_type, "Payment");
        assert_eq!(rows[0].direction, "▼");
        assert_eq!(rows[1].hash, "DEF456");
        assert_eq!(rows[1].direction, "▲");
        assert_eq!(rows[2].hash, "GHI789");
        assert_eq!(rows[2].direction, "·");
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
        let rows = parse_book_offers_value(&v);
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
        let sum = parse_server_info_value(&si);
        assert_eq!(sum.ledger_index, 80000000);
        assert_eq!(sum.hostid, "test-host");

        let fee = json!({"result": {"drops": {"open_ledger_fee": 15}}});
        let fs = parse_fee_value(&fee);
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
        let err = check_xrpl_error(&value).expect_err("not found must not be swallowed");
        assert!(is_not_found_error(&format!("{err}")));
    }

    #[test]
    fn parse_submit_success_requires_tes_success_and_hash() {
        let value = json!({
            "result": {
                "engine_result": "tesSUCCESS",
                "tx_json": {"hash": "ABC123"}
            }
        });
        let summary = parse_submit_success(&value).expect("submit should parse");
        assert_eq!(summary.hash, "ABC123");

        let failed = json!({
            "result": {
                "engine_result": "tecNO_DST_INSUF_XRP",
                "engine_result_message": "Destination does not exist."
            }
        });
        assert!(parse_submit_success(&failed).is_err());
    }

    #[test]
    fn parse_simulate_result_with_meta() {
        let value = json!({
            "result": {
                "tx_json": {
                    "Account": "rf1BiGeXwwQoi8Z2ueFYTEXSwuJYfV2Jpn",
                    "Fee": "10",
                    "Sequence": 360,
                    "TransactionType": "Payment"
                },
                "engine_result": "tesSUCCESS",
                "engine_result_message": "The simulated transaction would have been applied.",
                "ledger_index": 3,
                "meta": { "TransactionResult": "tesSUCCESS" }
            }
        });
        let sim = parse_simulate_result(&value).expect("simulate should parse");
        assert_eq!(sim.engine_result, "tesSUCCESS");
        assert_eq!(sim.ledger_index, 3);
        assert!(sim.meta.is_some());
        assert_eq!(sim.tx_json["Fee"], "10");
        assert_eq!(sim.tx_json["Sequence"], 360);
    }

    #[test]
    fn parse_simulate_result_tec_no_meta() {
        // Non-TEC failures omit meta per XRPL spec
        let value = json!({
            "result": {
                "tx_json": { "Account": "rTest" },
                "engine_result": "terNO_LINE",
                "engine_result_message": "No such line.",
                "ledger_index": 5
            }
        });
        let sim = parse_simulate_result(&value).expect("ter result should still parse");
        assert_eq!(sim.engine_result, "terNO_LINE");
        assert_eq!(sim.ledger_index, 5);
        assert!(sim.meta.is_none());
    }

    #[test]
    fn parse_simulate_result_missing_tx_json() {
        let value = json!({"result": {"engine_result": "tesSUCCESS"}});
        let err = parse_simulate_result(&value).expect_err("missing tx_json should fail");
        assert!(format!("{err}").contains("tx_json"));
    }

    /// TC-071 account_objects parse (empty)
    #[test]
    fn parse_account_objects_empty() {
        let v = json!({ "result": { "account_objects": [] } });
        assert!(parse_account_objects_value(&v).is_empty());
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
        let rows = parse_account_objects_value(&v);
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
        use crate::xrpl::{is_escrow_type, is_objects_tab_ledger_type, is_pay_channel_type};
        assert!(is_objects_tab_ledger_type("Check"));
        assert!(is_objects_tab_ledger_type("Ticket"));
        assert!(is_objects_tab_ledger_type("MPToken"));
        assert!(is_objects_tab_ledger_type("DID"));
        assert!(!is_objects_tab_ledger_type("PayChannel"));
        assert!(is_pay_channel_type("PayChannel"));
        assert!(is_escrow_type("Escrow"));
    }

    #[test]
    fn xrp_to_drops_whole() {
        assert_eq!(xrp_to_drops("1").unwrap(), 1_000_000);
    }

    #[test]
    fn xrp_to_drops_with_fraction() {
        assert_eq!(xrp_to_drops("1.5").unwrap(), 1_500_000);
    }

    #[test]
    fn xrp_to_drops_six_decimals() {
        assert_eq!(xrp_to_drops("1.123456").unwrap(), 1_123_456);
    }

    #[test]
    fn xrp_to_drops_too_many_decimals_err() {
        assert!(xrp_to_drops("1.1234567").is_err());
    }

    #[test]
    fn xrp_to_drops_empty_err() {
        assert!(xrp_to_drops("").is_err());
    }

    #[test]
    fn xrp_to_drops_invalid_err() {
        assert!(xrp_to_drops("abc").is_err());
    }

    #[test]
    fn xrp_to_drops_multiple_dots_err() {
        assert!(xrp_to_drops("1.2.3").is_err());
    }

    #[test]
    fn xrp_to_drops_leading_dot_err() {
        assert!(xrp_to_drops(".5").is_err());
    }

    #[test]
    fn xrp_to_drops_tiny_amount() {
        assert_eq!(xrp_to_drops("0.000001").unwrap(), 1);
    }

    #[test]
    fn xrp_to_drops_zero() {
        assert_eq!(xrp_to_drops("0").unwrap(), 0);
    }

    /// TC-079 ripple_path_find parse
    #[test]
    fn parse_ripple_path_find_with_alternatives() {
        let value = json!({
            "result": {
                "alternatives": [
                    {
                        "paths_computed": [[
                            {"currency": "USD", "issuer": "rIssuer1", "type": 48},
                            {"currency": "USD", "issuer": "rIssuer2", "type": 48}
                        ]],
                        "source_amount": {
                            "currency": "USD",
                            "issuer": "rSourceIssuer",
                            "value": "105.5"
                        }
                    }
                ],
                "destination_account": "rDest11111111111111111111111111111111",
                "destination_amount": {
                    "currency": "USD",
                    "issuer": "rDestIssuer",
                    "value": "100"
                },
                "source_account": "rSrc111111111111111111111111111111111"
            }
        });
        let paths = parse_ripple_path_find(&value).expect("path find should parse");
        assert_eq!(paths.alternatives.len(), 1);
        assert_eq!(
            paths.destination_account,
            "rDest11111111111111111111111111111111"
        );
        assert_eq!(
            paths.source_account,
            "rSrc111111111111111111111111111111111"
        );
        let alt = &paths.alternatives[0];
        assert_eq!(alt.source_amount["value"], "105.5");
    }

    /// TC-080 ripple_path_find empty alternatives
    #[test]
    fn parse_ripple_path_find_no_alternatives() {
        let value = json!({
            "result": {
                "alternatives": [],
                "destination_account": "rDest",
                "destination_amount": { "currency": "XRP" },
                "source_account": "rSrc"
            }
        });
        let paths = parse_ripple_path_find(&value).expect("empty alternatives should parse");
        assert!(paths.alternatives.is_empty());
    }

    /// TC-081 ripple_path_find source_amount as string (XRP drops)
    #[test]
    fn parse_ripple_path_find_source_amount_string() {
        let value = json!({
            "result": {
                "alternatives": [
                    {
                        "paths_computed": [],
                        "source_amount": "256987"
                    }
                ],
                "destination_account": "rDest",
                "destination_amount": { "currency": "USD", "issuer": "rIssuer", "value": "10" },
                "source_account": "rSrc"
            }
        });
        let paths = parse_ripple_path_find(&value).expect("string source_amount should parse");
        assert_eq!(paths.alternatives.len(), 1);
        // source_amount is a plain drops string, not an object
        assert_eq!(paths.alternatives[0].source_amount, json!("256987"));
    }

    /// TC-082 wallet_propose ed25519
    #[test]
    fn parse_wallet_propose_ed25519() {
        let value = json!({
            "result": {
                "account_id": "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh",
                "key_type": "ed25519",
                "master_seed": "sEdTzYqD8TKiF4MjRmq9h5RZVvqQeGF",
                "master_seed_hex": "DEDCE9CE67B451D852FD4E846FCDE31C",
                "master_key": "ED74D4036C6591A4BDF9C54CEFA39B996A5DCE5F86D11FDA1878C3A9E45606A5AB",
                "public_key": "aBQG8RQAzjs1eTKFEAQXr2gSJutMrk9oXqVtYN7qFZjNn82BScnG",
                "public_key_hex": "ED74D4036C6591A4BDF9C54CEFA39B996A5DCE5F86D11FDA1878C3A9E45606A5AB",
                "status": "success"
            }
        });
        let r = parse_wallet_propose(&value).expect("wallet_propose should parse");
        assert!(r.master_seed.starts_with('s'));
        assert!(r.account_id.starts_with('r'));
        assert!(r.public_key.starts_with('a'));
        assert_eq!(r.key_type, "ed25519");
        assert!(!r.master_seed_hex.is_empty());
    }
}
