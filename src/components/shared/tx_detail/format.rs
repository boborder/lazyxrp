use crate::components::shared::theme;
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use serde_json::Value;
use xrpl::models::Amount;

pub(crate) fn fmt_xrpl_amount(amount: &Amount) -> String {
    match amount {
        Amount::XRPAmount(xrp) => crate::xrpl::drops_to_xrp(&xrp.to_string()),
        Amount::IssuedCurrencyAmount(ic) => {
            format!("{} {} (issuer: {})", ic.value, ic.currency, ic.issuer)
        }
    }
}

pub(crate) fn push_common_lines(
    lines: &mut Vec<Line>,
    account: &str,
    sequence: Option<u32>,
    fee: Option<String>,
) {
    let hi = theme::accent_style();
    let val = Style::new().fg(theme::ACCENT);
    lines.push(Line::from(vec![
        Span::styled("Account", hi),
        Span::raw(": "),
        Span::styled(account.to_string(), val),
    ]));
    if let Some(seq) = sequence {
        lines.push(Line::from(vec![
            Span::styled("Sequence", hi),
            Span::raw(": "),
            Span::styled(seq.to_string(), val),
        ]));
    }
    if let Some(f) = fee {
        lines.push(Line::from(vec![
            Span::styled("Fee", hi),
            Span::raw(": "),
            Span::styled(crate::xrpl::drops_to_xrp(&f), theme::dim_style()),
        ]));
    }
}

/// Format an xrpl Currency for display.
pub(crate) fn fmt_currency(c: &xrpl::models::Currency) -> String {
    match c {
        xrpl::models::Currency::IssuedCurrency(ic) => {
            format!("{} ({})", ic.currency, ic.issuer)
        }
        xrpl::models::Currency::XRP(_) => "XRP".to_string(),
    }
}

pub(crate) fn format_value(key: &str, value: &Value) -> String {
    // Try xrpl-rust Amount for any value-bearing field (XRP drops string or IssuedCurrency object)
    if (key.ends_with("Amount")
        || key == "Fee"
        || key == "SendMax"
        || key == "DeliverMin"
        || key == "Balance"
        || key == "TakerGets"
        || key == "TakerPays")
        && let Ok(amount) = serde_json::from_value::<Amount<'static>>(value.clone())
    {
        match amount {
            Amount::XRPAmount(xrp) => {
                // xrpl-rust XRPAmount deserializes arbitrary objects into a raw string.
                // Only trust the result when the original value was a string/number.
                if !value.is_string() && !value.is_number() {
                    // fall through to generic Value formatting below
                } else if let Ok(drops) = xrp.0.parse::<u64>() {
                    return format!("{:.6} XRP", drops as f64 / 1_000_000.0);
                } else {
                    return xrp.0.to_string();
                }
            }
            Amount::IssuedCurrencyAmount(ica) => {
                return format!("{} {} (issuer: {})", ica.value, ica.currency, ica.issuer);
            }
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
                let val = o.get("value").and_then(Value::as_str).unwrap_or("0");
                return format!("{val} {currency}");
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
    fn fmt_xrpl_amount_xrp() {
        let amount = Amount::XRPAmount(xrpl::models::XRPAmount("1000000".into()));
        // drops_to_xrp does not append " XRP"
        assert_eq!(fmt_xrpl_amount(&amount), "1.000000");
    }

    #[test]
    fn fmt_xrpl_amount_issued() {
        let amount = Amount::IssuedCurrencyAmount(xrpl::models::IssuedCurrencyAmount {
            value: "100".into(),
            currency: "USD".into(),
            issuer: "rsA2LpG".into(),
        });
        assert_eq!(fmt_xrpl_amount(&amount), "100 USD (issuer: rsA2LpG)");
    }

    #[test]
    fn push_common_lines_account_only() {
        let mut lines = Vec::new();
        push_common_lines(&mut lines, "rTest", None, None);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].to_string().contains("rTest"));
    }

    #[test]
    fn push_common_lines_all_fields() {
        let mut lines = Vec::new();
        push_common_lines(&mut lines, "rTest", Some(42), Some("1000".to_string()));
        assert_eq!(lines.len(), 3);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        assert!(text[0].contains("rTest"));
        assert!(text[1].contains("42"));
        assert!(text[2].contains("0.001000"));
    }

    #[test]
    fn fmt_currency_xrp() {
        let c = xrpl::models::Currency::XRP(xrpl::models::XRPAmount("0".into()).into());
        assert_eq!(fmt_currency(&c), "XRP");
    }

    #[test]
    fn fmt_currency_issued() {
        let c = xrpl::models::Currency::IssuedCurrency(
            xrpl::models::IssuedCurrencyAmount {
                value: "0".into(),
                currency: "USD".into(),
                issuer: "rsA2LpG".into(),
            }
            .into(),
        );
        assert_eq!(fmt_currency(&c), "USD (rsA2LpG)");
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
        let result = format_value("Foo", &v);
        assert!(result.ends_with('…'));
        // "…" is 3 bytes in UTF-8, so total byte length is 83
        assert_eq!(result.len(), 83);
    }

    #[test]
    fn format_value_short_object_not_truncated() {
        let v = json!({"a":"hi"});
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
