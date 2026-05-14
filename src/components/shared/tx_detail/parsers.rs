use crate::components::shared::theme;
use crate::components::shared::tx_detail::format::{
    fmt_currency, fmt_xrpl_amount, push_common_lines,
};
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use serde_json::Value;

/// Try to parse a Payment transaction into typed detail lines.
pub(crate) fn payment_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::payment::Payment;
    let payment: Payment<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &payment.common_fields.account,
        payment.common_fields.sequence,
        payment.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("Destination", theme::accent_style()),
        Span::raw(": "),
        Span::styled(payment.destination.to_string(), val),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Amount", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_xrpl_amount(&payment.amount), val),
    ]));

    if let Some(tag) = payment.destination_tag {
        lines.push(Line::from(vec![
            Span::styled("DestinationTag", theme::accent_style()),
            Span::raw(": "),
            Span::styled(tag.to_string(), val),
        ]));
    }

    Some(lines)
}

/// Try to parse an AccountSet transaction into typed detail lines.
pub(crate) fn account_set_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::account_set::AccountSet;
    let tx: AccountSet<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    if let Some(set) = tx.set_flag {
        lines.push(Line::from(vec![
            Span::styled("SetFlag", theme::accent_style()),
            Span::raw(": "),
            Span::styled(set.to_string(), val),
        ]));
    }
    if let Some(clear) = tx.clear_flag {
        lines.push(Line::from(vec![
            Span::styled("ClearFlag", theme::accent_style()),
            Span::raw(": "),
            Span::styled(clear.to_string(), val),
        ]));
    }
    if let Some(domain) = tx.domain {
        lines.push(Line::from(vec![
            Span::styled("Domain", theme::accent_style()),
            Span::raw(": "),
            Span::styled(domain.to_string(), val),
        ]));
    }
    if let Some(tick) = tx.tick_size {
        lines.push(Line::from(vec![
            Span::styled("TickSize", theme::accent_style()),
            Span::raw(": "),
            Span::styled(tick.to_string(), val),
        ]));
    }
    if let Some(rate) = tx.transfer_rate {
        lines.push(Line::from(vec![
            Span::styled("TransferRate", theme::accent_style()),
            Span::raw(": "),
            Span::styled(rate.to_string(), val),
        ]));
    }

    Some(lines)
}

/// Try to parse a TrustSet transaction into typed detail lines.
pub(crate) fn trust_set_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::trust_set::TrustSet;
    let tx: TrustSet<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("LimitAmount", theme::accent_style()),
        Span::raw(": "),
        Span::styled(
            format!(
                "{} {} (issuer: {})",
                tx.limit_amount.value, tx.limit_amount.currency, tx.limit_amount.issuer
            ),
            val,
        ),
    ]));

    if let Some(q_in) = tx.quality_in {
        lines.push(Line::from(vec![
            Span::styled("QualityIn", theme::accent_style()),
            Span::raw(": "),
            Span::styled(q_in.to_string(), val),
        ]));
    }
    if let Some(q_out) = tx.quality_out {
        lines.push(Line::from(vec![
            Span::styled("QualityOut", theme::accent_style()),
            Span::raw(": "),
            Span::styled(q_out.to_string(), val),
        ]));
    }

    Some(lines)
}

/// Try to parse an OfferCreate transaction into typed detail lines.
pub(crate) fn offer_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::offer_create::OfferCreate;
    let tx: OfferCreate<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("TakerGets", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_xrpl_amount(&tx.taker_gets), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("TakerPays", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_xrpl_amount(&tx.taker_pays), val),
    ]));

    if let Some(exp) = tx.expiration {
        let unix = exp as i64 + super::RIPPLE_EPOCH;
        let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
        lines.push(Line::from(vec![
            Span::styled("Expiration", theme::accent_style()),
            Span::raw(": "),
            Span::styled(ts, val),
        ]));
    }
    if let Some(seq) = tx.offer_sequence {
        lines.push(Line::from(vec![
            Span::styled("OfferSequence", theme::accent_style()),
            Span::raw(": "),
            Span::styled(seq.to_string(), val),
        ]));
    }

    Some(lines)
}

