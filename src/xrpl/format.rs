//! Pure formatting helpers for XRPL amounts, paths, and time.

use time::OffsetDateTime;

use serde_json::Value;

use super::types::{
    ArcValue, PathFindRow, PathFindSnapshot, RIPPLE_EPOCH_UNIX, RipplePathFindResult,
    asset_display_name,
};

/// Ripple epoch seconds (2000-01-01 UTC) → `YYYY-MM-DD HH:MM UTC`.
pub fn format_ripple_time_utc(seconds: u64) -> String {
    let unix = RIPPLE_EPOCH_UNIX
        .saturating_add(seconds.min(i64::MAX as u64 - RIPPLE_EPOCH_UNIX as u64) as i64);
    // `time` (without `large-dates`) represents years -9999..=9999; clamp the
    // rare absurd-seconds case to 9999-12-31T23:59:59Z so conversion is total.
    let dt = OffsetDateTime::from_unix_timestamp(unix.min(253_402_300_799))
        .expect("clamped timestamp is representable");
    let (y, m, d) = (dt.year(), u8::from(dt.month()), dt.day());
    format!(
        "{y:04}-{m:02}-{d:02} {hh:02}:{mm:02} UTC",
        hh = dt.hour(),
        mm = dt.minute()
    )
}

pub fn xrp_to_drops(xrp: &str) -> color_eyre::Result<u64> {
    const DROPS_PER_XRP: u64 = 1_000_000;
    let overflow = || color_eyre::eyre::eyre!("XRP amount overflow");
    match xrp.split_once('.') {
        None => {
            let whole = xrp.parse::<u64>()?;
            whole.checked_mul(DROPS_PER_XRP).ok_or_else(overflow)
        }
        Some((whole, frac)) => {
            if frac.len() > 6 {
                return Err(color_eyre::eyre::eyre!(
                    "XRP amount can only have up to 6 decimal places"
                ));
            }
            let whole: u64 = whole.parse()?;
            let frac: u64 = format!("{frac:0<6}").parse()?;
            let whole_drops = whole.checked_mul(DROPS_PER_XRP).ok_or_else(overflow)?;
            whole_drops.checked_add(frac).ok_or_else(overflow)
        }
    }
}

pub fn drops_to_xrp(drops: &str) -> String {
    let drops_num = drops.parse::<f64>().unwrap_or_default();
    format!("{:.6}", drops_num / 1_000_000.0)
}

