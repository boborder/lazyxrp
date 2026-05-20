use crate::components::shared::theme;
use crate::components::shared::tx_detail::format::{
    fmt_xrpl_amount_from_value, push_common_lines_from_value,
};
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use serde_json::Value;

/// Payment detail lines parsed directly from Value (no clone).
pub(crate) fn payment_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("Payment") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(dest) = _tx.get("Destination").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Destination", theme::accent_style()),
            Span::raw(": "),
            Span::styled(dest.to_string(), val),
        ]));
    }

    if let Some(amount) = _tx.get("Amount") {
        lines.push(Line::from(vec![
            Span::styled("Amount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(amount), val),
        ]));
    }

    if let Some(tag) = _tx.get("DestinationTag").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("DestinationTag", theme::accent_style()),
            Span::raw(": "),
            Span::styled(tag.to_string(), val),
        ]));
    }

    Some(lines)
}

/// AccountSet detail lines parsed directly from Value.
pub(crate) fn account_set_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("AccountSet") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(set) = _tx.get("SetFlag").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("SetFlag", theme::accent_style()),
            Span::raw(": "),
            Span::styled(set.to_string(), val),
        ]));
    }
    if let Some(clear) = _tx.get("ClearFlag").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("ClearFlag", theme::accent_style()),
            Span::raw(": "),
            Span::styled(clear.to_string(), val),
        ]));
    }
    if let Some(domain) = _tx.get("Domain").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Domain", theme::accent_style()),
            Span::raw(": "),
            Span::styled(domain.to_string(), val),
        ]));
    }
    if let Some(tick) = _tx.get("TickSize").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("TickSize", theme::accent_style()),
            Span::raw(": "),
            Span::styled(tick.to_string(), val),
        ]));
    }
    if let Some(rate) = _tx.get("TransferRate").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("TransferRate", theme::accent_style()),
            Span::raw(": "),
            Span::styled(rate.to_string(), val),
        ]));
    }

    Some(lines)
}

/// TrustSet detail lines parsed directly from Value.
pub(crate) fn trust_set_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("TrustSet") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(limit) = _tx.get("LimitAmount") {
        lines.push(Line::from(vec![
            Span::styled("LimitAmount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(limit), val),
        ]));
    }

    if let Some(q_in) = _tx.get("QualityIn").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("QualityIn", theme::accent_style()),
            Span::raw(": "),
            Span::styled(q_in.to_string(), val),
        ]));
    }
    if let Some(q_out) = _tx.get("QualityOut").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("QualityOut", theme::accent_style()),
            Span::raw(": "),
            Span::styled(q_out.to_string(), val),
        ]));
    }

    Some(lines)
}

/// OfferCreate detail lines parsed directly from Value.
pub(crate) fn offer_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("OfferCreate") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(gets) = _tx.get("TakerGets") {
        lines.push(Line::from(vec![
            Span::styled("TakerGets", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(gets), val),
        ]));
    }
    if let Some(pays) = _tx.get("TakerPays") {
        lines.push(Line::from(vec![
            Span::styled("TakerPays", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(pays), val),
        ]));
    }

    if let Some(exp) = _tx.get("Expiration").and_then(Value::as_u64) {
        let unix = exp as i64 + super::RIPPLE_EPOCH;
        let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
        lines.push(Line::from(vec![
            Span::styled("Expiration", theme::accent_style()),
            Span::raw(": "),
            Span::styled(ts, val),
        ]));
    }
    if let Some(seq) = _tx.get("OfferSequence").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("OfferSequence", theme::accent_style()),
            Span::raw(": "),
            Span::styled(seq.to_string(), val),
        ]));
    }

    Some(lines)
}

/// NFTokenMint detail lines parsed directly from Value.
pub(crate) fn nftoken_mint_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("NFTokenMint") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(taxon) = _tx.get("NFTokenTaxon").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("NFTokenTaxon", theme::accent_style()),
            Span::raw(": "),
            Span::styled(taxon.to_string(), val),
        ]));
    }

    if let Some(issuer) = _tx.get("Issuer").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Issuer", theme::accent_style()),
            Span::raw(": "),
            Span::styled(issuer.to_string(), val),
        ]));
    }
    if let Some(fee) = _tx.get("TransferFee").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("TransferFee", theme::accent_style()),
            Span::raw(": "),
            Span::styled(format!("{:.3}%", fee as f64 / 1000.0), val),
        ]));
    }
    if let Some(uri) = _tx.get("URI").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("URI", theme::accent_style()),
            Span::raw(": "),
            Span::styled(uri.to_string(), val),
        ]));
    }

    Some(lines)
}

