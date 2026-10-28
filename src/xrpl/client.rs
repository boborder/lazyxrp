use std::time::Duration;

use serde_json::{Value, json};
use xrpl::asynch::clients::{AsyncJsonRpcClient, XRPLClient};

use super::dunl::{
    DUNL_RETRY_BACKOFF, XRPLF_DUNL_URL, dunl_cache_get, dunl_cache_mark_failed, dunl_cache_store,
    parse_xrplf_dunl_json,
};
use super::format::drops_to_xrp;
pub use super::format::{path_find_snapshot, xrp_to_drops};
pub use super::parse::SubmitOutcome;
pub(crate) use super::parse::empty_account_tx_page_on_not_found;
use super::parse::{
    book_currency, book_offer_best_price, escrow_offer_sequence_from_tx, is_not_found_error,
    is_rate_limited_error, is_transient_xrpl_error, parse_account_escrows_value,
    parse_account_lines_value, parse_account_nfts_value, parse_account_objects_value,
    parse_account_offers_value, parse_account_tx_page, parse_aggregate_price_value,
    parse_amm_info_value, parse_amm_lp_token, parse_book_offers_value, parse_fee_value,
    parse_ripple_path_find, parse_server_info_value, parse_simulate_result, parse_submit_success,
};
use super::types::{
    AccountOfferRow, AccountSummary, AccountTxPage, AggregatePrice, AmmSummary, BookMidPrice,
    DunlSummary, EscrowRow, FeeSummary, LedgerObjectRow, NftRow, OfferRow, OracleId,
    RipplePathFindResult, ServerInfoSummary, SimulateResult, TrustLineRow, TxRow,
};
use super::util::{json_str, json_u32};

pub(crate) const RPC_TIMEOUT: Duration = Duration::from_secs(20);
/// Post-submit `tx` lookups after an ambiguous transport failure (spaced, not retried blindly).
const SUBMIT_LOOKUP_TRIES: usize = 3;
const SUBMIT_LOOKUP_INTERVAL: Duration = Duration::from_secs(3);
/// Hard cap for JSON-RPC / dUNL response bodies (DoS guard).
pub(crate) const MAX_RPC_BODY_BYTES: usize = 16 * 1024 * 1024;

fn retry_delay_ms(attempt: usize, message: &str) -> u64 {
    // Unit tests retry against local fixtures; skip the real backoff.
    if cfg!(test) {
        return 0;
    }
    let delay = if is_rate_limited_error(message) {
        Duration::from_secs(2 * (attempt + 1) as u64)
    } else if is_transient_xrpl_error(message) {
        // A node that is lagging or starting up needs seconds,
        // not milliseconds, to become servable.
        Duration::from_secs((attempt + 1) as u64)
    } else {
        Duration::from_millis(100 * (attempt + 1) as u64)
    };
    delay.as_millis() as u64
}

pub struct RpcClient {
    client: AsyncJsonRpcClient,
    http: reqwest::Client,
    network_id: std::sync::atomic::AtomicU32,
}