/// Try to parse an NFTokenMint transaction into typed detail lines.
pub(crate) fn nftoken_mint_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::nftoken_mint::NFTokenMint;
    let tx: NFTokenMint<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("NFTokenTaxon", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.nftoken_taxon.to_string(), val),
    ]));

    if let Some(issuer) = tx.issuer {
        lines.push(Line::from(vec![
            Span::styled("Issuer", theme::accent_style()),
            Span::raw(": "),
            Span::styled(issuer.to_string(), val),
        ]));
    }
    if let Some(fee) = tx.transfer_fee {
        lines.push(Line::from(vec![
            Span::styled("TransferFee", theme::accent_style()),
            Span::raw(": "),
            Span::styled(format!("{:.3}%", fee as f64 / 1000.0), val),
        ]));
    }
    if let Some(uri) = tx.uri {
        lines.push(Line::from(vec![
            Span::styled("URI", theme::accent_style()),
            Span::raw(": "),
            Span::styled(uri.to_string(), val),
        ]));
    }

    Some(lines)
}

/// Try to parse an OfferCancel transaction into typed detail lines.
pub(crate) fn offer_cancel_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::offer_cancel::OfferCancel;
    let tx: OfferCancel<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("OfferSequence", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.offer_sequence.to_string(), val),
    ]));

    Some(lines)
}

/// Try to parse a CheckCreate transaction into typed detail lines.
pub(crate) fn check_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::check_create::CheckCreate;
    let tx: CheckCreate<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("Destination", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.destination.to_string(), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("SendMax", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_xrpl_amount(&tx.send_max), val),
    ]));

    if let Some(tag) = tx.destination_tag {
        lines.push(Line::from(vec![
            Span::styled("DestinationTag", theme::accent_style()),
            Span::raw(": "),
            Span::styled(tag.to_string(), val),
        ]));
    }
    if let Some(exp) = tx.expiration {
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

/// Try to parse a SignerListSet transaction into typed detail lines.
pub(crate) fn signer_list_set_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::signer_list_set::SignerListSet;
    let tx: SignerListSet<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("SignerQuorum", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.signer_quorum.to_string(), val),
    ]));

    if let Some(entries) = tx.signer_entries {
        for (i, entry) in entries.iter().enumerate() {
            lines.push(Line::from(vec![
                Span::styled(format!("Signer {}", i + 1), theme::accent_style()),
                Span::raw(": "),
                Span::styled(
                    format!("{} (weight: {})", entry.account, entry.signer_weight),
                    val,
                ),
            ]));
        }
    }

    Some(lines)
}

/// Try to parse an EscrowCreate transaction into typed detail lines.
pub(crate) fn escrow_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::escrow_create::EscrowCreate;
    let tx: EscrowCreate<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("Destination", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.destination.to_string(), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Amount", theme::accent_style()),
        Span::raw(": "),
        Span::styled(crate::xrpl::drops_to_xrp(&tx.amount.to_string()), val),
    ]));

    if let Some(tag) = tx.destination_tag {
        lines.push(Line::from(vec![
            Span::styled("DestinationTag", theme::accent_style()),
            Span::raw(": "),
            Span::styled(tag.to_string(), val),
        ]));
    }
    if let Some(finish) = tx.finish_after {
        let unix = finish as i64 + super::RIPPLE_EPOCH;
        let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
        lines.push(Line::from(vec![
            Span::styled("FinishAfter", theme::accent_style()),
            Span::raw(": "),
            Span::styled(ts, val),
        ]));
    }
    if let Some(cancel) = tx.cancel_after {
        let unix = cancel as i64 + super::RIPPLE_EPOCH;
        let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
        lines.push(Line::from(vec![
            Span::styled("CancelAfter", theme::accent_style()),
            Span::raw(": "),
            Span::styled(ts, val),
        ]));
    }
    if let Some(cond) = tx.condition {
        lines.push(Line::from(vec![
            Span::styled("Condition", theme::accent_style()),
            Span::raw(": "),
            Span::styled(cond.to_string(), val),
        ]));
    }

    Some(lines)
}

/// Try to parse an EscrowFinish transaction into typed detail lines.
pub(crate) fn escrow_finish_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::escrow_finish::EscrowFinish;
    let tx: EscrowFinish<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("Owner", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.owner.to_string(), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("OfferSequence", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.offer_sequence.to_string(), val),
    ]));

    if let Some(cond) = tx.condition {
        lines.push(Line::from(vec![
            Span::styled("Condition", theme::accent_style()),
            Span::raw(": "),
            Span::styled(cond.to_string(), val),
        ]));
    }
    if let Some(ful) = tx.fulfillment {
        lines.push(Line::from(vec![
            Span::styled("Fulfillment", theme::accent_style()),
            Span::raw(": "),
            Span::styled(ful.to_string(), val),
        ]));
    }

    Some(lines)
}

