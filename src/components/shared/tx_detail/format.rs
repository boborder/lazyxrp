use crate::components::shared::theme;
use crate::xrpl::{JsonAmount, drops_to_xrp, hex_to_ascii, json_amount};
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
            Span::styled(drops_to_xrp(fee), theme::dim_style()),
        ]));
    }
}

/// Format a transaction Amount field directly from serde_json::Value (no clone).
pub(crate) fn fmt_xrpl_amount_from_value(value: &Value) -> String {
    match json_amount(value) {
        Some(JsonAmount::XrpDrops(s)) => drops_to_xrp(s),
        Some(JsonAmount::Issued {
            currency,
            value: amount,
            issuer,
        }) => match issuer {
            Some(issuer) => format!("{amount} {currency} (issuer: {issuer})"),
            None => format!("{amount} {currency}"),
        },
        None => value
            .as_u64()
            .map_or_else(|| value.to_string(), |n| drops_to_xrp(&n.to_string())),
    }
}

fn is_amount_field(key: &str) -> bool {
    matches!(
        key,
        "Fee" | "SendMax" | "DeliverMin" | "Balance" | "TakerGets" | "TakerPays"
    ) || key.ends_with("Amount")
}

pub(crate) fn format_value(key: &str, value: &Value) -> String {
    if is_amount_field(key) {
        if let Some(s) = value.as_str()
            && s.parse::<u64>().is_err()
        {
            return s.to_string();
        }
        if value.as_str().is_some() || value.as_u64().is_some() {
            return format!("{} XRP", fmt_xrpl_amount_from_value(value));
        }
        if value.is_object() {
            return fmt_xrpl_amount_from_value(value);
        }
    }

    match value {
        Value::String(s) => {
            if key == "Domain" {
                return hex_to_ascii(s).unwrap_or_else(|| s.clone());
            }
            s.clone()
        }
        Value::Object(_) => {
            if json_amount(value).is_some() {
                return fmt_xrpl_amount_from_value(value);
            }
            let s = value.to_string();
            if s.chars().count() > 80 {
                let truncated: String = s.chars().take(80).collect();
                format!("{truncated}…")
            } else {
                s
            }
        }
        Value::Array(arr) => format!("[{} items]", arr.len()),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fmt_xrpl_amount_from_value_xrp() {
        assert_eq!(fmt_xrpl_amount_from_value(&json!("1000000")), "1.000000");
        assert_eq!(fmt_xrpl_amount_from_value(&json!(1_000_000)), "1.000000");
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
    fn format_value_maps_json_fields_to_display_strings() {
        let cases = [
            ("Amount", json!("1000000"), "1.000000 XRP"),
            ("Amount", json!(1_000_000), "1.000000 XRP"),
            ("Amount", json!("not_a_number"), "not_a_number"),
            (
                "Amount",
                json!({"value":"100","currency":"USD","issuer":"rsA2LpG"}),
                "100 USD (issuer: rsA2LpG)",
            ),
            (
                "LimitAmount",
                json!({"currency":"EUR","value":"50"}),
                "50 EUR",
            ),
            (
                "LimitAmount",
                json!({"currency":"EUR","value":"50","issuer":"rsA2LpG"}),
                "50 EUR (issuer: rsA2LpG)",
            ),
            ("Domain", json!("6578616d706c652e636f6d"), "example.com"),
            ("Domain", json!("zzzz"), "zzzz"),
            ("Memo", json!("hello"), "hello"),
            ("Memos", json!([1, 2, 3]), "[3 items]"),
            ("Count", json!(42), "42"),
            ("Nothing", json!(null), "null"),
        ];
        for (key, value, expected) in cases {
            assert_eq!(format_value(key, &value), expected, "key={key}");
        }
    }

    #[test]
    fn format_value_long_object_truncated() {
        let v = json!({"a":"x".repeat(100)});
        let full = v.to_string();
        assert!(
            full.chars().count() > 80,
            "fixture must exceed truncate threshold"
        );
        let result = format_value("Foo", &v);
        assert!(result.ends_with('…'));
        let prefix: String = full.chars().take(80).collect();
        assert_eq!(result, format!("{prefix}…"));
    }

    #[test]
    fn format_value_long_object_truncates_on_char_boundary() {
        // Multibyte field values must not panic when the 80-char cut lands mid-codepoint.
        let v = json!({"note":"あ".repeat(90)});
        let result = format_value("Foo", &v);
        assert!(result.ends_with('…'));
        assert_eq!(result.chars().count(), 81, "80 chars + ellipsis");
        assert!(result.contains('あ'));
    }

    #[test]
    fn format_value_short_object_not_truncated() {
        let v = json!({"a":"accent"});
        let result = format_value("Foo", &v);
        assert!(!result.ends_with('…'));
        assert_eq!(result, v.to_string());
    }
}
