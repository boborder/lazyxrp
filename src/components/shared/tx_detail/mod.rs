use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{
        Block, BorderType, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
};

use serde_json::Value;

use crate::components::shared::{theme, widgets::centered_popup_rect};
use crate::xrpl::ArcValue;

mod format;
mod parsers;

/// Ripple epoch offset: seconds between Unix epoch (1970-01-01) and Ripple epoch (2000-01-01).
const RIPPLE_EPOCH: i64 = 946_684_800;

use format::format_value;
use parsers::typed_detail_lines;

/// Scrollable transaction-detail overlay state.
#[derive(Default)]
pub struct TxDetailState {
    pub visible: bool,
    pub tx_json: ArcValue,
    pub meta_json: ArcValue,
    pub scroll: u16,
    cached_lines: Option<Vec<Line<'static>>>,
}

impl TxDetailState {
    pub fn open(&mut self, tx_json: ArcValue, meta_json: ArcValue) {
        self.visible = true;
        self.tx_json = tx_json;
        self.meta_json = meta_json;
        self.scroll = 0;
        self.cached_lines = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.scroll = 0;
    }
}

/// Render a centered popup with parsed transaction details.
fn to_static_lines(lines: Vec<Line<'_>>) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .map(|line| {
            let spans: Vec<Span<'static>> = line
                .spans
                .into_iter()
                .map(|span| Span::styled(span.content.into_owned(), span.style))
                .collect();
            let mut new_line = Line::from(spans);
            new_line.style = line.style;
            new_line.alignment = line.alignment;
            new_line
        })
        .collect()
}

pub fn render_tx_detail(frame: &mut Frame, area: Rect, state: &mut TxDetailState) {
    if !state.visible {
        return;
    }

    let popup = centered_popup_rect(area, 40, 12);

    frame.render_widget(Clear, popup);

    let tx_type = state
        .tx_json
        .0
        .get("TransactionType")
        .and_then(Value::as_str)
        .unwrap_or("Transaction");
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme::ACCENT))
        .title_style(Style::new().fg(theme::TITLE).bold())
        .title(format!(" {tx_type} Detail "));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let lines = state
        .cached_lines
        .get_or_insert_with(|| {
            to_static_lines(detail_lines_for(&state.tx_json.0, &state.meta_json.0))
        })
        .clone();
    let line_count = lines.len();
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .scroll((state.scroll, 0));

    let [content_area, sb_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);

    frame.render_widget(paragraph, content_area);

    let content_height = content_area.height as usize;
    let max_scroll = line_count.saturating_sub(content_height);
    let mut sb_state = ScrollbarState::new(max_scroll).position(state.scroll as usize);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(theme::dim_style())
            .thumb_style(theme::secondary_style()),
        sb_area,
        &mut sb_state,
    );
}