/// OfferCancel detail lines parsed directly from Value.
pub(crate) fn offer_cancel_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("OfferCancel") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(seq) = _tx.get("OfferSequence").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("OfferSequence", theme::accent_style()),
            Span::raw(": "),
            Span::styled(seq.to_string(), val),
        ]));
    }

    Some(lines)
}

/// CheckCreate detail lines parsed directly from Value.
pub(crate) fn check_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("CheckCreate") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(dest) = _tx.get("Destination").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Destination", theme::accent_style()),
            Span::raw(": "),
            Span::styled(dest.to_string(), val),
        ]));
    }
    if let Some(send_max) = _tx.get("SendMax") {
        lines.push(Line::from(vec![
            Span::styled("SendMax", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(send_max), val),
        ]));
    }

    if let Some(tag) = _tx.get("DestinationTag").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("DestinationTag", theme::accent_style()),
            Span::raw(": "),
            Span::styled(tag.to_string(), val),
        ]));
    }
    if let Some(exp) = _tx.get("Expiration").and_then(Value::as_u64) {
        let unix = exp as i64 + super::RIPPLE_EPOCH;
        let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
        lines.push(Line::from(vec![
            Span::styled("Expiration", theme::accent_style()),
            Span::raw(": "),
            Span::styled(ts, val),
        ]));
    }

    Some(lines)
}

/// SignerListSet detail lines parsed directly from Value.
pub(crate) fn signer_list_set_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("SignerListSet") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(quorum) = _tx.get("SignerQuorum").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("SignerQuorum", theme::accent_style()),
            Span::raw(": "),
            Span::styled(quorum.to_string(), val),
        ]));
    }

    if let Some(entries) = _tx.get("SignerEntries").and_then(Value::as_array) {
        for (i, entry) in entries.iter().enumerate() {
            let account = entry
                .get("SignerEntry")
                .and_then(|v| v.get("Account"))
                .and_then(Value::as_str)
                .unwrap_or("?");
            let weight = entry
                .get("SignerEntry")
                .and_then(|v| v.get("SignerWeight"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            lines.push(Line::from(vec![
                Span::styled(format!("Signer {}", i + 1), theme::accent_style()),
                Span::raw(": "),
                Span::styled(format!("{} (weight: {})", account, weight), val),
            ]));
        }
    }

    Some(lines)
}

/// EscrowCreate detail lines parsed directly from Value.
pub(crate) fn escrow_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("EscrowCreate") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(dest) = _tx.get("Destination").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Destination", theme::accent_style()),
            Span::raw(": "),
            Span::styled(dest.to_string(), val),
        ]));
    }
    if let Some(amount) = _tx.get("Amount") {
        lines.push(Line::from(vec![
            Span::styled("Amount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(amount), val),
        ]));
    }

    if let Some(tag) = _tx.get("DestinationTag").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("DestinationTag", theme::accent_style()),
            Span::raw(": "),
            Span::styled(tag.to_string(), val),
        ]));
    }
    if let Some(finish) = _tx.get("FinishAfter").and_then(Value::as_u64) {
        let unix = finish as i64 + super::RIPPLE_EPOCH;
        let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
        lines.push(Line::from(vec![
            Span::styled("FinishAfter", theme::accent_style()),
            Span::raw(": "),
            Span::styled(ts, val),
        ]));
    }
    if let Some(cancel) = _tx.get("CancelAfter").and_then(Value::as_u64) {
        let unix = cancel as i64 + super::RIPPLE_EPOCH;
        let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
        lines.push(Line::from(vec![
            Span::styled("CancelAfter", theme::accent_style()),
            Span::raw(": "),
            Span::styled(ts, val),
        ]));
    }
    if let Some(cond) = _tx.get("Condition").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Condition", theme::accent_style()),
            Span::raw(": "),
            Span::styled(cond.to_string(), val),
        ]));
    }

    Some(lines)
}

/// EscrowFinish detail lines parsed directly from Value.
pub(crate) fn escrow_finish_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("EscrowFinish") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(owner) = _tx.get("Owner").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Owner", theme::accent_style()),
            Span::raw(": "),
            Span::styled(owner.to_string(), val),
        ]));
    }
    if let Some(seq) = _tx.get("OfferSequence").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("OfferSequence", theme::accent_style()),
            Span::raw(": "),
            Span::styled(seq.to_string(), val),
        ]));
    }

    if let Some(cond) = _tx.get("Condition").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Condition", theme::accent_style()),
            Span::raw(": "),
            Span::styled(cond.to_string(), val),
        ]));
    }
    if let Some(ful) = _tx.get("Fulfillment").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Fulfillment", theme::accent_style()),
            Span::raw(": "),
            Span::styled(ful.to_string(), val),
        ]));
    }

    Some(lines)
}

