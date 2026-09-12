use std::borrow::Cow;

use crate::components::shared::theme;
use crate::components::shared::tx_detail::format::{
    fmt_xrpl_amount_from_value, push_common_lines_from_value,
};
use ratatui::text::{Line, Span};
use serde_json::Value;

fn detail_line(label: impl Into<Cow<'static, str>>, value: String) -> Line<'static> {
    let accent = theme::accent_style();
    Line::from(vec![
        Span::styled(label, accent),
        Span::raw(": "),
        Span::styled(value, accent),
    ])
}

/// Shared preamble for typed parsers: type guard, common lines, then extras.
fn typed_detail<'a>(
    tx: &'a Value,
    tx_type: &str,
    extra: impl FnOnce(&mut Vec<Line<'static>>),
) -> Option<Vec<Line<'a>>> {
    if tx.get("TransactionType")?.as_str() != Some(tx_type) {
        return None;
    }
    let mut lines: Vec<Line<'static>> = Vec::new();
    push_common_lines_from_value(&mut lines, tx);
    extra(&mut lines);
    Some(lines)
}

/// Payment detail lines parsed directly from Value (no clone).
pub(crate) fn payment_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "Payment", |lines| {
        if let Some(dest) = _tx.get("Destination").and_then(Value::as_str) {
            lines.push(detail_line("Destination", dest.to_string()));
        }

        if let Some(amount) = _tx.get("Amount") {
            lines.push(detail_line("Amount", fmt_xrpl_amount_from_value(amount)));
        }

        if let Some(tag) = _tx.get("DestinationTag").and_then(Value::as_u64) {
            lines.push(detail_line("DestinationTag", tag.to_string()));
        }
    })
}

/// AccountSet detail lines parsed directly from Value.
pub(crate) fn account_set_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "AccountSet", |lines| {
        if let Some(set) = _tx.get("SetFlag").and_then(Value::as_u64) {
            lines.push(detail_line("SetFlag", set.to_string()));
        }
        if let Some(clear) = _tx.get("ClearFlag").and_then(Value::as_u64) {
            lines.push(detail_line("ClearFlag", clear.to_string()));
        }
        if let Some(domain) = _tx.get("Domain").and_then(Value::as_str) {
            lines.push(detail_line("Domain", domain.to_string()));
        }
        if let Some(tick) = _tx.get("TickSize").and_then(Value::as_u64) {
            lines.push(detail_line("TickSize", tick.to_string()));
        }
        if let Some(rate) = _tx.get("TransferRate").and_then(Value::as_u64) {
            lines.push(detail_line("TransferRate", rate.to_string()));
        }
    })
}

/// TrustSet detail lines parsed directly from Value.
pub(crate) fn trust_set_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "TrustSet", |lines| {
        if let Some(limit) = _tx.get("LimitAmount") {
            lines.push(detail_line(
                "LimitAmount",
                fmt_xrpl_amount_from_value(limit),
            ));
        }

        if let Some(q_in) = _tx.get("QualityIn").and_then(Value::as_u64) {
            lines.push(detail_line("QualityIn", q_in.to_string()));
        }
        if let Some(q_out) = _tx.get("QualityOut").and_then(Value::as_u64) {
            lines.push(detail_line("QualityOut", q_out.to_string()));
        }
    })
}

/// OfferCreate detail lines parsed directly from Value.
pub(crate) fn offer_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "OfferCreate", |lines| {
        if let Some(gets) = _tx.get("TakerGets") {
            lines.push(detail_line("TakerGets", fmt_xrpl_amount_from_value(gets)));
        }
        if let Some(pays) = _tx.get("TakerPays") {
            lines.push(detail_line("TakerPays", fmt_xrpl_amount_from_value(pays)));
        }

        if let Some(exp) = _tx.get("Expiration").and_then(Value::as_u64) {
            let unix = exp as i64 + super::RIPPLE_EPOCH;
            let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
            lines.push(detail_line("Expiration", ts));
        }
        if let Some(seq) = _tx.get("OfferSequence").and_then(Value::as_u64) {
            lines.push(detail_line("OfferSequence", seq.to_string()));
        }
    })
}