/// Try to parse an EscrowCancel transaction into typed detail lines.
pub(crate) fn escrow_cancel_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::escrow_cancel::EscrowCancel;
    let tx: EscrowCancel<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("Owner", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.owner.to_string(), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("OfferSequence", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.offer_sequence.to_string(), val),
    ]));

    Some(lines)
}

/// Try to parse a PaymentChannelCreate transaction into typed detail lines.
pub(crate) fn payment_channel_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::payment_channel_create::PaymentChannelCreate;
    let tx: PaymentChannelCreate<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("Destination", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.destination.to_string(), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Amount", theme::accent_style()),
        Span::raw(": "),
        Span::styled(crate::xrpl::drops_to_xrp(&tx.amount.to_string()), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("SettleDelay", theme::accent_style()),
        Span::raw(": "),
        Span::styled(format!("{}s", tx.settle_delay), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("PublicKey", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.public_key.to_string(), val),
    ]));

    if let Some(tag) = tx.destination_tag {
        lines.push(Line::from(vec![
            Span::styled("DestinationTag", theme::accent_style()),
            Span::raw(": "),
            Span::styled(tag.to_string(), val),
        ]));
    }
    if let Some(cancel) = tx.cancel_after {
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

/// Try to parse a PaymentChannelFund transaction into typed detail lines.
pub(crate) fn payment_channel_fund_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::payment_channel_fund::PaymentChannelFund;
    let tx: PaymentChannelFund<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("Channel", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.channel.to_string(), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Amount", theme::accent_style()),
        Span::raw(": "),
        Span::styled(crate::xrpl::drops_to_xrp(&tx.amount.to_string()), val),
    ]));

    if let Some(exp) = tx.expiration {
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

/// Try to parse a PaymentChannelClaim transaction into typed detail lines.
pub(crate) fn payment_channel_claim_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::payment_channel_claim::PaymentChannelClaim;
    let tx: PaymentChannelClaim<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("Channel", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.channel.to_string(), val),
    ]));

    if let Some(balance) = tx.balance {
        lines.push(Line::from(vec![
            Span::styled("Balance", theme::accent_style()),
            Span::raw(": "),
            Span::styled(crate::xrpl::drops_to_xrp(&balance), val),
        ]));
    }
    if let Some(amount) = tx.amount {
        lines.push(Line::from(vec![
            Span::styled("Amount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(crate::xrpl::drops_to_xrp(&amount), val),
        ]));
    }
    if let Some(sig) = tx.signature {
        lines.push(Line::from(vec![
            Span::styled("Signature", theme::accent_style()),
            Span::raw(": "),
            Span::styled(sig.to_string(), val),
        ]));
    }
    if let Some(pk) = tx.public_key {
        lines.push(Line::from(vec![
            Span::styled("PublicKey", theme::accent_style()),
            Span::raw(": "),
            Span::styled(pk.to_string(), val),
        ]));
    }

    Some(lines)
}

/// Try to parse a CheckCash transaction into typed detail lines.
pub(crate) fn check_cash_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::check_cash::CheckCash;
    let tx: CheckCash<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("CheckID", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.check_id.to_string(), val),
    ]));

    if let Some(amount) = tx.amount {
        lines.push(Line::from(vec![
            Span::styled("Amount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount(&amount), val),
        ]));
    }
    if let Some(min) = tx.deliver_min {
        lines.push(Line::from(vec![
            Span::styled("DeliverMin", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount(&min), val),
        ]));
    }

    Some(lines)
}

/// Try to parse a CheckCancel transaction into typed detail lines.
pub(crate) fn check_cancel_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::check_cancel::CheckCancel;
    let tx: CheckCancel<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("CheckID", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.check_id.to_string(), val),
    ]));

    Some(lines)
}

/// Try to parse a DepositPreauth transaction into typed detail lines.
pub(crate) fn deposit_preauth_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::deposit_preauth::DepositPreauth;
    let tx: DepositPreauth<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    if let Some(auth) = tx.authorize {
        lines.push(Line::from(vec![
            Span::styled("Authorize", theme::accent_style()),
            Span::raw(": "),
            Span::styled(auth.to_string(), val),
        ]));
    }
    if let Some(unauth) = tx.unauthorize {
        lines.push(Line::from(vec![
            Span::styled("Unauthorize", theme::accent_style()),
            Span::raw(": "),
            Span::styled(unauth.to_string(), val),
        ]));
    }

    Some(lines)
}