/// EscrowCancel detail lines parsed directly from Value.
pub(crate) fn escrow_cancel_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("EscrowCancel") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(owner) = _tx.get("Owner").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Owner", theme::accent_style()),
            Span::raw(": "),
            Span::styled(owner.to_string(), val),
        ]));
    }
    if let Some(seq) = _tx.get("OfferSequence").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("OfferSequence", theme::accent_style()),
            Span::raw(": "),
            Span::styled(seq.to_string(), val),
        ]));
    }

    Some(lines)
}

/// PaymentChannelCreate detail lines parsed directly from Value.
pub(crate) fn payment_channel_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("PaymentChannelCreate") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(dest) = _tx.get("Destination").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Destination", theme::accent_style()),
            Span::raw(": "),
            Span::styled(dest.to_string(), val),
        ]));
    }
    if let Some(amount) = _tx.get("Amount") {
        lines.push(Line::from(vec![
            Span::styled("Amount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(amount), val),
        ]));
    }
    if let Some(delay) = _tx.get("SettleDelay").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("SettleDelay", theme::accent_style()),
            Span::raw(": "),
            Span::styled(format!("{}s", delay), val),
        ]));
    }
    if let Some(pk) = _tx.get("PublicKey").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("PublicKey", theme::accent_style()),
            Span::raw(": "),
            Span::styled(pk.to_string(), val),
        ]));
    }

    if let Some(tag) = _tx.get("DestinationTag").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("DestinationTag", theme::accent_style()),
            Span::raw(": "),
            Span::styled(tag.to_string(), val),
        ]));
    }
    if let Some(cancel) = _tx.get("CancelAfter").and_then(Value::as_u64) {
        let unix = cancel as i64 + super::RIPPLE_EPOCH;
        let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
        lines.push(Line::from(vec![
            Span::styled("CancelAfter", theme::accent_style()),
            Span::raw(": "),
            Span::styled(ts, val),
        ]));
    }

    Some(lines)
}

/// PaymentChannelFund detail lines parsed directly from Value.
pub(crate) fn payment_channel_fund_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("PaymentChannelFund") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(channel) = _tx.get("Channel").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Channel", theme::accent_style()),
            Span::raw(": "),
            Span::styled(channel.to_string(), val),
        ]));
    }
    if let Some(amount) = _tx.get("Amount") {
        lines.push(Line::from(vec![
            Span::styled("Amount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(amount), val),
        ]));
    }

    if let Some(exp) = _tx.get("Expiration").and_then(Value::as_u64) {
        let unix = exp as i64 + super::RIPPLE_EPOCH;
        let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
        lines.push(Line::from(vec![
            Span::styled("Expiration", theme::accent_style()),
            Span::raw(": "),
            Span::styled(ts, val),
        ]));
    }

    Some(lines)
}

/// PaymentChannelClaim detail lines parsed directly from Value.
pub(crate) fn payment_channel_claim_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("PaymentChannelClaim") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(channel) = _tx.get("Channel").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Channel", theme::accent_style()),
            Span::raw(": "),
            Span::styled(channel.to_string(), val),
        ]));
    }

    if let Some(balance) = _tx.get("Balance") {
        lines.push(Line::from(vec![
            Span::styled("Balance", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(balance), val),
        ]));
    }
    if let Some(amount) = _tx.get("Amount") {
        lines.push(Line::from(vec![
            Span::styled("Amount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(amount), val),
        ]));
    }
    if let Some(sig) = _tx.get("Signature").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Signature", theme::accent_style()),
            Span::raw(": "),
            Span::styled(sig.to_string(), val),
        ]));
    }
    if let Some(pk) = _tx.get("PublicKey").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("PublicKey", theme::accent_style()),
            Span::raw(": "),
            Span::styled(pk.to_string(), val),
        ]));
    }

    Some(lines)
}

