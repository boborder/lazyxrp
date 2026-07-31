use crate::components::shared::theme;
use ratatui::text::{Line, Span};
use serde_json::Value;

/// Format an xrpl Currency for display.
/// Push Account, Sequence, Fee directly from a serde_json::Value without cloning.
pub(crate) fn push_common_lines_from_value(lines: &mut Vec<Line>, tx: &Value) {
    let accent = theme::accent_style();
    if let Some(account) = tx.get("Account").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Account", accent),
            Span::raw(": "),
            Span::styled(account.to_string(), accent),
        ]));
    }
    if let Some(seq) = tx.get("Sequence").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("Sequence", accent),
            Span::raw(": "),
            Span::styled(seq.to_string(), accent),
        ]));
    }
    if let Some(fee) = tx.get("Fee").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Fee", accent),
            Span::raw(": "),
            Span::styled(crate::xrpl::drops_to_xrp(fee), theme::dim_style()),
        ]));
    }
}

/// Format a transaction Amount field directly from serde_json::Value (no clone).
pub(crate) fn fmt_xrpl_amount_from_value(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        crate::xrpl::drops_to_xrp(s)
    } else if let Some(obj) = value.as_object() {
        let currency = obj.get("currency").and_then(Value::as_str).unwrap_or("?");
        let amount_value = obj.get("value").and_then(Value::as_str).unwrap_or("0");
        if let Some(issuer) = obj.get("issuer").and_then(Value::as_str) {
            format!("{amount_value} {currency} (issuer: {issuer})")
        } else {
            format!("{amount_value} {currency}")
        }
    } else {
        value.to_string()
    }
}

