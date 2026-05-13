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
        return match amount {
            Amount::XRPAmount(xrp) => {
                if let Ok(drops) = xrp.0.parse::<u64>() {
                    format!("{:.6} XRP", drops as f64 / 1_000_000.0)
                } else {
                    xrp.0.to_string()
                }
            }
            Amount::IssuedCurrencyAmount(ica) => {
                format!("{} {} (issuer: {})", ica.value, ica.currency, ica.issuer)
            }
        };
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
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect();
    String::from_utf8(bytes).ok()
}