/// CheckCash detail lines parsed directly from Value.
pub(crate) fn check_cash_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("CheckCash") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(check_id) = _tx.get("CheckID").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("CheckID", theme::accent_style()),
            Span::raw(": "),
            Span::styled(check_id.to_string(), val),
        ]));
    }

    if let Some(amount) = _tx.get("Amount") {
        lines.push(Line::from(vec![
            Span::styled("Amount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(amount), val),
        ]));
    }
    if let Some(min) = _tx.get("DeliverMin") {
        lines.push(Line::from(vec![
            Span::styled("DeliverMin", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(min), val),
        ]));
    }

    Some(lines)
}

/// CheckCancel detail lines parsed directly from Value.
pub(crate) fn check_cancel_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("CheckCancel") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(check_id) = _tx.get("CheckID").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("CheckID", theme::accent_style()),
            Span::raw(": "),
            Span::styled(check_id.to_string(), val),
        ]));
    }

    Some(lines)
}

/// DepositPreauth detail lines parsed directly from Value.
pub(crate) fn deposit_preauth_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("DepositPreauth") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(auth) = _tx.get("Authorize").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Authorize", theme::accent_style()),
            Span::raw(": "),
            Span::styled(auth.to_string(), val),
        ]));
    }
    if let Some(unauth) = _tx.get("Unauthorize").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Unauthorize", theme::accent_style()),
            Span::raw(": "),
            Span::styled(unauth.to_string(), val),
        ]));
    }

    Some(lines)
}

/// SetRegularKey detail lines parsed directly from Value.
pub(crate) fn set_regular_key_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("SetRegularKey") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(key) = _tx.get("RegularKey").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("RegularKey", theme::accent_style()),
            Span::raw(": "),
            Span::styled(key.to_string(), val),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("RegularKey", theme::accent_style()),
            Span::raw(": "),
            Span::styled("(removed)", theme::dim_style()),
        ]));
    }

    Some(lines)
}

/// NFTokenBurn detail lines parsed directly from Value.
pub(crate) fn nftoken_burn_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("NFTokenBurn") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(id) = _tx.get("NFTokenID").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("NFTokenID", theme::accent_style()),
            Span::raw(": "),
            Span::styled(id.to_string(), val),
        ]));
    }

    if let Some(owner) = _tx.get("Owner").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Owner", theme::accent_style()),
            Span::raw(": "),
            Span::styled(owner.to_string(), val),
        ]));
    }

    Some(lines)
}

/// NFTokenCreateOffer detail lines parsed directly from Value.
pub(crate) fn nftoken_create_offer_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("NFTokenCreateOffer") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(id) = _tx.get("NFTokenID").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("NFTokenID", theme::accent_style()),
            Span::raw(": "),
            Span::styled(id.to_string(), val),
        ]));
    }
    if let Some(amount) = _tx.get("Amount") {
        lines.push(Line::from(vec![
            Span::styled("Amount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(amount), val),
        ]));
    }

    if let Some(owner) = _tx.get("Owner").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Owner", theme::accent_style()),
            Span::raw(": "),
            Span::styled(owner.to_string(), val),
        ]));
    }
    if let Some(exp) = _tx.get("Expiration").and_then(Value::as_u64) {
        let unix = exp as i64 + super::RIPPLE_EPOCH;
        let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
        lines.push(Line::from(vec![
            Span::styled("Expiration", theme::accent_style()),
            Span::raw(": "),
            Span::styled(ts, val),
        ]));
    }
    if let Some(dest) = _tx.get("Destination").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("Destination", theme::accent_style()),
            Span::raw(": "),
            Span::styled(dest.to_string(), val),
        ]));
    }

    Some(lines)
}

/// NFTokenAcceptOffer detail lines parsed directly from Value.
pub(crate) fn nftoken_accept_offer_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("NFTokenAcceptOffer") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(sell) = _tx.get("NFTokenSellOffer").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("NFTokenSellOffer", theme::accent_style()),
            Span::raw(": "),
            Span::styled(sell.to_string(), val),
        ]));
    }
    if let Some(buy) = _tx.get("NFTokenBuyOffer").and_then(Value::as_str) {
        lines.push(Line::from(vec![
            Span::styled("NFTokenBuyOffer", theme::accent_style()),
            Span::raw(": "),
            Span::styled(buy.to_string(), val),
        ]));
    }
    if let Some(fee) = _tx.get("NFTokenBrokerFee") {
        lines.push(Line::from(vec![
            Span::styled("NFTokenBrokerFee", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(fee), val),
        ]));
    }

    Some(lines)
}