pub(crate) fn hex_to_ascii(hex: &str) -> Option<String> {
    hex::decode(hex)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

pub(crate) fn format_asset(v: Option<&Value>) -> String {
    match v {
        Some(Value::Object(m)) => m
            .get("currency")
            .and_then(Value::as_str)
            .unwrap_or("XRP")
            .to_string(),
        _ => "XRP".to_string(),
    }
}

/// Amount field for path-find rows. Strings are XRP drops; objects are
/// `{currency, value}` where the currency label falls back to `fallback_label`
/// when missing.
pub fn format_path_amount(value: &Value, fallback_label: &str) -> String {
    match json_amount(value) {
        Some(JsonAmount::XrpDrops(drops)) => format!("{} XRP", drops_to_xrp(drops)),
        Some(JsonAmount::Issued {
            currency,
            value: amount,
            ..
        }) if currency.eq_ignore_ascii_case("XRP") => {
            format!("{} XRP", drops_to_xrp(amount))
        }
        Some(JsonAmount::Issued {
            currency,
            value: amount,
            ..
        }) => {
            format!("{amount} {}", asset_display_name(currency))
        }
        None if value.is_object() => {
            let obj = value.as_object().expect("checked is_object");
            let currency = obj
                .get("currency")
                .and_then(Value::as_str)
                .unwrap_or(fallback_label);
            let amount = obj.get("value").and_then(Value::as_str).unwrap_or("-");
            if currency.eq_ignore_ascii_case("XRP") {
                return format!("{} XRP", drops_to_xrp(amount));
            }
            format!("{amount} {}", asset_display_name(currency))
        }
        None => "-".to_string(),
    }
}

/// Short summary of the first computed path (currency/issuer hops).
pub fn summarize_paths_computed(paths_computed: &Value) -> String {
    let Some(paths) = paths_computed.as_array() else {
        return "-".into();
    };
    let Some(first_path) = paths.first().and_then(Value::as_array) else {
        return if paths.is_empty() {
            "-".into()
        } else {
            "direct".into()
        };
    };
    if first_path.is_empty() {
        return "direct".into();
    }
    let mut parts = Vec::with_capacity(first_path.len());
    for step in first_path {
        let step_label = path_step_label(step);
        if parts.last() != Some(&step_label) {
            parts.push(step_label);
        }
    }
    if parts.is_empty() {
        "-".into()
    } else {
        parts.join(" → ")
    }
}

fn path_step_label(step: &Value) -> String {
    if let Some(account) = step.as_str() {
        return shorten_r_address(account);
    }
    if let Some(account) = step.get("account").and_then(Value::as_str) {
        return shorten_r_address(account);
    }
    let currency = step.get("currency").and_then(Value::as_str).unwrap_or("?");
    if currency.eq_ignore_ascii_case("XRP") {
        return "XRP".into();
    }
    let asset_label = asset_display_name(currency);
    if let Some(issuer) = step.get("issuer").and_then(Value::as_str) {
        format!("{asset_label}@{}", shorten_r_address(issuer))
    } else {
        asset_label
    }
}

fn shorten_r_address(addr: &str) -> String {
    // Char-based: path steps are remote JSON strings and may contain multibyte characters.
    let n = addr.chars().count();
    if n > 10 {
        let head: String = addr.chars().take(4).collect();
        let tail: String = addr.chars().skip(n - 4).collect();
        format!("{head}…{tail}")
    } else {
        addr.to_string()
    }
}

pub fn path_hop_count(paths_computed: &Value) -> usize {
    paths_computed
        .as_array()
        .and_then(|paths| paths.first())
        .and_then(Value::as_array)
        .map_or(0, |p| p.len())
}

/// Human-readable hop count for the Path-Find table (`direct` / `1 hop` / `N hops`).
pub fn format_path_hops_label(hop_count: usize) -> String {
    match hop_count {
        0 => "direct".into(),
        1 => "1 hop".into(),
        n => format!("{n} hops"),
    }
}

fn source_amount_sort_key(value: &Value) -> f64 {
    // Non-finite values (e.g. remote "NaN") sort last, like unparsable ones.
    let parse = |s: &str| s.parse::<f64>().ok().filter(|v| v.is_finite());
    value
        .as_str()
        .and_then(parse)
        .or_else(|| value.get("value").and_then(Value::as_str).and_then(parse))
        .unwrap_or(f64::MAX)
}

pub fn path_find_snapshot(result: &RipplePathFindResult, quote_label: &str) -> PathFindSnapshot {
    PathFindSnapshot {
        dest_summary: format_path_amount(&result.destination_amount, quote_label),
        rows: path_find_rows_from(result),
    }
}

pub fn path_find_rows_from(result: &RipplePathFindResult) -> Vec<PathFindRow> {
    let mut alternatives = result.alternatives.clone();
    alternatives.sort_by(|a, b| {
        source_amount_sort_key(&a.source_amount)
            .total_cmp(&source_amount_sort_key(&b.source_amount))
            .then_with(|| path_hop_count(&a.paths_computed).cmp(&path_hop_count(&b.paths_computed)))
    });
    alternatives
        .iter()
        .map(|alt| {
            let hops_n = path_hop_count(&alt.paths_computed);
            PathFindRow {
                send: format_path_amount(&alt.source_amount, "?"),
                hops: format_path_hops_label(hops_n),
                path: summarize_paths_computed(&alt.paths_computed),
                raw_json: ArcValue::new(serde_json::json!({
                    "source_amount": alt.source_amount,
                    "paths_computed": alt.paths_computed,
                })),
            }
        })
        .collect()
}

/// `ripple_path_find` `source_currencies` entry for a Payment quote.
/// The Send max field's currency prefix (`XRP`, `CUR:issuer[:value]`) wins; empty field
/// with an IOU Amount defaults to XRP. An XRP Amount needs a non-XRP source.
pub fn quote_source_currency(send_max_field: &str, amount_is_iou: bool) -> Result<Value, String> {
    let mut parts = send_max_field.trim().splitn(3, ':');
    let currency = parts.next().unwrap_or_default().to_ascii_uppercase();
    let issuer = parts.next().map(str::trim).filter(|s| !s.is_empty());
    match (currency.as_str(), issuer) {
        ("", _) if amount_is_iou => Ok(serde_json::json!({"currency": "XRP"})),
        ("", _) => Err("set Send max currency first (CUR:issuer), then quote".into()),
        ("XRP", _) if amount_is_iou => Ok(serde_json::json!({"currency": "XRP"})),
        ("XRP", _) => Err("XRP→XRP needs no quote; set Send max to CUR:issuer".into()),
        (cur, Some(iss)) => Ok(serde_json::json!({"currency": cur, "issuer": iss})),
        (_, None) => Err("Send max IOU source needs CUR:issuer".into()),
    }
}

/// Cheapest `ripple_path_find` alternative in the requested source currency + `slippage_bps`
/// (rounded up) → Send max spec (`XRP:drops` / `CUR:issuer:value`). Alternatives in any other
/// currency/issuer are ignored: the node must not pick what the signer commits to.
pub fn send_max_spec_from_quote(
    result: &RipplePathFindResult,
    requested: &Value,
    slippage_bps: u32,
) -> Result<String, String> {
    let want_currency = requested
        .get("currency")
        .and_then(Value::as_str)
        .unwrap_or("XRP");
    let want_issuer = requested.get("issuer").and_then(Value::as_str);
    let matches = |amount: &Value| match json_amount(amount) {
        Some(JsonAmount::XrpDrops(_)) => want_currency == "XRP",
        Some(JsonAmount::Issued {
            currency, issuer, ..
        }) => currency == want_currency && issuer.is_some() && issuer == want_issuer,
        None => false,
    };
    let best = result
        .alternatives
        .iter()
        .filter(|a| matches(&a.source_amount))
        .min_by(|a, b| {
            source_amount_sort_key(&a.source_amount)
                .total_cmp(&source_amount_sort_key(&b.source_amount))
        })
        .ok_or("no payment path found in the requested source currency")?;
    match json_amount(&best.source_amount) {
        Some(JsonAmount::XrpDrops(d)) => {
            let drops: u128 = d.parse().map_err(|_| format!("bad source drops: {d}"))?;
            let bps = u128::from(slippage_bps);
            drops
                .checked_mul(10_000 + bps)
                .map(|v| format!("XRP:{}", v.div_ceil(10_000)))
                .ok_or_else(|| format!("source drops out of range: {d}"))
        }
        Some(JsonAmount::Issued {
            currency,
            value,
            issuer: Some(issuer),
        }) => {
            let v: f64 = value
                .parse()
                .ok()
                .filter(|v: &f64| v.is_finite() && *v > 0.0)
                .ok_or_else(|| format!("bad source value: {value}"))?;
            let with_slip = v * (1.0 + f64::from(slippage_bps) / 10_000.0);
            // ponytail: round up at 6 significant digits in f64 (SendMax is a ceiling, so
            // slight over-allowance is safe); switch to decimal math if users need exact values.
            let decimals = (5 - with_slip.log10().floor() as i32).max(0);
            let scale = 10f64.powi(decimals);
            let rounded = ((with_slip * scale) - 1e-9).ceil() / scale;
            let text = format!("{rounded:.*}", decimals as usize);
            let text = if text.contains('.') {
                text.trim_end_matches('0').trim_end_matches('.')
            } else {
                &text
            };
            Ok(format!("{currency}:{issuer}:{text}"))
        }
        _ => Err("unsupported source amount in path".into()),
    }
}
/// Decoded XRPL amount JSON: a string is XRP drops; an object with
/// `currency`+`value` is an issued-currency amount.
pub(crate) enum JsonAmount<'a> {
    XrpDrops(&'a str),
    Issued {
        currency: &'a str,
        value: &'a str,
        issuer: Option<&'a str>,
    },
}

/// Classify an XRPL amount JSON value (no allocation).
pub(crate) fn json_amount(value: &Value) -> Option<JsonAmount<'_>> {
    if let Some(s) = value.as_str() {
        return Some(JsonAmount::XrpDrops(s));
    }
    let obj = value.as_object()?;
    Some(JsonAmount::Issued {
        currency: obj.get("currency").and_then(Value::as_str)?,
        value: obj.get("value").and_then(Value::as_str)?,
        issuer: obj.get("issuer").and_then(Value::as_str),
    })
}