pub(crate) fn format_value(key: &str, value: &Value) -> String {
    // Fast-path for amount-bearing fields without cloning the Value
    if key.ends_with("Amount")
        || key == "Fee"
        || key == "SendMax"
        || key == "DeliverMin"
        || key == "Balance"
        || key == "TakerGets"
        || key == "TakerPays"
    {
        if let Some(s) = value.as_str() {
            if let Ok(drops) = s.parse::<u64>() {
                return format!("{:.6} XRP", drops as f64 / 1_000_000.0);
            }
            return s.to_string();
        } else if let Some(n) = value.as_u64() {
            return format!("{:.6} XRP", n as f64 / 1_000_000.0);
        } else if let Some(obj) = value.as_object()
            && obj.contains_key("currency")
            && obj.contains_key("value")
        {
            let currency = obj.get("currency").and_then(Value::as_str).unwrap_or("?");
            let amount_value = obj.get("value").and_then(Value::as_str).unwrap_or("0");
            if let Some(issuer) = obj.get("issuer").and_then(Value::as_str) {
                return format!("{amount_value} {currency} (issuer: {issuer})");
            }
            return format!("{amount_value} {currency}");
        }
    }

    match value {
        Value::String(s) => {
            if key == "Domain" {
                return hex_to_ascii(s).unwrap_or_else(|| s.clone());
            }
            s.clone()
        }
        Value::Object(o) => {
            if o.contains_key("currency") && o.contains_key("value") {
                let currency = o.get("currency").and_then(Value::as_str).unwrap_or("?");
                let amount_value = o.get("value").and_then(Value::as_str).unwrap_or("0");
                return format!("{amount_value} {currency}");
            }
            let s = value.to_string();
            if s.len() > 80 {
                format!("{}…", &s[..80])
            } else {
                s
            }
        }
        Value::Array(arr) => format!("[{} items]", arr.len()),
        _ => value.to_string(),
    }
}
pub(crate) fn hex_to_ascii(hex: &str) -> Option<String> {
    if hex.is_empty() {
        return Some(String::new());
    }
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hex_to_ascii_basic() {
        assert_eq!(hex_to_ascii("68656c6c6f"), Some("hello".to_string()));
    }

    #[test]
    fn hex_to_ascii_empty() {
        assert_eq!(hex_to_ascii(""), Some(String::new()));
    }

    #[test]
    fn hex_to_ascii_odd_length_returns_none() {
        assert_eq!(hex_to_ascii("68656"), None);
    }

    #[test]
    fn hex_to_ascii_invalid_hex_returns_none() {
        assert_eq!(hex_to_ascii("zzzz"), None);
    }

    #[test]
    fn hex_to_ascii_non_utf8_returns_none() {
        // 0x80 is not valid UTF-8 start byte
        assert_eq!(hex_to_ascii("80"), None);
    }

    #[test]
    fn fmt_xrpl_amount_from_value_xrp() {
        assert_eq!(fmt_xrpl_amount_from_value(&json!("1000000")), "1.000000");
    }

    #[test]
    fn fmt_xrpl_amount_from_value_issued() {
        let v = json!({"value":"100","currency":"USD","issuer":"rsA2LpG"});
        assert_eq!(fmt_xrpl_amount_from_value(&v), "100 USD (issuer: rsA2LpG)");
    }

    #[test]
    fn push_common_lines_from_value_account_only() {
        let mut lines = Vec::new();
        push_common_lines_from_value(&mut lines, &json!({"Account":"rTest"}));
        assert_eq!(lines.len(), 1);
        assert!(lines[0].to_string().contains("rTest"));
    }

    #[test]
    fn push_common_lines_from_value_all_fields() {
        let mut lines = Vec::new();
        push_common_lines_from_value(
            &mut lines,
            &json!({"Account":"rTest","Sequence":42,"Fee":"1000"}),
        );
        assert_eq!(lines.len(), 3);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        assert!(text[0].contains("rTest"));
        assert!(text[1].contains("42"));
        assert!(text[2].contains("0.001000"));
    }

    #[test]
    fn format_value_xrp_drops_string() {
        let v = json!("1000000");
        assert_eq!(format_value("Amount", &v), "1.000000 XRP");
    }

    #[test]
    fn format_value_xrp_drops_parse_failure_falls_back() {
        let v = json!("not_a_number");
        assert_eq!(format_value("Amount", &v), "not_a_number");
    }

    #[test]
    fn format_value_issued_currency() {
        let v = json!({"value":"100","currency":"USD","issuer":"rsA2LpG"});
        assert_eq!(format_value("Amount", &v), "100 USD (issuer: rsA2LpG)");
    }

    #[test]
    fn format_value_domain_hex() {
        let v = json!("6578616d706c652e636f6d");
        assert_eq!(format_value("Domain", &v), "example.com");
    }

    #[test]
    fn format_value_domain_hex_invalid_fallback() {
        let v = json!("zzzz");
        // hex_to_ascii returns None for invalid hex, so unwrap_or_else falls back to raw string
        assert_eq!(format_value("Domain", &v), "zzzz");
    }

    #[test]
    fn format_value_plain_string() {
        let v = json!("hello");
        assert_eq!(format_value("Memo", &v), "hello");
    }

    #[test]
    fn format_value_currency_object_without_issuer() {
        let v = json!({"currency":"EUR","value":"50"});
        // Without issuer it does not parse as IssuedCurrencyAmount; should fall back to generic object formatting
        assert_eq!(format_value("LimitAmount", &v), "50 EUR");
    }

    #[test]
    fn format_value_issued_currency_with_issuer() {
        let v = json!({"currency":"EUR","value":"50","issuer":"rsA2LpG"});
        assert_eq!(format_value("LimitAmount", &v), "50 EUR (issuer: rsA2LpG)");
    }

    #[test]
    fn format_value_long_object_truncated() {
        let v = json!({"a":"x".repeat(100)});
        let full = v.to_string();
        assert!(full.len() > 80, "fixture must exceed truncate threshold");
        let result = format_value("Foo", &v);
        assert!(result.ends_with('…'));
        assert_eq!(&result.as_bytes()[..80], &full.as_bytes()[..80]);
        assert_eq!(result.len(), 80 + '…'.len_utf8());
    }

    #[test]
    fn format_value_short_object_not_truncated() {
        let v = json!({"a":"accent"});
        let result = format_value("Foo", &v);
        assert!(!result.ends_with('…'));
    }

    #[test]
    fn format_value_array() {
        let v = json!([1, 2, 3]);
        assert_eq!(format_value("Memos", &v), "[3 items]");
    }

    #[test]
    fn format_value_number() {
        let v = json!(42);
        assert_eq!(format_value("Count", &v), "42");
    }

    #[test]
    fn format_value_null() {
        let v = json!(null);
        assert_eq!(format_value("Nothing", &v), "null");
    }
}