/// NFTokenCancelOffer detail lines parsed directly from Value.
pub(crate) fn nftoken_cancel_offer_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("NFTokenCancelOffer") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(offers) = _tx.get("NFTokenOffers").and_then(Value::as_array) {
        for (i, offer) in offers.iter().enumerate() {
            let val_str = offer.as_str().unwrap_or("?");
            lines.push(Line::from(vec![
                Span::styled(format!("Offer {}", i + 1), theme::accent_style()),
                Span::raw(": "),
                Span::styled(val_str.to_string(), val),
            ]));
        }
    }

    Some(lines)
}

/// TicketCreate detail lines parsed directly from Value.
pub(crate) fn ticket_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("TicketCreate") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(count) = _tx.get("TicketCount").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("TicketCount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(count.to_string(), val),
        ]));
    }

    Some(lines)
}

/// AMMCreate detail lines parsed directly from Value.
pub(crate) fn amm_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("AMMCreate") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(amount) = _tx.get("Amount") {
        lines.push(Line::from(vec![
            Span::styled("Amount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(amount), val),
        ]));
    }
    if let Some(amount2) = _tx.get("Amount2") {
        lines.push(Line::from(vec![
            Span::styled("Amount2", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(amount2), val),
        ]));
    }
    if let Some(fee) = _tx.get("TradingFee").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("TradingFee", theme::accent_style()),
            Span::raw(": "),
            Span::styled(format!("{:.3}%", fee as f64 / 1000.0), val),
        ]));
    }

    Some(lines)
}

/// AMMDeposit detail lines parsed directly from Value.
pub(crate) fn amm_deposit_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("AMMDeposit") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(asset) = _tx.get("Asset") {
        lines.push(Line::from(vec![
            Span::styled("Asset", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(asset), val),
        ]));
    }
    if let Some(asset2) = _tx.get("Asset2") {
        lines.push(Line::from(vec![
            Span::styled("Asset2", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(asset2), val),
        ]));
    }

    if let Some(amount) = _tx.get("Amount") {
        lines.push(Line::from(vec![
            Span::styled("Amount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(amount), val),
        ]));
    }
    if let Some(amount2) = _tx.get("Amount2") {
        lines.push(Line::from(vec![
            Span::styled("Amount2", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(amount2), val),
        ]));
    }
    if let Some(price) = _tx.get("EPrice") {
        lines.push(Line::from(vec![
            Span::styled("EPrice", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(price), val),
        ]));
    }
    if let Some(lp) = _tx.get("LPTokenOut") {
        lines.push(Line::from(vec![
            Span::styled("LPTokenOut", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(lp), val),
        ]));
    }

    Some(lines)
}

/// AMMWithdraw detail lines parsed directly from Value.
pub(crate) fn amm_withdraw_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("AMMWithdraw") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(asset) = _tx.get("Asset") {
        lines.push(Line::from(vec![
            Span::styled("Asset", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(asset), val),
        ]));
    }
    if let Some(asset2) = _tx.get("Asset2") {
        lines.push(Line::from(vec![
            Span::styled("Asset2", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(asset2), val),
        ]));
    }

    if let Some(amount) = _tx.get("Amount") {
        lines.push(Line::from(vec![
            Span::styled("Amount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(amount), val),
        ]));
    }
    if let Some(amount2) = _tx.get("Amount2") {
        lines.push(Line::from(vec![
            Span::styled("Amount2", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(amount2), val),
        ]));
    }
    if let Some(price) = _tx.get("EPrice") {
        lines.push(Line::from(vec![
            Span::styled("EPrice", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(price), val),
        ]));
    }
    if let Some(lp) = _tx.get("LPTokenIn") {
        lines.push(Line::from(vec![
            Span::styled("LPTokenIn", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(lp), val),
        ]));
    }

    Some(lines)
}

/// AMMVote detail lines parsed directly from Value.
pub(crate) fn amm_vote_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("AMMVote") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(asset) = _tx.get("Asset") {
        lines.push(Line::from(vec![
            Span::styled("Asset", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(asset), val),
        ]));
    }
    if let Some(asset2) = _tx.get("Asset2") {
        lines.push(Line::from(vec![
            Span::styled("Asset2", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(asset2), val),
        ]));
    }

    if let Some(fee) = _tx.get("TradingFee").and_then(Value::as_u64) {
        lines.push(Line::from(vec![
            Span::styled("TradingFee", theme::accent_style()),
            Span::raw(": "),
            Span::styled(format!("{:.3}%", fee as f64 / 1000.0), val),
        ]));
    }

    Some(lines)
}

