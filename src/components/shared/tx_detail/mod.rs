use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
};

use serde_json::Value;

use crate::components::shared::theme;
use crate::xrpl::ArcValue;

mod format;
mod parsers;

use format::format_value;
use parsers::*;

/// Scrollable transaction-detail overlay state.
#[derive(Default)]
pub struct TxDetailState {
    pub visible: bool,
    pub tx_json: ArcValue,
    pub meta_json: ArcValue,
    pub scroll: u16,
}

impl TxDetailState {
    pub fn open(&mut self, tx_json: ArcValue, meta_json: ArcValue) {
        self.visible = true;
        self.tx_json = tx_json;
        self.meta_json = meta_json;
        self.scroll = 0;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.scroll = 0;
    }
}

/// Render a centered popup with parsed transaction details.
pub fn render_tx_detail(frame: &mut Frame, area: Rect, state: &TxDetailState) {
    if !state.visible {
        return;
    }

    let popup_w = (area.width * 4 / 5).clamp(40, area.width.saturating_sub(4));
    let popup_h = (area.height * 4 / 5).clamp(12, area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup = Rect::new(x, y, popup_w, popup_h);

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
        .title_style(Style::new().fg(theme::TITLE).add_modifier(Modifier::BOLD))
        .title(format!(" {tx_type} Detail "));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let lines = detail_lines_for(&state.tx_json.0, &state.meta_json.0);
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
    let hi_style = Style::new().fg(theme::TITLE).add_modifier(Modifier::BOLD);

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

    if let Some(ledger) = tx
        .get("ledger_index")
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|i| i as u64)))
    {
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
        let unix = date as i64 + 946_684_800;
        let ts = crate::components::shared::fmt::fmt_local_datetime(unix);
        lines.push(Line::from(vec![
            Span::styled("Date", hi_style),
            Span::raw(": "),
            Span::styled(ts, value_style),
        ]));
    }

    lines.push(Line::from(""));

    // Try typed parse for specific transaction types
    let mut typed_shown = false;
    if let Some(tx_type) = tx.get("TransactionType").and_then(Value::as_str) {
        let maybe_lines = match tx_type {
            "Payment" => payment_detail_lines(tx),
            "AccountSet" => account_set_detail_lines(tx),
            "TrustSet" => trust_set_detail_lines(tx),
            "OfferCreate" => offer_create_detail_lines(tx),
            "NFTokenMint" => nftoken_mint_detail_lines(tx),
            "OfferCancel" => offer_cancel_detail_lines(tx),
            "CheckCreate" => check_create_detail_lines(tx),
            "SignerListSet" => signer_list_set_detail_lines(tx),
            "EscrowCreate" => escrow_create_detail_lines(tx),
            "EscrowFinish" => escrow_finish_detail_lines(tx),
            "EscrowCancel" => escrow_cancel_detail_lines(tx),
            "PaymentChannelCreate" => payment_channel_create_detail_lines(tx),
            "PaymentChannelFund" => payment_channel_fund_detail_lines(tx),
            "PaymentChannelClaim" => payment_channel_claim_detail_lines(tx),
            "CheckCash" => check_cash_detail_lines(tx),
            "CheckCancel" => check_cancel_detail_lines(tx),
            "DepositPreauth" => deposit_preauth_detail_lines(tx),
            "SetRegularKey" => set_regular_key_detail_lines(tx),
            "NFTokenBurn" => nftoken_burn_detail_lines(tx),
            "NFTokenCreateOffer" => nftoken_create_offer_detail_lines(tx),
            "NFTokenAcceptOffer" => nftoken_accept_offer_detail_lines(tx),
            "NFTokenCancelOffer" => nftoken_cancel_offer_detail_lines(tx),
            "AMMCreate" => amm_create_detail_lines(tx),
            "AMMDeposit" => amm_deposit_detail_lines(tx),
            "AMMWithdraw" => amm_withdraw_detail_lines(tx),
            "AMMVote" => amm_vote_detail_lines(tx),
            "AMMBid" => amm_bid_detail_lines(tx),
            "AMMDelete" => amm_delete_detail_lines(tx),
            "TicketCreate" => ticket_create_detail_lines(tx),
            _ => None,
        };
        if let Some(typed_lines) = maybe_lines {
            lines.extend(typed_lines);
            typed_shown = true;
        }
    }

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
        "URI",
    ];

    let obj = match tx {
        Value::Object(o) => o,
        _ => return lines,
    };

    if !typed_shown {
        for key in known.iter() {
            if let Some(value) = obj.get(*key) {
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
    for (key, value) in obj.iter() {
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
    if !meta.is_null() && meta.is_object() {
        let meta_obj = meta.as_object().unwrap();
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
    }

    lines
}