/// NFTokenMint detail lines parsed directly from Value.
pub(crate) fn nftoken_mint_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "NFTokenMint", |lines| {
        if let Some(taxon) = _tx.get("NFTokenTaxon").and_then(Value::as_u64) {
            lines.push(detail_line("NFTokenTaxon", taxon.to_string()));
        }

        if let Some(issuer) = _tx.get("Issuer").and_then(Value::as_str) {
            lines.push(detail_line("Issuer", issuer.to_string()));
        }
        if let Some(fee) = _tx.get("TransferFee").and_then(Value::as_u64) {
            lines.push(detail_line(
                "TransferFee",
                format!("{:.3}%", fee as f64 / 1000.0),
            ));
        }
        if let Some(uri) = _tx.get("URI").and_then(Value::as_str) {
            lines.push(detail_line("URI", uri.to_string()));
        }
    })
}

/// OfferCancel detail lines parsed directly from Value.
pub(crate) fn offer_cancel_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "OfferCancel", |lines| {
        if let Some(seq) = _tx.get("OfferSequence").and_then(Value::as_u64) {
            lines.push(detail_line("OfferSequence", seq.to_string()));
        }
    })
}

/// CheckCreate detail lines parsed directly from Value.
pub(crate) fn check_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "CheckCreate", |lines| {
        if let Some(dest) = _tx.get("Destination").and_then(Value::as_str) {
            lines.push(detail_line("Destination", dest.to_string()));
        }
        if let Some(send_max) = _tx.get("SendMax") {
            lines.push(detail_line("SendMax", fmt_xrpl_amount_from_value(send_max)));
        }

        if let Some(tag) = _tx.get("DestinationTag").and_then(Value::as_u64) {
            lines.push(detail_line("DestinationTag", tag.to_string()));
        }
        if let Some(exp) = _tx.get("Expiration").and_then(Value::as_u64) {
            let unix = exp as i64 + super::RIPPLE_EPOCH;
            let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
            lines.push(detail_line("Expiration", ts));
        }
    })
}

/// SignerListSet detail lines parsed directly from Value.
pub(crate) fn signer_list_set_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "SignerListSet", |lines| {
        if let Some(quorum) = _tx.get("SignerQuorum").and_then(Value::as_u64) {
            lines.push(detail_line("SignerQuorum", quorum.to_string()));
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
                lines.push(detail_line(
                    format!("Signer {}", i + 1),
                    format!("{} (weight: {})", account, weight),
                ));
            }
        }
    })
}

/// EscrowCreate detail lines parsed directly from Value.
pub(crate) fn escrow_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "EscrowCreate", |lines| {
        if let Some(dest) = _tx.get("Destination").and_then(Value::as_str) {
            lines.push(detail_line("Destination", dest.to_string()));
        }
        if let Some(amount) = _tx.get("Amount") {
            lines.push(detail_line("Amount", fmt_xrpl_amount_from_value(amount)));
        }

        if let Some(tag) = _tx.get("DestinationTag").and_then(Value::as_u64) {
            lines.push(detail_line("DestinationTag", tag.to_string()));
        }
        if let Some(finish) = _tx.get("FinishAfter").and_then(Value::as_u64) {
            let unix = finish as i64 + super::RIPPLE_EPOCH;
            let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
            lines.push(detail_line("FinishAfter", ts));
        }
        if let Some(cancel) = _tx.get("CancelAfter").and_then(Value::as_u64) {
            let unix = cancel as i64 + super::RIPPLE_EPOCH;
            let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
            lines.push(detail_line("CancelAfter", ts));
        }
        if let Some(cond) = _tx.get("Condition").and_then(Value::as_str) {
            lines.push(detail_line("Condition", cond.to_string()));
        }
    })
}

/// EscrowFinish detail lines parsed directly from Value.
pub(crate) fn escrow_finish_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "EscrowFinish", |lines| {
        if let Some(owner) = _tx.get("Owner").and_then(Value::as_str) {
            lines.push(detail_line("Owner", owner.to_string()));
        }
        if let Some(seq) = _tx.get("OfferSequence").and_then(Value::as_u64) {
            lines.push(detail_line("OfferSequence", seq.to_string()));
        }

        if let Some(cond) = _tx.get("Condition").and_then(Value::as_str) {
            lines.push(detail_line("Condition", cond.to_string()));
        }
        if let Some(ful) = _tx.get("Fulfillment").and_then(Value::as_str) {
            lines.push(detail_line("Fulfillment", ful.to_string()));
        }
    })
}

