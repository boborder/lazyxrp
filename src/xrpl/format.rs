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
    match xrp.split_once('.') {
        None => Ok(xrp.parse::<u64>()? * DROPS_PER_XRP),
        Some((whole, frac)) => {
            if frac.len() > 6 {
                return Err(color_eyre::eyre::eyre!(
                    "XRP amount can only have up to 6 decimal places"
                ));
            }
            let whole: u64 = whole.parse()?;
            let frac: u64 = format!("{frac:0<6}").parse()?;
            Ok(whole * DROPS_PER_XRP + frac)
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

pub(crate) fn decode_uri(hex: &str) -> String {
    hex_to_ascii(hex).unwrap_or_else(|| hex.to_string())
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
    if addr.len() > 10 {
        format!("{}…{}", &addr[..4], &addr[addr.len() - 4..])
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
    if let Some(s) = value.as_str() {
        s.parse::<f64>().unwrap_or(f64::MAX)
    } else if let Some(v) = value.get("value").and_then(Value::as_str) {
        v.parse::<f64>().unwrap_or(f64::MAX)
    } else {
        f64::MAX
    }
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
            .partial_cmp(&source_amount_sort_key(&b.source_amount))
            .unwrap_or(std::cmp::Ordering::Equal)
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
    fn format_ripple_time_utc_known_value() {
        // Ripple epoch 946684800 + 838204893 = unix 1784889693 → 2026-07-24 10:41 UTC
        assert_eq!(format_ripple_time_utc(838_204_893), "2026-07-24 10:41 UTC");
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

    /// TC-083 summarize_paths_computed abbreviates hop chain
    #[test]
    fn summarize_paths_computed_multi_hop() {
        let paths = json!([[
            {"currency": "XRP", "type": 16},
            {"currency": "USD", "issuer": "rIssuer1", "type": 48},
            {"currency": "USD", "issuer": "rIssuer2", "type": 48}
        ]]);
        assert_eq!(
            summarize_paths_computed(&paths),
            "XRP → USD@rIssuer1 → USD@rIssuer2"
        );
    }

    #[test]
    fn summarize_paths_computed_string_account_step() {
        let paths = json!([["rN7n67967NcFqXSBYfSouqMDPMaFmMgfe"]]);
        assert_eq!(summarize_paths_computed(&paths), "rN7n…Mgfe");
    }

    #[test]
    fn summarize_paths_computed_hex_currency_display_name() {
        let paths = json!([[
            {"currency": "524C555344000000000000000000000000000000", "issuer": "rIssuer1", "type": 48}
        ]]);
        assert_eq!(summarize_paths_computed(&paths), "RLUSD@rIssuer1");
    }

    #[test]
    fn format_path_amount_hex_currency() {
        let amount = json!({
            "currency": "524C555344000000000000000000000000000000",
            "issuer": "rIssuer",
            "value": "1.05"
        });
        assert_eq!(format_path_amount(&amount, "?"), "1.05 RLUSD");
    }

    /// TC-084 path_find_rows_from builds display rows
    #[test]
    fn path_find_rows_from_alternatives() {
        let result = RipplePathFindResult {
            alternatives: vec![PathAlternative {
                paths_computed: json!([]),
                source_amount: json!("1000000"),
            }],
            destination_account: "rDest".into(),
            destination_amount: json!({"currency": "USD", "value": "1"}),
            source_account: "rSrc".into(),
        };
        let rows = path_find_rows_from(&result);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].send, "1.000000 XRP");
        assert_eq!(rows[0].hops, "direct");
    }

    #[test]
    fn path_find_rows_sorted_by_cheapest_send() {
        let result = RipplePathFindResult {
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
        let rows = path_find_rows_from(&result);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].send, "1.000000 XRP");
        assert_eq!(rows[1].send, "2.000000 XRP");
    }
}