pub(crate) async fn read_response_text_capped(
    mut resp: reqwest::Response,
    label: &str,
) -> color_eyre::Result<String> {
    if let Some(len) = resp.content_length()
        && len as usize > MAX_RPC_BODY_BYTES
    {
        color_eyre::eyre::bail!(
            "{label}: response Content-Length {len} exceeds cap {MAX_RPC_BODY_BYTES}"
        );
    }
    // Chunked bodies carry no Content-Length: enforce the cap while reading, not after.
    let read = async {
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| color_eyre::eyre::eyre!("{label} response error: {e}"))?
        {
            if bytes.len() + chunk.len() > MAX_RPC_BODY_BYTES {
                color_eyre::eyre::bail!("{label}: response body exceeds cap {MAX_RPC_BODY_BYTES}");
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    };
    let bytes = tokio::time::timeout(RPC_TIMEOUT, read)
        .await
        .map_err(|_| color_eyre::eyre::eyre!("{label} response timeout"))??;
    String::from_utf8(bytes)
        .map_err(|e| color_eyre::eyre::eyre!("{label} response utf8 error: {e}"))
}

impl RpcClient {
    pub fn connect(rpc_url: &str) -> color_eyre::Result<Self> {
        Ok(Self {
            network_id: std::sync::atomic::AtomicU32::new(0),
            client: AsyncJsonRpcClient::connect(rpc_url.parse()?),
            // No redirects: an https endpoint must not bounce `submit` (signed blob) elsewhere.
            http: reqwest::Client::builder()
                .http1_only()
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
        })
    }

    /// One request/response, no retry (a `submit` must not be replayed blindly).
    async fn rpc_once(&self, method: &str, params: &Value) -> color_eyre::Result<Value> {
        let url = self.client.get_host().to_string();
        let req_json = json!({ "method": method, "params": [params] });
        let resp = tokio::time::timeout(RPC_TIMEOUT, self.http.post(&url).json(&req_json).send())
            .await
            .map_err(|_| color_eyre::eyre::eyre!("{method} timeout"))?
            .map_err(|e| color_eyre::eyre::eyre!("{method} request error: {e}"))?;
        let status = resp.status();
        let text = read_response_text_capped(resp, method).await?;
        if status.as_u16() == 429 {
            let preview: String = text.chars().take(120).collect();
            return Err(color_eyre::eyre::eyre!(
                "{method} Rate limited (HTTP 429); body={preview:?}"
            ));
        }
        let value = serde_json::from_str::<Value>(&text).map_err(|e| {
            let preview: String = text.chars().take(120).collect();
            color_eyre::eyre::eyre!(
                "{method} JSON parse error: {e}; status={status}; body={preview:?}"
            )
        })?;
        ensure_no_xrpl_rpc_error(&value)?;
        Ok(value)
    }

    async fn rpc_value(&self, method: &str, params: Value) -> color_eyre::Result<Value> {
        let mut last_error = None;
        for attempt in 0..3 {
            match self.rpc_once(method, &params).await {
                Ok(value) => return Ok(value),
                Err(e) if attempt < 2 && is_retryable_rpc_error(&format!("{e}")) => {
                    let message = format!("{e}");
                    last_error = Some(e);
                    let delay_ms = retry_delay_ms(attempt, &message);
                    tracing::warn!(
                        attempt = attempt + 1,
                        delay_ms,
                        "{method} retry after: {message}"
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_error.unwrap_or_else(|| color_eyre::eyre::eyre!("{method} failed")))
    }

    /// `rpc_value`, but an account/object/ledger "not found" error is `None` instead of an error.
    async fn rpc_value_or_none(
        &self,
        method: &str,
        params: Value,
    ) -> color_eyre::Result<Option<Value>> {
        match self.rpc_value(method, params).await {
            Ok(value) => Ok(Some(value)),
            Err(e) if is_not_found_error(&format!("{e}")) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn server_info(&self) -> color_eyre::Result<ServerInfoSummary> {
        let value = self.rpc_value("server_info", json!({})).await?;
        Ok(parse_server_info_value(&value))
    }

    /// Before signing: the endpoint's `server_info.network_id` must be `network`'s.
    pub async fn ensure_network(
        &self,
        network: &crate::network::Network,
    ) -> color_eyre::Result<()> {
        let value = self.rpc_value("server_info", json!({})).await?;
        let reported = value["result"]["info"]["network_id"]
            .as_u64()
            .map(|n| u32::try_from(n).unwrap_or(u32::MAX));
        network.check_endpoint_network_id(reported)?;
        self.network_id
            .store(network.network_id(), std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// `network_id` of the network last verified by [`Self::ensure_network`] (0 before that);
    /// signing and simulate put it in `NetworkID` when > 1024.
    pub fn network_id(&self) -> u32 {
        self.network_id.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub async fn fetch_xrplf_dunl(&self) -> color_eyre::Result<DunlSummary> {
        // Stale-on-error: a failed refresh keeps serving the cached summary for
        // DUNL_RETRY_BACKOFF instead of erroring / re-hitting the mirror every poll.
        if let Some(cached) = dunl_cache_get() {
            return Ok(cached);
        }
        // ponytail: a first-ever fetch failure has no cache to serve and errors every
        // poll until the first success — acceptable; a failure cache for the empty case
        // would add state for a transient startup window.
        let attempt = async {
            let resp = tokio::time::timeout(RPC_TIMEOUT, self.http.get(XRPLF_DUNL_URL).send())
                .await
                .map_err(|_| color_eyre::eyre::eyre!("dUNL fetch timeout"))?
                .map_err(|e| color_eyre::eyre::eyre!("dUNL fetch error: {e}"))?;
            let text = read_response_text_capped(resp, "dUNL").await?;
            parse_xrplf_dunl_json(&text)
        };
        match attempt.await {
            Ok(summary) => {
                dunl_cache_store(summary.clone());
                Ok(summary)
            }
            Err(e) => {
                dunl_cache_mark_failed();
                // Warn only when a cached summary will actually be served.
                if dunl_cache_get().is_some() {
                    tracing::warn!(
                        "dUNL refresh failed; serving cached manifest for up to {}s",
                        DUNL_RETRY_BACKOFF.as_secs()
                    );
                }
                Err(e)
            }
        }
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
            sequence: json_u32(&value, &["result", "account_data", "Sequence"]),
            owner_count: json_u32(&value, &["result", "account_data", "OwnerCount"]),
            flags: json_u32(&value, &["result", "account_data", "Flags"]),
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
        let Some(value) = self
            .rpc_value_or_none(
                "account_nfts",
                json!({ "account": watch_address, "limit": 100 }),
            )
            .await?
        else {
            return Ok(vec![]);
        };
        Ok(parse_account_nfts_value(&value))
    }

    pub async fn account_lines(
        &self,
        watch_address: &str,
    ) -> color_eyre::Result<Vec<TrustLineRow>> {
        let Some(value) = self
            .rpc_value_or_none("account_lines", json!({ "account": watch_address }))
            .await?
        else {
            return Ok(vec![]);
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

    /// LP token `(currency, issuer)` of the pool for `asset`/`asset2` (`XRP` or `CUR:issuer`).
    pub async fn amm_lp_token(
        &self,
        asset: &str,
        asset2: &str,
    ) -> color_eyre::Result<(String, String)> {
        let (c1, i1) = crate::signing::parse_amm_asset(asset)?;
        let (c2, i2) = crate::signing::parse_amm_asset(asset2)?;
        let value = self
            .rpc_value(
                "amm_info",
                json!({
                    "asset": book_currency(&c1, i1.as_deref()),
                    "asset2": book_currency(&c2, i2.as_deref())
                }),
            )
            .await?;
        parse_amm_lp_token(&value)
            .ok_or_else(|| color_eyre::eyre::eyre!("amm_info: no LP token for {asset} / {asset2}"))
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
            Ok(Err(e)) if is_not_found_error(&format!("{e}")) => None,
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(color_eyre::eyre::eyre!("account_info timeout")),
        };

        let (txs, marker) = match tx_res {
            Ok(Ok(page)) => (page.rows, page.marker),
            Ok(Err(e)) if is_not_found_error(&format!("{e}")) => (vec![], None),
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(color_eyre::eyre::eyre!("account_tx timeout")),
        };

        Ok((account, txs, marker))
    }

    pub async fn account_objects(
        &self,
        watch_address: &str,
    ) -> color_eyre::Result<Vec<LedgerObjectRow>> {
        let Some(value) = self
            .rpc_value_or_none(
                "account_objects",
                json!({ "account": watch_address, "limit": 200 }),
            )
            .await?
        else {
            return Ok(vec![]);
        };
        Ok(parse_account_objects_value(&value))
    }

    pub async fn account_offers(&self, account: &str) -> color_eyre::Result<Vec<AccountOfferRow>> {
        // ponytail: 50 pages × 200 caps a node that keeps returning markers
        let mut rows = Vec::new();
        let mut marker: Option<Value> = None;
        for _ in 0..50 {
            let mut req = json!({ "account": account, "limit": 200 });
            if let Some(m) = marker.take() {
                req["marker"] = m;
            }
            let Some(value) = self.rpc_value_or_none("account_offers", req).await? else {
                return Ok(vec![]);
            };
            rows.extend(parse_account_offers_value(&value));
            marker = value.get("result").and_then(|r| r.get("marker")).cloned();
            if marker.is_none() {
                break;
            }
        }
        Ok(rows)
    }

    /// Escrows the account owns or receives, each with the `OfferSequence` Finish/Cancel needs.
    /// When the object lacks `Sequence`, it is read from the creating tx (`PreviousTxnID`); a
    /// failed lookup leaves `offer_sequence` `None` and the composer refuses that row.
    pub async fn account_escrows(&self, account: &str) -> color_eyre::Result<Vec<EscrowRow>> {
        // ponytail: first 200 escrows, one `tx` lookup each without Sequence; page if needed
        let Some(value) = self
            .rpc_value_or_none(
                "account_objects",
                json!({ "account": account, "type": "escrow", "limit": 200 }),
            )
            .await?
        else {
            return Ok(vec![]);
        };
        let mut rows = parse_account_escrows_value(&value);
        for row in rows.iter_mut().filter(|r| r.offer_sequence.is_none()) {
            if let Ok(tx) = self.tx(&row.previous_txn_id).await {
                row.offer_sequence = escrow_offer_sequence_from_tx(&tx);
            }
        }
        Ok(rows)
    }

    pub async fn book_offers(
        &self,
        taker_gets_currency: &str,
        taker_gets_issuer: Option<&str>,
        taker_pays_currency: &str,
        taker_pays_issuer: Option<&str>,
        limit: u16,
    ) -> color_eyre::Result<Vec<OfferRow>> {
        let Some(value) = self
            .rpc_value_or_none(
                "book_offers",
                json!({
                    "taker_gets": book_currency(taker_gets_currency, taker_gets_issuer),
                    "taker_pays": book_currency(taker_pays_currency, taker_pays_issuer),
                    "limit": limit
                }),
            )
            .await?
        else {
            return Ok(vec![]);
        };
        Ok(parse_book_offers_value(&value))
    }

    pub async fn book_mid_price(
        &self,
        quote_currency: &str,
        issuer: &str,
    ) -> color_eyre::Result<BookMidPrice> {
        let bid_params = json!({
            "taker_gets": book_currency("XRP", None),
            "taker_pays": book_currency(quote_currency, Some(issuer)),
            "limit": 1
        });
        let ask_params = json!({
            "taker_gets": book_currency(quote_currency, Some(issuer)),
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

        Ok(BookMidPrice {
            bid: bid_str,
            ask: ask_str,
            mid: mid_str,
        })
    }

    /// Whether `address` exists on the ledger: a successful `account_info` is activation
    /// (the reserve is not a balance threshold — an account may hold less than the reserve).
    pub async fn fetch_account_activation_status(&self, address: &str) -> color_eyre::Result<bool> {
        match self.account_info(address).await {
            Ok(_) => Ok(true),
            Err(e) if is_not_found_error(&format!("{e}")) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Look up a validated (or open) transaction by hash via the `tx` RPC.
    pub async fn tx(&self, hash: &str) -> color_eyre::Result<Value> {
        self.rpc_value("tx", json!({ "transaction": hash })).await
    }

    /// Submit a signed blob exactly once; the hash is computed from the blob, not taken from the
    /// node. A node-side rejection is final. A transport failure is ambiguous (the node may have
    /// applied it), so the locally computed hash is looked up: found → real outcome, not found →
    /// an explicit "unknown" error that names the hash.
    pub async fn submit_signed_tx(
        &self,
        signed_tx_blob: &str,
    ) -> color_eyre::Result<SubmitOutcome> {
        let hash = crate::signing::tx_hash_from_blob(signed_tx_blob)?;
        let cause = match self
            .rpc_once("submit", &json!({ "tx_blob": signed_tx_blob }))
            .await
        {
            Ok(value) => {
                let accepted = parse_submit_success(&value)?;
                return Ok(SubmitOutcome { hash, ..accepted });
            }
            Err(e) if format!("{e}").starts_with("[XRPL-") => return Err(e),
            Err(e) => e,
        };
        for attempt in 0..SUBMIT_LOOKUP_TRIES {
            if attempt > 0 && !cfg!(test) {
                tokio::time::sleep(SUBMIT_LOOKUP_INTERVAL).await;
            }
            let Ok(found) = self.rpc_once("tx", &json!({ "transaction": hash })).await else {
                continue;
            };
            let result = &found["result"];
            return match result["meta"]["TransactionResult"].as_str() {
                Some(code) if !code.starts_with("tes") => Err(color_eyre::eyre::eyre!(
                    "submit reply lost ({cause}); transaction {hash} was applied with {}",
                    super::util::sanitize_remote(code)
                )),
                _ => Ok(SubmitOutcome {
                    hash,
                    queued: result["validated"].as_bool() != Some(true),
                }),
            };
        }
        Err(color_eyre::eyre::eyre!(
            "submit outcome unknown ({cause}); transaction {hash} not found on the node yet — check hash {hash} before resending"
        ))
    }

    /// Dry-run an unsigned transaction via the `simulate` RPC.
    ///
    /// Returns the auto-filled `tx_json` (Fee, Sequence, etc.) and metadata
    /// without committing to the ledger.
    pub async fn simulate_tx(&self, mut tx_json: Value) -> color_eyre::Result<SimulateResult> {
        // xrpl-rust serializes some unset fields (e.g. `Memo.MemoFormat`) as `null`; rippled
        // rejects those as "bad type" (the signed binary omits them).
        drop_nulls(&mut tx_json);
        // Networks with id > 1024 (Xahau) require `NetworkID` in every tx.
        let network_id = self.network_id();
        if network_id > 1024 && tx_json.get("NetworkID").is_none() {
            tx_json["NetworkID"] = json!(network_id);
        }
        let value = self
            .rpc_value("simulate", json!({ "tx_json": tx_json }))
            .await?;
        parse_simulate_result(&value)
    }

    /// `ripple_path_find` – find payment paths between two accounts for a given
    /// destination currency amount.  Use this to preview swap rates before
    /// submitting an IOU payment (especially self-pay → DEX swap).
    ///
    /// `destination_amount` is an IOU object like
    /// `{"currency":"USD","issuer":"r...","value":"100"}` or an XRP drops string.
    /// `source_currencies`: optional `[{"currency":..,"issuer"?:..}]` filter (Payment quote).
    pub async fn ripple_path_find(
        &self,
        source_account: &str,
        destination_account: &str,
        destination_amount: &Value,
        source_currencies: Option<&[Value]>,
    ) -> color_eyre::Result<RipplePathFindResult> {
        let mut params = json!({
            "source_account": source_account,
            "destination_account": destination_account,
            "destination_amount": destination_amount,
        });
        if let Some(currencies) = source_currencies {
            params["source_currencies"] = Value::from(currencies);
        }
        let value = self.rpc_value("ripple_path_find", params).await?;
        parse_ripple_path_find(&value)
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

/// Remove `null` object members at any depth (a JSON null is never a valid XRPL field value).
fn drop_nulls(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            map.values_mut().for_each(drop_nulls);
        }
        Value::Array(items) => items.iter_mut().for_each(drop_nulls),
        _ => {}
    }
}

/// Whether an RPC attempt is worth retrying. Transport/parse/HTTP errors
/// always retry; XRPL-level errors only when the server signals a transient
/// node state (lagging, starting up, overloaded).
fn is_retryable_rpc_error(message: &str) -> bool {
    if message.starts_with("[XRPL-") {
        is_transient_xrpl_error(message)
    } else {
        true
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
        let error_message = super::util::sanitize_remote(
            result
                .get("error_message")
                .and_then(Value::as_str)
                .unwrap_or(error),
        );
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

    /// A chunked body (no Content-Length) larger than the cap is cut off while reading.
    #[tokio::test]
    async fn chunked_body_over_cap_is_rejected_while_streaming() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = [0_u8; 1024];
            let _ = s.read(&mut buf).await;
            let head =
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
            if s.write_all(head).await.is_err() {
                return;
            }
            let chunk = vec![b'x'; 1024 * 1024];
            for _ in 0..(MAX_RPC_BODY_BYTES / (1024 * 1024) + 2) {
                if s.write_all(b"100000\r\n").await.is_err()
                    || s.write_all(&chunk).await.is_err()
                    || s.write_all(b"\r\n").await.is_err()
                {
                    return; // client hung up at the cap
                }
            }
        });
        let http = reqwest::Client::builder().no_proxy().build().unwrap();
        let resp = http.get(format!("http://{addr}/")).send().await.unwrap();
        assert!(resp.content_length().is_none());
        let err = read_response_text_capped(resp, "test").await.unwrap_err();
        assert!(err.to_string().contains("exceeds cap"), "{err}");
        server.abort();
    }

    /// A declared Content-Length over the cap is rejected before any body is read.
    #[tokio::test]
    async fn content_length_over_cap_is_rejected_up_front() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = [0_u8; 1024];
            let _ = s.read(&mut buf).await;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_RPC_BODY_BYTES + 1
            );
            let _ = s.write_all(head.as_bytes()).await;
            tokio::time::sleep(std::time::Duration::from_secs(2)).await; // no body ever sent
        });
        let http = reqwest::Client::builder().no_proxy().build().unwrap();
        let resp = http.get(format!("http://{addr}/")).send().await.unwrap();
        let err = read_response_text_capped(resp, "test").await.unwrap_err();
        assert!(err.to_string().contains("Content-Length"), "{err}");
        server.abort();
    }

    /// TC-210: an RPC endpoint answering 307 is an error — the request (and the signed blob
    /// in `submit`) is never re-posted to the redirect target, which may be plain `http://`.
    #[tokio::test]
    async fn rpc_redirect_is_not_followed() {
        use crate::test_support::rpc_mock;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let target = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let target_addr = target.local_addr().unwrap();
        let target_srv = tokio::spawn(rpc_mock::serve(target, 1, |_| json!({"info": {}})));
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = [0_u8; 4096];
            let _ = s.read(&mut buf).await;
            let head = format!(
                "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{target_addr}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            let _ = s.write_all(head.as_bytes()).await;
        });
        let rpc = RpcClient::connect(&format!("http://{addr}")).unwrap();
        let err = rpc.rpc_once("server_info", &json!({})).await.unwrap_err();
        assert!(err.to_string().contains("307"), "{err}");
        server.abort();
        target_srv.abort();
    }

    /// TC-068: XRPL RPC error — not found is not silently swallowed
    #[test]
    fn ensure_no_xrpl_rpc_error_preserves_not_found_as_error() {
        let err = |result: Value| {
            ensure_no_xrpl_rpc_error(&json!({ "result": result }))
                .expect_err("error result must be Err")
                .to_string()
        };
        assert_eq!(
            err(json!({"error": "actNotFound", "error_code": 19,
                       "error_message": "Account not found."})),
            "[XRPL-NOT_FOUND] Account not found."
        );
        // Other errors carry the numeric code; node-supplied text is control-stripped.
        assert_eq!(
            err(json!({"error": "malformed", "error_code": 12,
                       "error_message": "bad\u{1b}[2J tx"})),
            "[XRPL-12] bad[2J tx"
        );
        // No `error` key: Ok pass-through.
        assert!(ensure_no_xrpl_rpc_error(&json!({"result": {"status": "success"}})).is_ok());
    }

    /// TC-102: rate-limited RPC errors remain retryable while permanent XRPL errors do not
    #[test]
    fn rate_limited_rpc_errors_are_retryable_but_nontransient_xrpl_errors_are_not() {
        assert!(is_retryable_rpc_error(
            "account_info Rate limited (HTTP 429)"
        ));
        assert!(is_retryable_rpc_error("request error: connection reset"));
        assert!(!is_retryable_rpc_error("[XRPL-12] malformed transaction"));
        assert!(is_retryable_rpc_error("[XRPL-17] tooBusy"));
    }

    /// TC-105: local HTTP 429 responses are retried before success
    #[tokio::test]
    async fn rpc_value_retries_local_http_429_then_returns_success() {
        use std::time::Duration;

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for attempt in 0..3 {
                let (mut stream, _) =
                    tokio::time::timeout(Duration::from_secs(5), listener.accept())
                        .await
                        .unwrap()
                        .unwrap();
                let mut request = [0_u8; 1024];
                let _ = stream.read(&mut request).await.unwrap();
                let (status, body) = if attempt < 2 {
                    ("429 Too Many Requests", "{}")
                } else {
                    ("200 OK", r#"{"result":{}}"#)
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });

        let rpc = RpcClient::connect(&format!("http://{address}")).unwrap();
        let value = rpc.rpc_value("server_info", json!({})).await.unwrap();
        assert_eq!(value, json!({"result": {}}));
        server.await.unwrap();
    }

    /// TC-102: persistent 429 gives up after exactly 3 attempts and returns the rate-limit error.
    #[tokio::test]
    async fn rpc_value_gives_up_after_three_attempts() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut served = 0;
            while let Ok(Ok((mut stream, _))) =
                tokio::time::timeout(std::time::Duration::from_millis(500), listener.accept()).await
            {
                let mut request = [0_u8; 1024];
                let _ = stream.read(&mut request).await;
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                    )
                    .await;
                served += 1;
            }
            served
        });
        let rpc = RpcClient::connect(&format!("http://{address}")).unwrap();
        let err = rpc.rpc_value("server_info", json!({})).await.unwrap_err();
        assert!(err.to_string().contains("Rate limited (HTTP 429)"), "{err}");
        assert_eq!(server.await.unwrap(), 3, "exactly 3 attempts");
    }

    /// TC-102: a transient XRPL error (`tooBusy`, code 17) is retried; success follows.
    #[tokio::test]
    async fn rpc_value_retries_transient_xrpl_error() {
        use crate::test_support::rpc_mock;
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        fn result_for(_: &str) -> Value {
            if CALLS.fetch_add(1, Ordering::SeqCst) == 0 {
                json!({"error": "tooBusy", "error_code": 17,
                       "error_message": "Server is tooBusy"})
            } else {
                json!({"status": "success"})
            }
        }
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(rpc_mock::serve(listener, 2, result_for));
        let rpc = RpcClient::connect(&url).unwrap();
        let value = rpc.rpc_value("server_info", json!({})).await.unwrap();
        assert_eq!(value["result"]["status"], "success");
        server.await.unwrap().unwrap();
    }

    /// TC-229: `account_offers` follows `marker` across pages until the node stops returning one.
    #[tokio::test]
    async fn account_offers_follows_marker_pages() {
        use crate::test_support::rpc_mock;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let offer = |seq: u32| json!({"seq": seq, "taker_gets": "1", "taker_pays": "2"});
        let server = tokio::spawn(rpc_mock::serve_raw(
            listener,
            2,
            move |body| match body["params"][0].get("marker") {
                None => json!({"result": {"offers": [offer(1), offer(2)], "marker": "M1"}}),
                Some(m) => {
                    assert_eq!(m, "M1");
                    json!({"result": {"offers": [offer(3)]}})
                }
            },
        ));
        let rpc = RpcClient::connect(&url).unwrap();
        let seqs: Vec<u32> = rpc
            .account_offers("rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh")
            .await
            .unwrap()
            .iter()
            .map(|o| o.seq)
            .collect();
        assert_eq!(seqs, [1, 2, 3]);
        server.await.unwrap().unwrap();
    }

    /// TC-230: `simulate` never sends JSON nulls — xrpl-rust serializes unset `Memo` fields as
    /// `null`, which rippled rejects ("Field 'Memos.Memo.MemoFormat' has bad type").
    #[tokio::test]
    async fn simulate_request_drops_null_fields() {
        use crate::test_support::rpc_mock;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(rpc_mock::serve(listener, 1, |_| {
            json!({"engine_result": "tesSUCCESS", "ledger_index": 1,
                   "tx_json": {"Sequence": 7, "Fee": "12"}})
        }));
        let memo = crate::signing::build_direct_mint_memo_data(
            "0xabcdef0123456789abcdef0123456789abcdef01",
        )
        .unwrap();
        let tx_json = crate::signing::build_payment_tx_json_for_simulate(
            "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh",
            "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN",
            "1",
            None,
            None,
            None,
            None,
            Some(&memo),
            7,
        )
        .unwrap();
        let rpc = RpcClient::connect(&url).unwrap();
        rpc.simulate_tx(tx_json).await.unwrap();
        let requests = server.await.unwrap().unwrap();
        let sent = &requests[0]["params"][0]["tx_json"];
        assert!(!sent.to_string().contains("null"), "{sent}");
        assert_eq!(sent["Memos"][0]["Memo"]["MemoData"], json!(memo));
    }
}

#[cfg(test)]
mod submit_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Local node: `submit` connections are dropped without a reply (lost response), `tx` gets
    /// `tx_reply`; every received method is recorded.
    async fn lossy_node(
        tx_reply: Value,
    ) -> (
        String,
        tokio::task::JoinHandle<()>,
        std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        let server = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = vec![0_u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let text = String::from_utf8_lossy(&buf[..n]).to_string();
                let method = if text.contains("\"submit\"") {
                    "submit"
                } else {
                    "tx"
                };
                log.lock().unwrap().push(method.to_string());
                if method == "tx" {
                    let body = json!({"result": tx_reply}).to_string();
                    let _ = stream
                        .write_all(
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                                body.len()
                            )
                            .as_bytes(),
                        )
                        .await;
                }
            }
        });
        (url, server, seen)
    }

    /// `NetworkID` is carried only when the network id is > 1024: mainnet/testnet omit it,
    /// Xahau / Xahau-testnet sign 21337 / 21338 into the blob and into the simulate request.
    #[tokio::test]
    async fn network_id_is_set_only_above_1024() {
        use crate::network::Network;
        use crate::test_support::rpc_mock;
        let cred = crate::signing::SigningCredential::from_bip39_mnemonic(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
        )
        .unwrap();
        let wallet = cred.wallet().unwrap();
        for (net, expect) in [
            (Network::Mainnet, None),
            (Network::Testnet, None),
            (Network::Xahau, Some(21337)),
            (Network::XahauTest, Some(21338)),
        ] {
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                .await
                .unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let id = net.network_id();
            let server = tokio::spawn(rpc_mock::serve_raw(listener, 2, move |req| {
                json!({"result": match req["method"].as_str() {
                    Some("server_info") => json!({"info": {"network_id": id}}),
                    _ => json!({"engine_result": "tesSUCCESS", "tx_json": {}}),
                }})
            }));
            let rpc = RpcClient::connect(&url).unwrap();
            rpc.ensure_network(&net).await.unwrap();
            rpc.simulate_tx(json!({"TransactionType": "OfferCancel", "OfferSequence": 1}))
                .await
                .unwrap();
            let requests = server.await.unwrap().unwrap();
            assert_eq!(
                requests[1]["params"][0]["tx_json"].get("NetworkID"),
                expect.map(|n: u32| json!(n)).as_ref(),
                "{net:?} simulate"
            );
            let blob = crate::signing::with_network_id(rpc.network_id(), || {
                crate::signing::create_and_sign_offer_cancel(
                    &wallet,
                    &wallet.classic_address,
                    1,
                    2,
                    12,
                    99,
                )
            })
            .unwrap();
            let decoded = xrpl::core::binarycodec::decode(&blob).unwrap();
            assert_eq!(
                decoded.get("NetworkID").cloned(),
                expect.map(|n: u32| json!(n)),
                "{net:?} blob"
            );
        }
    }

    /// Lost submit reply: one submit attempt only, then the local hash is looked up and the
    /// real outcome reported.
    #[tokio::test]
    async fn lost_submit_reply_resolves_via_tx_lookup_without_replaying() {
        let (url, server, seen) = lossy_node(json!({"validated": true,
            "meta": {"TransactionResult": "tesSUCCESS"}}))
        .await;
        let rpc = RpcClient::connect(&url).unwrap();
        let out = rpc.submit_signed_tx("120000").await.unwrap();
        assert_eq!(
            out.hash,
            "DC71B939905587CDF8410B02ACA9CC5E2BED966CC46C2D524EA08B62E2437A7D"
        );
        assert!(!out.queued);
        assert_eq!(*seen.lock().unwrap(), ["submit", "tx"]);
        server.abort();
    }

    /// Applied with a failure code after the reply was lost → error naming the code.
    #[tokio::test]
    async fn lost_submit_reply_reports_applied_failure() {
        let (url, server, _) = lossy_node(json!({"validated": true,
            "meta": {"TransactionResult": "tecUNFUNDED_PAYMENT"}}))
        .await;
        let rpc = RpcClient::connect(&url).unwrap();
        let err = rpc
            .submit_signed_tx("120000")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("tecUNFUNDED_PAYMENT"), "{err}");
        server.abort();
    }

    /// Not found after the lookups → explicit unknown-outcome error carrying the hash.
    #[tokio::test]
    async fn lost_submit_reply_not_found_is_explicit_unknown() {
        let (url, server, seen) = lossy_node(json!({"error": "txnNotFound", "error_code": 29,
                "error_message": "Transaction not found."}))
        .await;
        let rpc = RpcClient::connect(&url).unwrap();
        let err = rpc
            .submit_signed_tx("120000")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("outcome unknown"), "{err}");
        assert!(err.contains("DC71B939905587CD"), "{err}");
        assert_eq!(
            seen.lock()
                .unwrap()
                .iter()
                .filter(|m| *m == "submit")
                .count(),
            1
        );
        server.abort();
    }
}