/// EscrowCancel detail lines parsed directly from Value.
pub(crate) fn escrow_cancel_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "EscrowCancel", |lines| {
        if let Some(owner) = _tx.get("Owner").and_then(Value::as_str) {
            lines.push(detail_line("Owner", owner.to_string()));
        }
        if let Some(seq) = _tx.get("OfferSequence").and_then(Value::as_u64) {
            lines.push(detail_line("OfferSequence", seq.to_string()));
        }
    })
}

/// PaymentChannelCreate detail lines parsed directly from Value.
pub(crate) fn payment_channel_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "PaymentChannelCreate", |lines| {
        if let Some(dest) = _tx.get("Destination").and_then(Value::as_str) {
            lines.push(detail_line("Destination", dest.to_string()));
        }
        if let Some(amount) = _tx.get("Amount") {
            lines.push(detail_line("Amount", fmt_xrpl_amount_from_value(amount)));
        }
        if let Some(delay) = _tx.get("SettleDelay").and_then(Value::as_u64) {
            lines.push(detail_line("SettleDelay", format!("{}s", delay)));
        }
        if let Some(pk) = _tx.get("PublicKey").and_then(Value::as_str) {
            lines.push(detail_line("PublicKey", pk.to_string()));
        }

        if let Some(tag) = _tx.get("DestinationTag").and_then(Value::as_u64) {
            lines.push(detail_line("DestinationTag", tag.to_string()));
        }
        if let Some(cancel) = _tx.get("CancelAfter").and_then(Value::as_u64) {
            let unix = cancel as i64 + super::RIPPLE_EPOCH;
            let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
            lines.push(detail_line("CancelAfter", ts));
        }
    })
}

/// PaymentChannelFund detail lines parsed directly from Value.
pub(crate) fn payment_channel_fund_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "PaymentChannelFund", |lines| {
        if let Some(channel) = _tx.get("Channel").and_then(Value::as_str) {
            lines.push(detail_line("Channel", channel.to_string()));
        }
        if let Some(amount) = _tx.get("Amount") {
            lines.push(detail_line("Amount", fmt_xrpl_amount_from_value(amount)));
        }

        if let Some(exp) = _tx.get("Expiration").and_then(Value::as_u64) {
            let unix = exp as i64 + super::RIPPLE_EPOCH;
            let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
            lines.push(detail_line("Expiration", ts));
        }
    })
}

/// PaymentChannelClaim detail lines parsed directly from Value.
pub(crate) fn payment_channel_claim_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "PaymentChannelClaim", |lines| {
        if let Some(channel) = _tx.get("Channel").and_then(Value::as_str) {
            lines.push(detail_line("Channel", channel.to_string()));
        }

        if let Some(balance) = _tx.get("Balance") {
            lines.push(detail_line("Balance", fmt_xrpl_amount_from_value(balance)));
        }
        if let Some(amount) = _tx.get("Amount") {
            lines.push(detail_line("Amount", fmt_xrpl_amount_from_value(amount)));
        }
        if let Some(sig) = _tx.get("Signature").and_then(Value::as_str) {
            lines.push(detail_line("Signature", sig.to_string()));
        }
        if let Some(pk) = _tx.get("PublicKey").and_then(Value::as_str) {
            lines.push(detail_line("PublicKey", pk.to_string()));
        }
    })
}

/// CheckCash detail lines parsed directly from Value.
pub(crate) fn check_cash_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "CheckCash", |lines| {
        if let Some(check_id) = _tx.get("CheckID").and_then(Value::as_str) {
            lines.push(detail_line("CheckID", check_id.to_string()));
        }

        if let Some(amount) = _tx.get("Amount") {
            lines.push(detail_line("Amount", fmt_xrpl_amount_from_value(amount)));
        }
        if let Some(min) = _tx.get("DeliverMin") {
            lines.push(detail_line("DeliverMin", fmt_xrpl_amount_from_value(min)));
        }
    })
}