pub fn format_amount(value: Option<&Value>) -> String {
    match value {
        None => "-".to_string(),
        Some(v) => match json_amount(v) {
            Some(JsonAmount::XrpDrops(drops)) => drops_to_xrp(drops),
            Some(JsonAmount::Issued {
                currency,
                value: amount,
                ..
            }) => {
                format!("{amount} {currency}")
            }
            None => {
                let currency = v.get("currency").and_then(Value::as_str).unwrap_or("?");
                let amount = v.get("value").and_then(Value::as_str).unwrap_or("0");
                format!("{amount} {currency}")
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::PathAlternative;
    use super::*;
    use serde_json::json;

    /// `drops_to_xrp`: valid drops render with 6 decimals; invalid input
    /// yields "0.000000".
    #[test]
    fn drops_to_xrp_converts() {
        assert_eq!(drops_to_xrp("1000000"), "1.000000");
        assert_eq!(drops_to_xrp("250000"), "0.250000");
        assert_eq!(drops_to_xrp("not-a-number"), "0.000000");
    }

    /// `format_amount`: None → "-", XRP drops string → XRP amount, and an
    /// issued-currency object renders `value CUR`.
    #[test]
    fn format_amount_branches() {
        assert_eq!(format_amount(None), "-");

        let drops = json!("1000000");
        assert_eq!(format_amount(Some(&drops)), "1.000000");

        let issued = json!({"currency": "USD", "value": "1.5", "issuer": "rXyz"});
        assert_eq!(format_amount(Some(&issued)), "1.5 USD");
    }

    /// TC-139: `format_ripple_time_utc` — a known value renders correctly,
    /// epoch zero renders 2000-01-01, and u64::MAX clamps to 9999-12-31
    /// without panicking.
    #[test]
    fn format_ripple_time_utc_values() {
        // Ripple epoch 946684800 + 838204893 = unix 1784889693 → 2026-07-24 10:41 UTC
        assert_eq!(format_ripple_time_utc(838_204_893), "2026-07-24 10:41 UTC");
        assert_eq!(format_ripple_time_utc(0), "2000-01-01 00:00 UTC");
        assert_eq!(format_ripple_time_utc(u64::MAX), "9999-12-31 23:59 UTC");
    }

    /// TC-141: xrp_to_drops — parses whole/fractional XRP into drops,
    /// accepts the u64::MAX boundary (18446744073709.551615 XRP), and
    /// rejects malformed or overflowing inputs.
    #[test]
    fn xrp_to_drops_converts_and_rejects_inputs() {
        let cases: [(&str, Option<u64>); 13] = [
            ("1", Some(1_000_000)),
            ("1.5", Some(1_500_000)),
            ("1.123456", Some(1_123_456)),
            ("0.000001", Some(1)),
            ("0", Some(0)),
            ("18446744073709.551615", Some(u64::MAX)),
            ("1.1234567", None),
            ("", None),
            ("abc", None),
            ("1.2.3", None),
            (".5", None),
            ("18446744073709.551616", None),
            ("18446744073710", None),
        ];
        for (input, expected) in cases {
            assert_eq!(xrp_to_drops(input).ok(), expected, "input: {input:?}");
        }
    }

    #[test]
    fn hex_to_ascii_cases() {
        let cases = [
            ("68656c6c6f", Some("hello".to_string())),
            ("", Some(String::new())),
            ("68656", None),
            ("zzzz", None),
            ("80", None),
        ];
        for (input, expected) in cases {
            assert_eq!(hex_to_ascii(input), expected, "input: {input:?}");
        }
    }

    /// TC-083: `summarize_paths_computed` — multi-hop currency chain,
    /// plain-account string steps, and hex currency decoded to display name.
    #[test]
    fn summarize_paths_computed_variants() {
        let paths = json!([[
            {"currency": "XRP", "type": 16},
            {"currency": "USD", "issuer": "rIssuer1", "type": 48},
            {"currency": "USD", "issuer": "rIssuer2", "type": 48}
        ]]);
        assert_eq!(
            summarize_paths_computed(&paths),
            "XRP → USD@rIssuer1 → USD@rIssuer2"
        );

        let string_step = json!([["rN7n67967NcFqXSBYfSouqMDPMaFmMgfe"]]);
        assert_eq!(summarize_paths_computed(&string_step), "rN7n…Mgfe");

        let hex_currency = json!([[
            {"currency": "524C555344000000000000000000000000000000", "issuer": "rIssuer1", "type": 48}
        ]]);
        assert_eq!(summarize_paths_computed(&hex_currency), "RLUSD@rIssuer1");
    }

    /// Remote multibyte strings must not panic the byte-slicing display helpers, and a
    /// NaN source amount must not break the sort order.
    #[test]
    fn remote_strings_do_not_panic_display_helpers() {
        let multibyte = format!("a{}", "é".repeat(12));
        assert_eq!(shorten_r_address(&multibyte), "aééé…éééé");

        assert_eq!(source_amount_sort_key(&json!("NaN")), f64::MAX);
        assert_eq!(source_amount_sort_key(&json!({"value": "inf"})), f64::MAX);
        assert_eq!(source_amount_sort_key(&json!("1.5")), 1.5);
    }

    #[test]
    fn format_path_amount_hex_currency() {
        let drops = json!("1000000");
        assert_eq!(format_path_amount(&drops, "?"), "1.000000 XRP");

        let amount = json!({
            "currency": "524C555344000000000000000000000000000000",
            "issuer": "rIssuer",
            "value": "1.05"
        });
        assert_eq!(format_path_amount(&amount, "?"), "1.05 RLUSD");
    }

    /// TC-084: path_find_rows_from — send/hops display rows, cheapest alternative first
    #[test]
    fn path_find_rows_rendering() {
        // Single direct alternative.
        let direct = RipplePathFindResult {
            alternatives: vec![PathAlternative {
                paths_computed: json!([]),
                source_amount: json!("1000000"),
            }],
            destination_account: "rDest".into(),
            destination_amount: json!({"currency": "USD", "value": "1"}),
            source_account: "rSrc".into(),
        };
        let rows = path_find_rows_from(&direct);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].send, "1.000000 XRP");
        assert_eq!(rows[0].hops, "direct");

        // Multiple alternatives are sorted by cheapest send.
        let multi = RipplePathFindResult {
            alternatives: vec![
                PathAlternative {
                    paths_computed: json!([]),
                    source_amount: json!("2000000"),
                },
                PathAlternative {
                    paths_computed: json!([]),
                    source_amount: json!("1000000"),
                },
            ],
            destination_account: "rDest".into(),
            destination_amount: json!("1000000"),
            source_account: "rSrc".into(),
        };
        let rows = path_find_rows_from(&multi);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].send, "1.000000 XRP");
        assert_eq!(rows[1].send, "2.000000 XRP");
    }

    /// TC-192: quote source currency — Send max prefix wins, IOU Amount defaults to XRP,
    /// XRP Amount needs a non-XRP prefix, IOU prefix needs an issuer.
    #[test]
    fn quote_source_currency_table() {
        let xrp = json!({"currency": "XRP"});
        let usd = json!({"currency": "USD", "issuer": "rI"});
        assert_eq!(quote_source_currency("", true).unwrap(), xrp);
        assert_eq!(quote_source_currency(" XRP ", true).unwrap(), xrp);
        assert_eq!(quote_source_currency("XRP:5", true).unwrap(), xrp);
        assert_eq!(quote_source_currency("usd:rI", false).unwrap(), usd);
        assert_eq!(quote_source_currency("USD:rI:3", false).unwrap(), usd);
        assert!(
            quote_source_currency("", false)
                .unwrap_err()
                .contains("Send max")
        );
        assert!(quote_source_currency("XRP", false).is_err());
        assert!(quote_source_currency("USD", true).is_err());
    }

    /// TC-193: quote → Send max spec — cheapest alternative, slippage rounds up, no path → Err.
    #[test]
    fn send_max_spec_from_quote_table() {
        let result = |sources: Vec<Value>| RipplePathFindResult {
            alternatives: sources
                .into_iter()
                .map(|source_amount| PathAlternative {
                    paths_computed: json!([]),
                    source_amount,
                })
                .collect(),
            destination_account: "rDest".into(),
            destination_amount: json!("1"),
            source_account: "rSrc".into(),
        };
        let xrp_req = json!({"currency": "XRP"});
        let usd_req = json!({"currency": "USD", "issuer": "rI"});
        let xrp = result(vec![json!("2000000"), json!("1000000")]);
        assert_eq!(
            send_max_spec_from_quote(&xrp, &xrp_req, 100).unwrap(),
            "XRP:1010000"
        );
        assert_eq!(
            send_max_spec_from_quote(&result(vec![json!("1")]), &xrp_req, 1).unwrap(),
            "XRP:2"
        );
        let iou = result(vec![
            json!({"currency": "USD", "issuer": "rI", "value": "10"}),
        ]);
        assert_eq!(
            send_max_spec_from_quote(&iou, &usd_req, 100).unwrap(),
            "USD:rI:10.1"
        );
        assert!(send_max_spec_from_quote(&result(vec![]), &xrp_req, 100).is_err());

        // Lead 2: alternatives not matching the requested source are ignored, even when cheaper.
        let mixed = result(vec![
            json!({"currency": "EUR", "issuer": "rI", "value": "1"}),
            json!({"currency": "USD", "issuer": "rEvil", "value": "1"}),
            json!("5"),
            json!({"currency": "USD", "issuer": "rI", "value": "10"}),
        ]);
        assert_eq!(
            send_max_spec_from_quote(&mixed, &usd_req, 0).unwrap(),
            "USD:rI:10"
        );
        assert!(send_max_spec_from_quote(&iou, &xrp_req, 0).is_err());
        // Overflowing drops is an error, not a wrapped ceiling.
        let huge = result(vec![json!("340282366920938463463374607431768211455")]);
        assert!(send_max_spec_from_quote(&huge, &xrp_req, 100).is_err());
    }
}