/// Try to parse a SetRegularKey transaction into typed detail lines.
pub(crate) fn set_regular_key_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::set_regular_key::SetRegularKey;
    let tx: SetRegularKey<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    if let Some(key) = tx.regular_key {
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

/// Try to parse an NFTokenBurn transaction into typed detail lines.
pub(crate) fn nftoken_burn_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::nftoken_burn::NFTokenBurn;
    let tx: NFTokenBurn<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("NFTokenID", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.nftoken_id.to_string(), val),
    ]));

    if let Some(owner) = tx.owner {
        lines.push(Line::from(vec![
            Span::styled("Owner", theme::accent_style()),
            Span::raw(": "),
            Span::styled(owner.to_string(), val),
        ]));
    }

    Some(lines)
}

/// Try to parse an NFTokenCreateOffer transaction into typed detail lines.
pub(crate) fn nftoken_create_offer_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::nftoken_create_offer::NFTokenCreateOffer;
    let tx: NFTokenCreateOffer<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("NFTokenID", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.nftoken_id.to_string(), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Amount", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_xrpl_amount(&tx.amount), val),
    ]));

    if let Some(owner) = tx.owner {
        lines.push(Line::from(vec![
            Span::styled("Owner", theme::accent_style()),
            Span::raw(": "),
            Span::styled(owner.to_string(), val),
        ]));
    }
    if let Some(exp) = tx.expiration {
        let unix = exp as i64 + super::RIPPLE_EPOCH;
        let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
        lines.push(Line::from(vec![
            Span::styled("Expiration", theme::accent_style()),
            Span::raw(": "),
            Span::styled(ts, val),
        ]));
    }
    if let Some(dest) = tx.destination {
        lines.push(Line::from(vec![
            Span::styled("Destination", theme::accent_style()),
            Span::raw(": "),
            Span::styled(dest.to_string(), val),
        ]));
    }

    Some(lines)
}

/// Try to parse an NFTokenAcceptOffer transaction into typed detail lines.
pub(crate) fn nftoken_accept_offer_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::nftoken_accept_offer::NFTokenAcceptOffer;
    let tx: NFTokenAcceptOffer<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    if let Some(sell) = tx.nftoken_sell_offer {
        lines.push(Line::from(vec![
            Span::styled("NFTokenSellOffer", theme::accent_style()),
            Span::raw(": "),
            Span::styled(sell.to_string(), val),
        ]));
    }
    if let Some(buy) = tx.nftoken_buy_offer {
        lines.push(Line::from(vec![
            Span::styled("NFTokenBuyOffer", theme::accent_style()),
            Span::raw(": "),
            Span::styled(buy.to_string(), val),
        ]));
    }
    if let Some(fee) = tx.nftoken_broker_fee {
        lines.push(Line::from(vec![
            Span::styled("NFTokenBrokerFee", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount(&fee), val),
        ]));
    }

    Some(lines)
}

/// Try to parse an NFTokenCancelOffer transaction into typed detail lines.
pub(crate) fn nftoken_cancel_offer_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::nftoken_cancel_offer::NFTokenCancelOffer;
    let tx: NFTokenCancelOffer<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    for (i, offer) in tx.nftoken_offers.iter().enumerate() {
        lines.push(Line::from(vec![
            Span::styled(format!("Offer {}", i + 1), theme::accent_style()),
            Span::raw(": "),
            Span::styled(offer.to_string(), val),
        ]));
    }

    Some(lines)
}

/// Try to parse a TicketCreate transaction into typed detail lines.
pub(crate) fn ticket_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::ticket_create::TicketCreate;
    let tx: TicketCreate<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("TicketCount", theme::accent_style()),
        Span::raw(": "),
        Span::styled(tx.ticket_count.to_string(), val),
    ]));

    Some(lines)
}

/// Try to parse an AMMCreate transaction into typed detail lines.
pub(crate) fn amm_create_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::amm_create::AMMCreate;
    let tx: AMMCreate<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("Amount", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_xrpl_amount(&tx.amount), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Amount2", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_xrpl_amount(&tx.amount2), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("TradingFee", theme::accent_style()),
        Span::raw(": "),
        Span::styled(format!("{:.3}%", tx.trading_fee as f64 / 1000.0), val),
    ]));

    Some(lines)
}