/// CheckCancel detail lines parsed directly from Value.
pub(crate) fn check_cancel_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "CheckCancel", |lines| {
        if let Some(check_id) = _tx.get("CheckID").and_then(Value::as_str) {
            lines.push(detail_line("CheckID", check_id.to_string()));
        }
    })
}

/// DepositPreauth detail lines parsed directly from Value.
pub(crate) fn deposit_preauth_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "DepositPreauth", |lines| {
        if let Some(auth) = _tx.get("Authorize").and_then(Value::as_str) {
            lines.push(detail_line("Authorize", auth.to_string()));
        }
        if let Some(unauth) = _tx.get("Unauthorize").and_then(Value::as_str) {
            lines.push(detail_line("Unauthorize", unauth.to_string()));
        }
    })
}

/// SetRegularKey detail lines parsed directly from Value.
pub(crate) fn set_regular_key_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "SetRegularKey", |lines| {
        if let Some(key) = _tx.get("RegularKey").and_then(Value::as_str) {
            lines.push(detail_line("RegularKey", key.to_string()));
        } else {
            lines.push(Line::from(vec![
                Span::styled("RegularKey", theme::accent_style()),
                Span::raw(": "),
                Span::styled("(removed)", theme::dim_style()),
            ]));
        }
    })
}

/// NFTokenBurn detail lines parsed directly from Value.
pub(crate) fn nftoken_burn_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "NFTokenBurn", |lines| {
        if let Some(id) = _tx.get("NFTokenID").and_then(Value::as_str) {
            lines.push(detail_line("NFTokenID", id.to_string()));
        }

        if let Some(owner) = _tx.get("Owner").and_then(Value::as_str) {
            lines.push(detail_line("Owner", owner.to_string()));
        }
    })
}

/// NFTokenCreateOffer detail lines parsed directly from Value.
pub(crate) fn nftoken_create_offer_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "NFTokenCreateOffer", |lines| {
        if let Some(id) = _tx.get("NFTokenID").and_then(Value::as_str) {
            lines.push(detail_line("NFTokenID", id.to_string()));
        }
        if let Some(amount) = _tx.get("Amount") {
            lines.push(detail_line("Amount", fmt_xrpl_amount_from_value(amount)));
        }

        if let Some(owner) = _tx.get("Owner").and_then(Value::as_str) {
            lines.push(detail_line("Owner", owner.to_string()));
        }
        if let Some(exp) = _tx.get("Expiration").and_then(Value::as_u64) {
            let unix = exp as i64 + super::RIPPLE_EPOCH;
            let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
            lines.push(detail_line("Expiration", ts));
        }
        if let Some(dest) = _tx.get("Destination").and_then(Value::as_str) {
            lines.push(detail_line("Destination", dest.to_string()));
        }
    })
}

/// NFTokenAcceptOffer detail lines parsed directly from Value.
pub(crate) fn nftoken_accept_offer_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "NFTokenAcceptOffer", |lines| {
        if let Some(sell) = _tx.get("NFTokenSellOffer").and_then(Value::as_str) {
            lines.push(detail_line("NFTokenSellOffer", sell.to_string()));
        }
        if let Some(buy) = _tx.get("NFTokenBuyOffer").and_then(Value::as_str) {
            lines.push(detail_line("NFTokenBuyOffer", buy.to_string()));
        }
        if let Some(fee) = _tx.get("NFTokenBrokerFee") {
            lines.push(detail_line(
                "NFTokenBrokerFee",
                fmt_xrpl_amount_from_value(fee),
            ));
        }
    })
}

/// NFTokenCancelOffer detail lines parsed directly from Value.
pub(crate) fn nftoken_cancel_offer_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "NFTokenCancelOffer", |lines| {
        if let Some(offers) = _tx.get("NFTokenOffers").and_then(Value::as_array) {
            for (i, offer) in offers.iter().enumerate() {
                let val_str = offer.as_str().unwrap_or("?");
                lines.push(detail_line(format!("Offer {}", i + 1), val_str.to_string()));
            }
        }
    })
}