/// AMMBid detail lines parsed directly from Value.
pub(crate) fn amm_bid_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("AMMBid") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(asset) = _tx.get("Asset") {
        lines.push(Line::from(vec![
            Span::styled("Asset", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(asset), val),
        ]));
    }
    if let Some(asset2) = _tx.get("Asset2") {
        lines.push(Line::from(vec![
            Span::styled("Asset2", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(asset2), val),
        ]));
    }

    if let Some(min) = _tx.get("BidMin") {
        lines.push(Line::from(vec![
            Span::styled("BidMin", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(min), val),
        ]));
    }
    if let Some(max) = _tx.get("BidMax") {
        lines.push(Line::from(vec![
            Span::styled("BidMax", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(max), val),
        ]));
    }

    Some(lines)
}

/// AMMDelete detail lines parsed directly from Value.
pub(crate) fn amm_delete_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    if _tx.get("TransactionType")?.as_str() != Some("AMMDelete") {
        return None;
    }
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines_from_value(&mut lines, _tx);

    if let Some(asset) = _tx.get("Asset") {
        lines.push(Line::from(vec![
            Span::styled("Asset", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(asset), val),
        ]));
    }
    if let Some(asset2) = _tx.get("Asset2") {
        lines.push(Line::from(vec![
            Span::styled("Asset2", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount_from_value(asset2), val),
        ]));
    }

    Some(lines)
}

/// Parser for one `TransactionType` (see [`TX_DETAIL_PARSERS`]).
pub(crate) type TxDetailParserFn = for<'a> fn(&'a Value) -> Option<Vec<Line<'a>>>;

/// Single registration table for typed TX detail parsers (29 types).
/// Add new types here and in `docs/tx-detail.md` only.
pub(crate) const TX_DETAIL_PARSERS: &[(&str, TxDetailParserFn)] = &[
    ("Payment", payment_detail_lines),
    ("AccountSet", account_set_detail_lines),
    ("TrustSet", trust_set_detail_lines),
    ("OfferCreate", offer_create_detail_lines),
    ("NFTokenMint", nftoken_mint_detail_lines),
    ("OfferCancel", offer_cancel_detail_lines),
    ("CheckCreate", check_create_detail_lines),
    ("SignerListSet", signer_list_set_detail_lines),
    ("EscrowCreate", escrow_create_detail_lines),
    ("EscrowFinish", escrow_finish_detail_lines),
    ("EscrowCancel", escrow_cancel_detail_lines),
    ("PaymentChannelCreate", payment_channel_create_detail_lines),
    ("PaymentChannelFund", payment_channel_fund_detail_lines),
    ("PaymentChannelClaim", payment_channel_claim_detail_lines),
    ("CheckCash", check_cash_detail_lines),
    ("CheckCancel", check_cancel_detail_lines),
    ("DepositPreauth", deposit_preauth_detail_lines),
    ("SetRegularKey", set_regular_key_detail_lines),
    ("NFTokenBurn", nftoken_burn_detail_lines),
    ("NFTokenCreateOffer", nftoken_create_offer_detail_lines),
    ("NFTokenAcceptOffer", nftoken_accept_offer_detail_lines),
    ("NFTokenCancelOffer", nftoken_cancel_offer_detail_lines),
    ("AMMCreate", amm_create_detail_lines),
    ("AMMDeposit", amm_deposit_detail_lines),
    ("AMMWithdraw", amm_withdraw_detail_lines),
    ("AMMVote", amm_vote_detail_lines),
    ("AMMBid", amm_bid_detail_lines),
    ("AMMDelete", amm_delete_detail_lines),
    ("TicketCreate", ticket_create_detail_lines),
];

/// Dispatch `TransactionType` to the registered parser, if any.
pub(crate) fn typed_detail_lines<'a>(tx: &'a Value) -> Option<Vec<Line<'a>>> {
    let tx_type = tx.get("TransactionType")?.as_str()?;
    TX_DETAIL_PARSERS
        .iter()
        .find(|(name, _)| *name == tx_type)
        .and_then(|(_, parse)| parse(tx))
}

#[cfg(test)]
mod registry_tests {
    use super::TX_DETAIL_PARSERS;

    #[test]
    fn tx_detail_parser_registry_has_29_types() {
        assert_eq!(TX_DETAIL_PARSERS.len(), 29);
        let mut names: Vec<_> = TX_DETAIL_PARSERS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            29,
            "duplicate TransactionType in TX_DETAIL_PARSERS"
        );
    }
}
