//! Pure formatting helpers for XRPL amounts, paths, and time.

use serde_json::Value;

use super::types::{
    ArcValue, PathFindRow, PathFindSnapshot, RipplePathFindResult, asset_display_name,
};

/// Ripple epoch seconds (2000-01-01 UTC) → `YYYY-MM-DD HH:MM UTC`.
pub fn format_ripple_time_utc(seconds: u64) -> String {
    const RIPPLE_EPOCH_UNIX: i64 = 946_684_800;
    let unix = RIPPLE_EPOCH_UNIX
        .saturating_add(seconds.min(i64::MAX as u64 - RIPPLE_EPOCH_UNIX as u64) as i64);
    let secs = unix.rem_euclid(86_400);
    let days = unix.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {hh:02}:{mm:02} UTC",
        hh = secs / 3600,
        mm = (secs % 3600) / 60
    )
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 {
        z / 146_097
    } else {
        (z - 146_096) / 146_097
    };
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    if m <= 2 {
        y += 1;
    }
    (y, m, d)
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

pub(crate) fn decode_uri(hex: &str) -> String {
    if hex.is_empty() {
        return String::new();
    }
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect();
    String::from_utf8(bytes).unwrap_or_else(|_| hex.to_string())
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

/// Human-readable label for a `ripple_path_find` destination_amount field.
pub fn format_path_destination(value: &Value, quote_label: &str) -> String {
    if let Some(s) = value.as_str() {
        return format!("{} XRP", drops_to_xrp(s));
    }
    if let Some(obj) = value.as_object() {
        let currency = obj
            .get("currency")
            .and_then(Value::as_str)
            .unwrap_or(quote_label);
        let value_str = obj.get("value").and_then(Value::as_str).unwrap_or("-");
        if currency.eq_ignore_ascii_case("XRP") {
            return format!("{} XRP", drops_to_xrp(value_str));
        }
        return format!("{value_str} {}", asset_display_name(currency));
    }
    "-".to_string()
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

/// Source amount for path-find rows (always includes currency, e.g. `1.000000 XRP`).
pub fn format_path_source_amount(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        return format!("{} XRP", drops_to_xrp(s));
    }
    if let Some(obj) = value.as_object() {
        let currency = obj.get("currency").and_then(Value::as_str).unwrap_or("?");
        let value_str = obj.get("value").and_then(Value::as_str).unwrap_or("-");
        if currency.eq_ignore_ascii_case("XRP") {
            return format!("{} XRP", drops_to_xrp(value_str));
        }
        return format!("{value_str} {}", asset_display_name(currency));
    }
    "-".into()
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
        dest_summary: format_path_destination(&result.destination_amount, quote_label),
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
                send: format_path_source_amount(&alt.source_amount),
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
    fn format_path_source_amount_hex_currency() {
        let amount = json!({
            "currency": "524C555344000000000000000000000000000000",
            "issuer": "rIssuer",
            "value": "1.05"
        });
        assert_eq!(format_path_source_amount(&amount), "1.05 RLUSD");
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