/// TicketCreate detail lines parsed directly from Value.
pub(crate) fn ticket_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    typed_detail(_tx, "TicketCreate", |lines| {
        if let Some(count) = _tx.get("TicketCount").and_then(Value::as_u64) {
            lines.push(detail_line("TicketCount", count.to_string()));
        }
    })
}

/// AMM detail fields in display order; absent fields remain omitted.
fn amm_detail_lines(tx: &Value) -> Option<Vec<Line<'_>>> {
    let tx_type = tx.get("TransactionType")?.as_str()?;
    let amount_fields: &[&str] = match tx_type {
        "AMMCreate" => &["Amount", "Amount2"],
        "AMMDeposit" => &[
            "Asset",
            "Asset2",
            "Amount",
            "Amount2",
            "EPrice",
            "LPTokenOut",
        ],
        "AMMWithdraw" => &[
            "Asset",
            "Asset2",
            "Amount",
            "Amount2",
            "EPrice",
            "LPTokenIn",
        ],
        "AMMVote" | "AMMDelete" => &["Asset", "Asset2"],
        "AMMBid" => &["Asset", "Asset2", "BidMin", "BidMax"],
        _ => return None,
    };
    let mut lines = Vec::new();
    push_common_lines_from_value(&mut lines, tx);
    for &field in amount_fields {
        if let Some(value) = tx.get(field) {
            lines.push(detail_line(field, fmt_xrpl_amount_from_value(value)));
        }
    }
    if matches!(tx_type, "AMMCreate" | "AMMVote")
        && let Some(fee) = tx.get("TradingFee").and_then(Value::as_u64)
    {
        lines.push(detail_line(
            "TradingFee",
            format!("{:.3}%", fee as f64 / 1000.0),
        ));
    }
    Some(lines)
}

/// Parser for one `TransactionType` (see [`TX_DETAIL_PARSERS`]).
pub(crate) type TxDetailParserFn = for<'a> fn(&'a Value) -> Option<Vec<Line<'a>>>;

/// Single registration table for typed TX detail parsers (29 types).
/// Adding a type: write a `*_detail_lines` fn here, add a row to this table.
/// Six AMM types share `amm_detail_lines` (ordered fields, explicit `null`,
/// TradingFee formatting on AMMCreate/AMMVote). Shared label/value row:
/// `detail_line`; shared formatting: `format.rs::push_common_lines_from_value`.
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
    ("AMMCreate", amm_detail_lines),
    ("AMMDeposit", amm_detail_lines),
    ("AMMWithdraw", amm_detail_lines),
    ("AMMVote", amm_detail_lines),
    ("AMMBid", amm_detail_lines),
    ("AMMDelete", amm_detail_lines),
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
    use super::{
        TX_DETAIL_PARSERS, account_set_detail_lines, payment_detail_lines, typed_detail_lines,
    };
    use serde_json::json;

    /// Contract: critical TX types are registered exactly once and dispatch.
    /// TC-094: registry covers required types without duplicates
    #[test]
    fn registry_covers_required_types_without_duplicates() {
        const REQUIRED: &[&str] = &[
            "Payment",
            "AccountSet",
            "OfferCreate",
            "TrustSet",
            "EscrowCreate",
            "NFTokenMint",
            "AMMCreate",
        ];
        let names: Vec<&str> = TX_DETAIL_PARSERS.iter().map(|(n, _)| *n).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        let mut uniq = sorted.clone();
        uniq.dedup();
        assert_eq!(
            sorted.len(),
            uniq.len(),
            "duplicate TransactionType in TX_DETAIL_PARSERS"
        );
        for req in REQUIRED {
            assert!(
                names.contains(req),
                "missing required TransactionType parser: {req}"
            );
            let tx = json!({"TransactionType": req, "Account": "rTest"});
            assert!(
                typed_detail_lines(&tx).is_some(),
                "typed_detail_lines should dispatch for {req}"
            );
        }
    }

    /// TC-094: typed parsers reject a mismatched TransactionType.
    #[test]
    fn typed_detail_rejects_mismatched_type() {
        let tx = json!({"TransactionType": "AccountSet", "Account": "rTest"});
        assert!(payment_detail_lines(&tx).is_none());
        assert!(account_set_detail_lines(&tx).is_some());
    }
}