/// Try to parse an AMMDeposit transaction into typed detail lines.
pub(crate) fn amm_deposit_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::amm_deposit::AMMDeposit;
    let tx: AMMDeposit<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("Asset", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_currency(&tx.asset), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Asset2", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_currency(&tx.asset2), val),
    ]));

    if let Some(amount) = tx.amount {
        lines.push(Line::from(vec![
            Span::styled("Amount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount(&amount), val),
        ]));
    }
    if let Some(amount2) = tx.amount2 {
        lines.push(Line::from(vec![
            Span::styled("Amount2", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount(&amount2), val),
        ]));
    }
    if let Some(price) = tx.e_price {
        lines.push(Line::from(vec![
            Span::styled("EPrice", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount(&price), val),
        ]));
    }
    if let Some(lp) = tx.lp_token_out {
        lines.push(Line::from(vec![
            Span::styled("LPTokenOut", theme::accent_style()),
            Span::raw(": "),
            Span::styled(
                format!("{} {} (issuer: {})", lp.value, lp.currency, lp.issuer),
                val,
            ),
        ]));
    }

    Some(lines)
}

/// Try to parse an AMMWithdraw transaction into typed detail lines.
pub(crate) fn amm_withdraw_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::amm_withdraw::AMMWithdraw;
    let tx: AMMWithdraw<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("Asset", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_currency(&tx.asset), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Asset2", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_currency(&tx.asset2), val),
    ]));

    if let Some(amount) = tx.amount {
        lines.push(Line::from(vec![
            Span::styled("Amount", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount(&amount), val),
        ]));
    }
    if let Some(amount2) = tx.amount2 {
        lines.push(Line::from(vec![
            Span::styled("Amount2", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount(&amount2), val),
        ]));
    }
    if let Some(price) = tx.e_price {
        lines.push(Line::from(vec![
            Span::styled("EPrice", theme::accent_style()),
            Span::raw(": "),
            Span::styled(fmt_xrpl_amount(&price), val),
        ]));
    }
    if let Some(lp) = tx.lp_token_in {
        lines.push(Line::from(vec![
            Span::styled("LPTokenIn", theme::accent_style()),
            Span::raw(": "),
            Span::styled(
                format!("{} {} (issuer: {})", lp.value, lp.currency, lp.issuer),
                val,
            ),
        ]));
    }

    Some(lines)
}

/// Try to parse an AMMVote transaction into typed detail lines.
pub(crate) fn amm_vote_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::amm_vote::AMMVote;
    let tx: AMMVote<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("Asset", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_currency(&tx.asset), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Asset2", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_currency(&tx.asset2), val),
    ]));

    if let Some(fee) = tx.trading_fee {
        lines.push(Line::from(vec![
            Span::styled("TradingFee", theme::accent_style()),
            Span::raw(": "),
            Span::styled(format!("{:.3}%", fee as f64 / 1000.0), val),
        ]));
    }

    Some(lines)
}

/// Try to parse an AMMBid transaction into typed detail lines.
pub(crate) fn amm_bid_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::amm_bid::AMMBid;
    let tx: AMMBid<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("Asset", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_currency(&tx.asset), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Asset2", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_currency(&tx.asset2), val),
    ]));

    if let Some(min) = tx.bid_min {
        lines.push(Line::from(vec![
            Span::styled("BidMin", theme::accent_style()),
            Span::raw(": "),
            Span::styled(
                format!("{} {} (issuer: {})", min.value, min.currency, min.issuer),
                val,
            ),
        ]));
    }
    if let Some(max) = tx.bid_max {
        lines.push(Line::from(vec![
            Span::styled("BidMax", theme::accent_style()),
            Span::raw(": "),
            Span::styled(
                format!("{} {} (issuer: {})", max.value, max.currency, max.issuer),
                val,
            ),
        ]));
    }

    Some(lines)
}

/// Try to parse an AMMDelete transaction into typed detail lines.
pub(crate) fn amm_delete_detail_lines<'a>(_tx: &'a Value) -> Option<Vec<Line<'a>>> {
    use xrpl::models::transactions::amm_delete::AMMDelete;
    let tx: AMMDelete<'static> = serde_json::from_value(_tx.clone()).ok()?;
    let mut lines = Vec::new();
    let val = Style::new().fg(theme::ACCENT);

    push_common_lines(
        &mut lines,
        &tx.common_fields.account,
        tx.common_fields.sequence,
        tx.common_fields.fee.as_ref().map(|f| f.to_string()),
    );

    lines.push(Line::from(vec![
        Span::styled("Asset", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_currency(&tx.asset), val),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Asset2", theme::accent_style()),
        Span::raw(": "),
        Span::styled(fmt_currency(&tx.asset2), val),
    ]));

    Some(lines)
}
