//! JSON-RPC response parsers for XRPL methods.

use serde_json::{Value, json};

use super::format::{decode_uri, format_amount, format_asset};
use super::json_util::{extract_json_u32, json_str};
use super::types::{
    AccountTxPage, AggregatePrice, AmmSummary, ArcValue, FeeSummary, LedgerObjectRow,
    NFTOKEN_FLAG_MUTABLE, NftRow, NodeValidatorListSummary, OfferRow, PathAlternative, PriceStats,
    RipplePathFindResult, ServerInfoSummary, SimulateResult, TrustLineRow, TxRow, TxSummary,
    WalletProposeResult,
};

pub(crate) fn parse_submit_success(value: &Value) -> color_eyre::Result<TxSummary> {
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

pub(crate) fn parse_simulate_result(value: &Value) -> color_eyre::Result<SimulateResult> {
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

pub(crate) fn parse_ripple_path_find(value: &Value) -> color_eyre::Result<RipplePathFindResult> {
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

pub(crate) fn parse_aggregate_price_value(value: &Value) -> color_eyre::Result<AggregatePrice> {
    let result = value.get("result").unwrap_or(&Value::Null);

    let entire_set = result
        .get("entire_set")
        .ok_or_else(|| color_eyre::eyre::eyre!("get_aggregate_price: missing entire_set"))?;
    let entire = PriceStats {
        mean: entire_set
            .get("mean")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        size: entire_set.get("size").and_then(Value::as_u64).unwrap_or(0) as u32,
        standard_deviation: entire_set
            .get("standard_deviation")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    };

    let trimmed_set = result.get("trimmed_set").map(|t| PriceStats {
        mean: t
            .get("mean")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        size: t.get("size").and_then(Value::as_u64).unwrap_or(0) as u32,
        standard_deviation: t
            .get("standard_deviation")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    });

    let time = result.get("time").and_then(Value::as_u64).unwrap_or(0);

    Ok(AggregatePrice {
        entire_set: entire,
        trimmed_set,
        time,
        base_asset: String::new(),
        quote_asset: String::new(),
    })
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn parse_wallet_propose(value: &Value) -> color_eyre::Result<WalletProposeResult> {
    let result = value.get("result").unwrap_or(&Value::Null);

    let master_seed = result
        .get("master_seed")
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "wallet_propose: missing master_seed (node may disable wallet methods on public/Clio RPC)"
            )
        })?;

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

pub(crate) fn parse_server_info_value(value: &Value) -> ServerInfoSummary {
    let info_json = value.get("result").and_then(|v| v.get("info"));
    let quorum = info_json
        .and_then(|i| i.get("validation_quorum"))
        .and_then(Value::as_u64)
        .map(|v| v.min(u32::MAX as u64) as u32);
    let validator_list = info_json
        .and_then(|i| i.get("validator_list"))
        .and_then(parse_node_validator_list);
    ServerInfoSummary {
        ledger_index: extract_json_u32(value, &["result", "info", "validated_ledger", "seq"]),
        hostid: json_str(value, &["result", "info", "hostid"]).to_string(),
        build_version: json_str(value, &["result", "info", "build_version"]).to_string(),
        validation_quorum: quorum,
        validator_list,
    }
}

pub(crate) fn parse_node_validator_list(value: &Value) -> Option<NodeValidatorListSummary> {
    let count = value.get("count").and_then(Value::as_u64)?;
    Some(NodeValidatorListSummary {
        count: count.min(u32::MAX as u64) as u32,
        status: value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("-")
            .to_string(),
        expiration: value
            .get("expiration")
            .and_then(Value::as_str)
            .unwrap_or("-")
            .to_string(),
    })
}

pub(crate) fn parse_fee_value(value: &Value) -> FeeSummary {
    FeeSummary {
        open_ledger_fee_drops: extract_json_u32(value, &["result", "drops", "open_ledger_fee"]),
    }
}

pub(crate) fn parse_account_nfts_value(value: &Value) -> Vec<NftRow> {
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

pub(crate) fn parse_account_lines_value(value: &Value) -> Vec<TrustLineRow> {
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

pub(crate) fn parse_amm_info_value(value: &Value) -> AmmSummary {
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

/// Consume the RPC `Value` and move `tx`/`meta` into `ArcValue` (no deep clone).
pub(crate) fn parse_account_tx_page(mut value: Value, watch_address: &str) -> AccountTxPage {
    let Some(result) = value.get_mut("result").and_then(Value::as_object_mut) else {
        return AccountTxPage {
            rows: vec![],
            marker: None,
        };
    };
    let marker = result.remove("marker");
    let txns = match result.remove("transactions") {
        Some(Value::Array(a)) => a,
        _ => Vec::new(),
    };

    let rows: Vec<TxRow> = txns
        .into_iter()
        .map(|mut entry| {
            let hash_outer = entry
                .get("hash")
                .and_then(Value::as_str)
                .map(str::to_string);
            let ledger_outer = entry.get("ledger_index").and_then(Value::as_u64);

            let tx = entry
                .as_object_mut()
                .and_then(|o| o.remove("tx").or_else(|| o.remove("tx_json")))
                .unwrap_or(Value::Null);
            let meta = entry
                .as_object_mut()
                .and_then(|o| o.remove("meta"))
                .unwrap_or(Value::Null);

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

            let hash = hash_outer
                .or_else(|| tx.get("hash").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| "-".into());
            let ledger_index = ledger_outer
                .or_else(|| tx.get("ledger_index").and_then(Value::as_u64))
                .unwrap_or(0) as u32;

            TxRow {
                hash,
                tx_type: tx
                    .get("TransactionType")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .to_string(),
                ledger_index,
                result: meta
                    .get("TransactionResult")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .to_string(),
                direction,
                tx_json: ArcValue::new(tx),
                meta_json: ArcValue::new(meta),
            }
        })
        .collect();

    AccountTxPage { rows, marker }
}

#[cfg(test)]
fn parse_account_tx_value(value: Value, watch_address: &str) -> Vec<TxRow> {
    parse_account_tx_page(value, watch_address).rows
}

pub(crate) fn parse_book_offers_value(value: &Value) -> Vec<OfferRow> {
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

/// HTTP/RPC rate-limit / overload signals (case-insensitive).
pub(crate) fn is_rate_limited_error(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("rate limited")
        || lower.contains("too many requests")
        || lower.contains("http 429")
        || lower.contains("status 429")
        || lower.contains("(429)")
}

/// Transient XRPL server-side errors: the node is starting up, lagging behind
/// the network, or overloaded. Retrying (possibly against another node in a
/// cluster) can succeed, so these should not fail the request immediately.
pub(crate) fn is_transient_xrpl_error(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    [
        "insufficientnetworkmode",
        "nonetwork",
        "noclosed",
        "nocurrent",
        "toobusy",
        "failedtoforward",
        "amendmentblocked",
    ]
    .iter()
    .any(|k| lower.contains(k))
}

pub(crate) fn book_offer_best_price(value: &Value, invert: bool) -> Option<f64> {
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

pub(crate) fn empty_account_tx_page_on_not_found(
    err: impl std::fmt::Display,
) -> Option<AccountTxPage> {
    if is_not_found_error(&format!("{err}")) {
        Some(AccountTxPage {
            rows: vec![],
            marker: None,
        })
    } else {
        None
    }
}

pub(crate) fn book_currency(currency: &str, issuer: Option<&str>) -> Value {
    if currency.eq_ignore_ascii_case("XRP") {
        json!({ "currency": "XRP" })
    } else {
        json!({
            "currency": currency,
            "issuer": issuer.unwrap_or("")
        })
    }
}

pub(crate) fn parse_account_objects_value(value: &Value) -> Vec<LedgerObjectRow> {
    let Some(arr) = value
        .get("result")
        .and_then(|r| r.get("account_objects"))
        .and_then(Value::as_array)
    else {
        return vec![];
    };
    arr.iter().filter_map(parse_one_ledger_object_row).collect()
}

fn parse_one_ledger_object_row(ledger_object_json: &Value) -> Option<LedgerObjectRow> {
    let ledger_type = ledger_object_json
        .get("LedgerEntryType")?
        .as_str()?
        .to_string();
    let index = ledger_object_json
        .get("index")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            ledger_object_json
                .get("hash")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let detail = summarize_ledger_object(ledger_object_json);
    Some(LedgerObjectRow {
        ledger_type,
        index,
        detail,
        raw_json: ArcValue::new(ledger_object_json.clone()),
    })
}

fn summarize_ledger_object(ledger_object_json: &Value) -> String {
    let ledger_entry_type = json_str(ledger_object_json, &["LedgerEntryType"]);
    let destination = json_str(ledger_object_json, &["Destination"]);
    match ledger_entry_type {
        "Check" => {
            let amt = format_amount(ledger_object_json.get("SendMax"));
            if destination.is_empty() {
                format!("Check · {amt}")
            } else {
                format!("→ {destination} · {amt}")
            }
        }
        "Ticket" => format!(
            "seq {}",
            extract_json_u32(ledger_object_json, &["TicketSequence"])
        ),
        "MPToken" | "MPTokenIssuance" => {
            let mid = json_str(ledger_object_json, &["MPTokenIssuanceID"]);
            let raw_amt = ledger_object_json
                .get("Amount")
                .map(|v| v.to_string())
                .unwrap_or_default();
            format!("MPT · {mid} · {raw_amt}")
        }
        "PayChannel" => {
            let amt = format_amount(ledger_object_json.get("Amount"));
            if destination.is_empty() {
                format!("PayChan · {amt}")
            } else {
                format!("→ {destination} · {amt}")
            }
        }
        "Escrow" => {
            let amt = format_amount(ledger_object_json.get("Amount"));
            if destination.is_empty() {
                format!("Escrow · {amt}")
            } else {
                format!("→ {destination} · {amt}")
            }
        }
        "DepositPreauth" => {
            let authorized_account = json_str(ledger_object_json, &["Authorize"]);
            format!("auth {authorized_account}")
        }
        "SignerList" => format!(
            "quorum {}",
            extract_json_u32(ledger_object_json, &["SignerQuorum"])
        ),
        _ => {
            let raw_json = ledger_object_json.to_string();
            let truncated_json = raw_json.chars().take(88).collect::<String>();
            if raw_json.len() > 88 {
                format!("{truncated_json}…")
            } else {
                truncated_json
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        let rows = parse_account_tx_value(v, "rWatch");
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
                    "hostid": "test-host",
                    "build_version": "2.5.0"
                }
            }
        });
        let sum = parse_server_info_value(&si);
        assert_eq!(sum.ledger_index, 80000000);
        assert_eq!(sum.hostid, "test-host");
        assert_eq!(sum.build_version, "2.5.0");
        assert_eq!(sum.validation_quorum, None);

        let si_vl = json!({
            "result": {
                "info": {
                    "validated_ledger": {"seq": 1},
                    "hostid": "v",
                    "validation_quorum": 28,
                    "validator_list": {
                        "count": 35,
                        "status": "active",
                        "expiration": "01012026000000+00:00"
                    }
                }
            }
        });
        let with_vl = parse_server_info_value(&si_vl);
        assert_eq!(with_vl.validation_quorum, Some(28));
        assert!(with_vl.build_version.is_empty());
        let vl = with_vl.validator_list.as_ref().expect("validator_list");
        assert_eq!(vl.count, 35);
        assert_eq!(vl.status, "active");

        let fee = json!({"result": {"drops": {"open_ledger_fee": 15}}});
        let fs = parse_fee_value(&fee);
        assert_eq!(fs.open_ledger_fee_drops, 15);
    }

    /// TC-001
    #[test]
    fn book_currency_xrp_uppercase() {
        let v = book_currency("XRP", None);
        assert_eq!(v["currency"], "XRP");
        assert!(v.get("issuer").is_none());
    }

    /// TC-002
    #[test]
    fn book_currency_xrp_case_insensitive() {
        let v = book_currency("xrp", Some("rIssuer"));
        assert_eq!(v["currency"], "XRP");
        assert!(v.get("issuer").is_none());
    }

    /// TC-003
    #[test]
    fn book_currency_issued_includes_issuer() {
        let v = book_currency("USD", Some("rIssuer"));
        assert_eq!(v["currency"], "USD");
        assert_eq!(v["issuer"], "rIssuer");
    }

    /// TC-089 (I-7): `account_tx` RPC not-found maps to empty page at client boundary
    #[test]
    fn empty_account_tx_page_on_not_found_maps_actnotfound() {
        let page = empty_account_tx_page_on_not_found("actNotFound")
            .expect("not-found should become empty page");
        assert!(page.rows.is_empty());
        assert!(page.marker.is_none());
    }

    #[test]
    fn empty_account_tx_page_on_not_found_ignores_other_errors() {
        assert!(empty_account_tx_page_on_not_found("timeout").is_none());
    }

    #[test]
    fn is_rate_limited_error_matches_common_signals() {
        assert!(is_rate_limited_error("Rate limited"));
        assert!(is_rate_limited_error("HTTP 429 from upstream"));
        assert!(is_rate_limited_error("Error: Too Many Requests"));
        assert!(!is_rate_limited_error("timeout"));
        assert!(!is_rate_limited_error("actNotFound"));
    }

    #[test]
    fn is_transient_xrpl_error_matches_node_state_signals() {
        assert!(is_transient_xrpl_error("[XRPL-17] InsufficientNetworkMode"));
        assert!(is_transient_xrpl_error("[XRPL-18] noNetwork"));
        assert!(is_transient_xrpl_error("noClosed"));
        assert!(is_transient_xrpl_error("noCurrent"));
        assert!(is_transient_xrpl_error("tooBusy"));
        assert!(is_transient_xrpl_error("failedToForward"));
        assert!(!is_transient_xrpl_error("[XRPL-19] actNotFound"));
        assert!(!is_transient_xrpl_error("[XRPL-5] malformedRequest"));
        assert!(!is_transient_xrpl_error("timeout"));
    }

    #[test]
    fn is_not_found_error_matches_skill_substrings() {
        assert!(is_not_found_error("actNotFound"));
        assert!(is_not_found_error("entryNotFound"));
        assert!(is_not_found_error("lgrNotFound"));
        assert!(is_not_found_error("Object not found"));
        assert!(is_not_found_error("Account not found."));
        assert!(!is_not_found_error("tecUNFUNDED_PAYMENT"));
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
        let err = parse_submit_success(&failed).expect_err("tec must fail");
        assert!(format!("{err}").contains("tecNO_DST_INSUF_XRP"));
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
        assert_eq!(
            sim.meta.as_ref().and_then(|m| m.get("TransactionResult")),
            Some(&json!("tesSUCCESS"))
        );
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
        assert!(rows[0].detail.contains("2.000000"));

        assert_eq!(rows[1].ledger_type, "Ticket");
        assert_eq!(rows[1].index, "TIK1");
        assert!(rows[1].detail.contains('7'));

        assert_eq!(rows[2].ledger_type, "MPToken");
        assert_eq!(rows[2].index, "MPT1");
        assert!(rows[2].detail.contains("001122"));
        assert!(rows[2].detail.contains("99"));

        assert_eq!(rows[3].ledger_type, "PayChannel");
        assert_eq!(rows[3].index, "PC1");
        assert!(rows[3].detail.contains("rDestPC"));
        assert!(rows[3].detail.contains("5.000000"));

        assert_eq!(rows[4].ledger_type, "Escrow");
        assert_eq!(rows[4].index, "ES1");
        assert!(rows[4].detail.contains("rDestE"));
        assert!(rows[4].detail.contains("1.000000"));
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

    /// TC-080 ripple_path_find parse
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

    /// TC-081 ripple_path_find empty alternatives
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

    /// TC-082 ripple_path_find source_amount as string (XRP drops)
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

    /// TC-095 wallet_propose ed25519
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
        assert_eq!(r.master_seed, "sEdTzYqD8TKiF4MjRmq9h5RZVvqQeGF");
        assert_eq!(r.account_id, "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh");
        assert_eq!(
            r.public_key,
            "aBQG8RQAzjs1eTKFEAQXr2gSJutMrk9oXqVtYN7qFZjNn82BScnG"
        );
        assert_eq!(r.key_type, "ed25519");
        assert_eq!(r.master_seed_hex, "DEDCE9CE67B451D852FD4E846FCDE31C");
        assert_eq!(
            r.public_key_hex,
            "ED74D4036C6591A4BDF9C54CEFA39B996A5DCE5F86D11FDA1878C3A9E45606A5AB"
        );
    }

    /// TC-096: get_aggregate_price parser — full response
    #[test]
    fn parse_aggregate_price_full() {
        let value = json!({
            "result": {
                "entire_set": {
                    "mean": "0.5234",
                    "size": 3,
                    "standard_deviation": "0.0012"
                },
                "trimmed_set": {
                    "mean": "0.5233",
                    "size": 2,
                    "standard_deviation": "0.0008"
                },
                "time": 1715779200
            }
        });
        let price = parse_aggregate_price_value(&value).expect("should parse");
        assert_eq!(price.entire_set.mean, "0.5234");
        assert_eq!(price.entire_set.size, 3);
        assert_eq!(price.entire_set.standard_deviation, "0.0012");
        assert_eq!(price.trimmed_set.as_ref().unwrap().mean, "0.5233");
        assert_eq!(price.trimmed_set.as_ref().unwrap().size, 2);
        assert_eq!(price.time, 1715779200);
    }

    /// TC-097: get_aggregate_price parser — trimmed_set omitted
    #[test]
    fn parse_aggregate_price_no_trim() {
        let value = json!({
            "result": {
                "entire_set": {
                    "mean": "1.0",
                    "size": 1,
                    "standard_deviation": "0"
                },
                "time": 0
            }
        });
        let price = parse_aggregate_price_value(&value).expect("should parse without trim");
        assert_eq!(price.entire_set.mean, "1.0");
        assert!(price.trimmed_set.is_none());
        assert_eq!(price.time, 0);
    }
}