/// Format an `xrpl::models::Amount` into a human-readable string.
fn detail_lines_for<'a>(tx: &'a Value, meta: &'a Value) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let label_style = theme::dim_style();
    let value_style = theme::accent_style();
    let hi_style = Style::new().fg(theme::TITLE).bold();

    // Header
    if let Some(hash) = tx.get("hash").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Hash", hi_style),
            Span::raw(": "),
            Span::styled(hash, theme::secondary_style()),
        ]));
    }

    let result = meta
        .get("TransactionResult")
        .and_then(Value::as_str)
        .or_else(|| tx.get("TransactionResult").and_then(Value::as_str))
        .unwrap_or("-");
    let result_style = if result == "tesSUCCESS" {
        theme::success_style()
    } else {
        theme::error_style()
    };
    lines.push(Line::from(vec![
        Span::styled("Result", hi_style),
        Span::raw(": "),
        Span::styled(result, result_style),
    ]));

    if let Some(ledger) = tx.get("ledger_index").and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_i64().map(|i| i as u64))
            .or_else(|| v.as_str()?.parse().ok())
    }) {
        lines.push(Line::from(vec![
            Span::styled("Ledger", hi_style),
            Span::raw(": "),
            Span::styled(
                crate::components::shared::fmt::group_digits_u64(ledger),
                value_style,
            ),
        ]));
    }

    if let Some(date) = tx.get("date").and_then(Value::as_u64) {
        let unix = date as i64 + RIPPLE_EPOCH;
        let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
        lines.push(Line::from(vec![
            Span::styled("Date", hi_style),
            Span::raw(": "),
            Span::styled(ts, value_style),
        ]));
    }

    lines.push(Line::from(""));

    // Typed parsers (registration table in parsers.rs)
    let typed_shown = if let Some(typed_lines) = typed_detail_lines(tx) {
        lines.extend(typed_lines);
        true
    } else {
        false
    };

    let known: &[&str] = &[
        "TransactionType",
        "Account",
        "Destination",
        "Amount",
        "Fee",
        "Sequence",
        "SetFlag",
        "ClearFlag",
        "Domain",
        "TickSize",
        "TransferRate",
        "LimitAmount",
        "QualityIn",
        "QualityOut",
        "TakerGets",
        "TakerPays",
        "OfferSequence",
        "Expiration",
        "NFTokenID",
        "URI",
        "TransferFee",
        "NFTokenTaxon",
        "Issuer",
        "RegularKey",
        "SignerQuorum",
        "Channel",
        "PublicKey",
        "SettleDelay",
        "CancelAfter",
        "FinishAfter",
        "Condition",
        "Fulfillment",
        "OwnerCount",
        "Balance",
        "Flags",
        "LastLedgerSequence",
        "SourceTag",
        "DestinationTag",
        "InvoiceID",
        "SendMax",
        "DeliverMin",
        "Memos",
        "Signers",
        "TxnSignature",
        "SigningPubKey",
        "MPTokenIssuanceID",
        "Holder",
        "Asset",
        "Asset2",
        "TradingFee",
        "LPTokenOut",
        "LPTokenIn",
        "EPrice",
        "BidMin",
        "BidMax",
        "Authorize",
        "Unauthorize",
        "DIDDocument",
        "Data",
    ];

    let tx_object = match tx {
        Value::Object(o) => o,
        _ => return lines,
    };

    if !typed_shown {
        for key in known.iter() {
            if let Some(value) = tx_object.get(*key) {
                if value.is_null() {
                    continue;
                }
                let formatted = format_value(key, value);
                lines.push(Line::from(vec![
                    Span::styled(*key, label_style),
                    Span::raw(": "),
                    Span::styled(formatted, value_style),
                ]));
            }
        }
    }

    let mut has_remaining = false;
    for (key, value) in tx_object.iter() {
        if known.contains(&key.as_str())
            || key == "hash"
            || key == "ledger_index"
            || key == "TransactionResult"
            || key == "date"
            || key == "meta"
        {
            continue;
        }
        if value.is_null() {
            continue;
        }
        if !has_remaining {
            lines.push(Line::from(""));
            lines.push(Line::styled("Other fields", hi_style));
            has_remaining = true;
        }
        let formatted = format_value(key, value);
        lines.push(Line::from(vec![
            Span::styled(key.as_str(), label_style),
            Span::raw(": "),
            Span::styled(formatted, theme::dim_style()),
        ]));
    }

    // Metadata (compact)
    let meta_obj = if !meta.is_null() {
        if let Some(obj) = meta.as_object() {
            obj
        } else {
            return lines;
        }
    } else {
        return lines;
    };
    let mut meta_lines = Vec::new();
    for (key, value) in meta_obj.iter() {
        if key == "TransactionResult" || value.is_null() {
            continue;
        }
        if key == "AffectedNodes" {
            let count = value.as_array().map(|a| a.len()).unwrap_or(0);
            meta_lines.push(Line::from(vec![
                Span::styled(key.as_str(), label_style),
                Span::raw(": "),
                Span::styled(format!("[{count} nodes]"), theme::dim_style()),
            ]));
            continue;
        }
        let formatted = format_value(key, value);
        meta_lines.push(Line::from(vec![
            Span::styled(key.as_str(), label_style),
            Span::raw(": "),
            Span::styled(formatted, theme::dim_style()),
        ]));
    }
    if !meta_lines.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::styled("Metadata", hi_style));
        lines.extend(meta_lines);
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    fn arc(v: serde_json::Value) -> ArcValue {
        ArcValue(Arc::new(v))
    }

    #[test]
    fn tx_detail_state_open_close() {
        let mut state = TxDetailState::default();
        assert!(!state.visible);
        state.open(arc(json!({"hash":"abc"})), arc(json!({})));
        assert!(state.visible);
        assert_eq!(state.scroll, 0);
        assert_eq!(state.tx_json.0["hash"], "abc");
        state.close();
        assert!(!state.visible);
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn detail_lines_empty_returns_minimal() {
        let tx = json!({});
        let meta = json!({});
        let lines = detail_lines_for(&tx, &meta);
        // Result line is always present
        assert!(!lines.is_empty());
        let text = lines[0].to_string();
        assert!(text.contains("Result"));
    }

    #[test]
    fn detail_lines_shows_hash_and_result() {
        let tx = json!({"hash":"DEADBEEF","TransactionResult":"tesSUCCESS"});
        let meta = json!({});
        let lines = detail_lines_for(&tx, &meta);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        assert!(
            text.iter()
                .any(|s| s.contains("Hash") && s.contains("DEADBEEF"))
        );
        assert!(
            text.iter()
                .any(|s| s.contains("Result") && s.contains("tesSUCCESS"))
        );
    }

    #[test]
    fn detail_lines_result_from_meta_fallback() {
        let tx = json!({});
        let meta = json!({"TransactionResult":"tecPATH_DRY"});
        let lines = detail_lines_for(&tx, &meta);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        assert!(text.iter().any(|s| s.contains("tecPATH_DRY")));
    }

    #[test]
    fn detail_lines_ledger_index_as_u64() {
        let tx = json!({"ledger_index":12345});
        let meta = json!({});
        let lines = detail_lines_for(&tx, &meta);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        assert!(
            text.iter()
                .any(|s| s.contains("Ledger") && s.contains("12,345"))
        );
    }

    #[test]
    fn detail_lines_ledger_index_as_string() {
        let tx = json!({"ledger_index":"67890"});
        let meta = json!({});
        let lines = detail_lines_for(&tx, &meta);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        assert!(
            text.iter()
                .any(|s| s.contains("Ledger") && s.contains("67,890"))
        );
    }

    #[test]
    fn detail_lines_date_converts_ripple_epoch() {
        let tx = json!({"date":0});
        let meta = json!({});
        let lines = detail_lines_for(&tx, &meta);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        // date:0 → unix RIPPLE_EPOCH, then local formatting — assert exact wiring
        let expected = crate::components::shared::fmt::fmt_local_datetime(RIPPLE_EPOCH);
        assert!(
            text.iter()
                .any(|s| s.contains("Date") && s.contains(&expected)),
            "expected Date line with {expected}, got {text:?}"
        );
    }

    #[test]
    fn detail_lines_payment_typed_parser_success() {
        // Complete Payment JSON triggers typed parser; known fields are handled by it.
        let tx = json!({
            "TransactionType":"Payment",
            "Account":"rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh",
            "Destination":"rf1BiGeXwwQoi8Z2ueFYTEXSwuJYfV2Jpn",
            "Amount":"1000000",
            "Sequence":1,
            "Fee":"12"
        });
        let meta = json!({});
        let lines = detail_lines_for(&tx, &meta);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();

        // Typed parser produces Account, Destination, Amount lines
        assert!(
            text.iter()
                .any(|s| s.contains("Account") && s.contains("rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh"))
        );
        assert!(
            text.iter()
                .any(|s| s.contains("Destination")
                    && s.contains("rf1BiGeXwwQoi8Z2ueFYTEXSwuJYfV2Jpn"))
        );
        assert!(
            text.iter()
                .any(|s| s.contains("Amount") && s.contains("1.000000"))
        );

        // Sequence and Fee also come from typed parser via push_common_lines
        assert!(
            text.iter()
                .any(|s| s.contains("Sequence") && s.contains("1"))
        );
        assert!(
            text.iter()
                .any(|s| s.contains("Fee") && s.contains("0.000012"))
        );

        // Count "Amount:" occurrences — should appear exactly once (typed parser, not fallback)
        let amount_count = text.iter().filter(|s| s.contains("Amount")).count();
        assert_eq!(
            amount_count, 1,
            "Amount should appear exactly once (typed parser), found {}: {:?}",
            amount_count, text
        );
    }

    #[test]
    fn detail_lines_unknown_tx_type_shows_fields_in_other_section() {
        // Unknown TransactionType never matches a typed parser, so all fields fall through to known list + Other fields.
        let tx = json!({"TransactionType":"CustomTx","CustomField":"hello"});
        let meta = json!({});
        let lines = detail_lines_for(&tx, &meta);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();

        // "CustomTx" is in known list? No, so it goes to Other fields
        assert!(
            text.iter().any(|s| s.contains("Other fields")),
            "Should have Other fields section: {:?}",
            text
        );
        assert!(
            text.iter()
                .any(|s| s.contains("CustomField") && s.contains("hello")),
            "CustomField should be in Other fields: {:?}",
            text
        );
    }

    #[test]
    fn detail_lines_meta_affected_nodes_compact() {
        let tx = json!({"hash":"abc"});
        let meta = json!({"AffectedNodes":[1,2,3],"TransactionResult":"tesSUCCESS"});
        let lines = detail_lines_for(&tx, &meta);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        assert!(
            text.iter()
                .any(|s| s.contains("AffectedNodes") && s.contains("[3 nodes]"))
        );
    }

    /// TC-093: opening a new tx clears cached render lines
    #[test]
    fn tx_detail_cache_invalidated_on_open() {
        let mut state = TxDetailState::default();
        let tx1 = json!({"TransactionType":"Payment","Account":"rA","Amount":"1000000"});
        let meta1 = json!({});
        state.open(arc(tx1), arc(meta1));

        // Cache should be None after open (invalidated)
        assert!(state.cached_lines.is_none());

        // Populate cache as draw path would, then reopen to prove invalidation.
        let lines = detail_lines_for(&state.tx_json.0, &state.meta_json.0);
        state.cached_lines = Some(to_static_lines(lines));
        assert!(
            state
                .cached_lines
                .as_ref()
                .is_some_and(|lines| !lines.is_empty())
        );

        let tx2 = json!({"TransactionType":"AccountSet","Account":"rB","SetFlag":8});
        let meta2 = json!({});
        state.open(arc(tx2), arc(meta2));
        assert!(state.cached_lines.is_none());
    }
}
