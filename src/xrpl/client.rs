use std::time::Duration;

use serde_json::{Value, json};
use xrpl::asynch::clients::{AsyncJsonRpcClient, XRPLClient};

use super::dunl::{XRPLF_DUNL_URL, parse_xrplf_dunl_json};
use super::format::drops_to_xrp;
pub use super::format::{path_find_snapshot, xrp_to_drops};
use super::json_util::{extract_json_u32, json_str};
pub(crate) use super::parse::empty_account_tx_page_on_not_found;
use super::parse::{
    book_currency, book_offer_best_price, is_not_found_error, is_rate_limited_error,
    parse_account_lines_value, parse_account_nfts_value, parse_account_objects_value,
    parse_account_tx_page, parse_aggregate_price_value, parse_amm_info_value,
    parse_book_offers_value, parse_fee_value, parse_ripple_path_find, parse_server_info_value,
    parse_simulate_result, parse_submit_success, parse_wallet_propose,
};
use super::types::{
    AccountSummary, AccountTxPage, AggregatePrice, AmmSummary, DunlSummary, FeeSummary,
    LedgerObjectRow, NftRow, OfferRow, OracleId, RipplePathFindResult, ServerInfoSummary,
    SimulateResult, TrustLineRow, TxRow, TxSummary, WalletProposeResult, XrplRlusdPrice,
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
                let status = resp.status();
                let text = tokio::time::timeout(RPC_TIMEOUT, resp.text())
                    .await
                    .map_err(|_| color_eyre::eyre::eyre!("{method} response timeout"))?
                    .map_err(|e| color_eyre::eyre::eyre!("{method} response error: {e}"))?;
                if status.as_u16() == 429 {
                    let preview: String = text.chars().take(120).collect();
                    return Err(color_eyre::eyre::eyre!(
                        "{method} Rate limited (HTTP 429); body={preview:?}"
                    ));
                }
                serde_json::from_str::<Value>(&text).map_err(|e| {
                    let preview: String = text.chars().take(120).collect();
                    color_eyre::eyre::eyre!(
                        "{method} JSON parse error: {e}; status={status}; body={preview:?}"
                    )
                })
            }
            .await;

            match result {
                Ok(value) => {
                    ensure_no_xrpl_rpc_error(&value)?;
                    return Ok(value);
                }
                Err(e) if attempt < 2 => {
                    let message = format!("{e}");
                    last_error = Some(e);
                    let delay = if is_rate_limited_error(&message) {
                        Duration::from_secs(2 * (attempt + 1) as u64)
                    } else {
                        Duration::from_millis(100 * (attempt + 1) as u64)
                    };
                    tracing::warn!(
                        attempt = attempt + 1,
                        delay_ms = delay.as_millis() as u64,
                        "{method} retry after: {message}"
                    );
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

    pub async fn fetch_xrplf_dunl(&self) -> color_eyre::Result<DunlSummary> {
        let resp = tokio::time::timeout(RPC_TIMEOUT, self.http.get(XRPLF_DUNL_URL).send())
            .await
            .map_err(|_| color_eyre::eyre::eyre!("dUNL fetch timeout"))?
            .map_err(|e| color_eyre::eyre::eyre!("dUNL fetch error: {e}"))?;
        let text = tokio::time::timeout(RPC_TIMEOUT, resp.text())
            .await
            .map_err(|_| color_eyre::eyre::eyre!("dUNL response timeout"))?
            .map_err(|e| color_eyre::eyre::eyre!("dUNL response error: {e}"))?;
        parse_xrplf_dunl_json(&text)
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
            Err(e) => {
                if let Some(page) = empty_account_tx_page_on_not_found(&e) {
                    return Ok(page);
                }
                return Err(e);
            }
        };
        Ok(parse_account_tx_page(value, watch_address))
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
                let error_text = format!("{e}");
                if is_not_found_error(&error_text) {
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
                let error_text = format!("{e}");
                if is_not_found_error(&error_text) {
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

    /// Generate a new XRPL wallet via `wallet_propose` RPC (tests / optional callers).
    ///
    /// TUI keygen uses [`crate::signing::propose_wallet_local`] because public RPC nodes
    /// often omit `master_seed`.
    #[allow(dead_code)]
    pub async fn wallet_propose(&self, key_type: &str) -> color_eyre::Result<WalletProposeResult> {
        let params = json!({ "key_type": key_type });
        let value = self.rpc_value("wallet_propose", params).await?;
        parse_wallet_propose(&value)
    }

    pub async fn get_aggregate_price(
        &self,
        oracles: &[OracleId],
        base_asset: &str,
        quote_asset: &str,
    ) -> color_eyre::Result<AggregatePrice> {
        let oracle_array: Vec<Value> = oracles
            .iter()
            .map(|o| {
                json!({
                    "account": o.account,
                    "oracle_document_id": o.oracle_document_id,
                })
            })
            .collect();
        let params = json!({
            "ledger_index": "current",
            "base_asset": base_asset,
            "quote_asset": quote_asset,
            "oracles": oracle_array,
        });
        let value = self.rpc_value("get_aggregate_price", params).await?;
        let mut price = parse_aggregate_price_value(&value)?;
        price.base_asset = base_asset.to_string();
        price.quote_asset = quote_asset.to_string();
        Ok(price)
    }
}

fn ensure_no_xrpl_rpc_error(value: &Value) -> color_eyre::Result<()> {
    if let Some(result) = value.get("result")
        && let Some(error) = result.get("error").and_then(Value::as_str)
    {
        let code = result
            .get("error_code")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let error_message = result
            .get("error_message")
            .and_then(Value::as_str)
            .unwrap_or(error);
        let kind = if is_not_found_error(error) {
            "NOT_FOUND".to_string()
        } else {
            code.to_string()
        };
        return Err(color_eyre::eyre::eyre!("[XRPL-{kind}] {error_message}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ensure_no_xrpl_rpc_error_preserves_not_found_as_error() {
        let value = json!({
            "result": {
                "error": "actNotFound",
                "error_code": 19,
                "error_message": "Account not found."
            }
        });
        let err = ensure_no_xrpl_rpc_error(&value).expect_err("not found must not be swallowed");
        assert!(is_not_found_error(&format!("{err}")));
    }
}
