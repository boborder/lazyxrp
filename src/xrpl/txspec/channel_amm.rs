//! Payment channel + AMM family: PaymentChannelCreate / Fund / Claim, AMMCreate / Vote / Bid /
//! Delete. Each `*_tx` builds the typed `xrpl::models` struct once; `*_json` and `*_sign` are its
//! simulate / signed forms (same pattern as `nft.rs`).
//!
//! Claims are signed off-line by the channel's source key: `Signature` / `PublicKey` are pasted
//! in, lazyxrp never generates a claim signature.
//!
//! Not supported: `CredentialIDs` on PaymentChannelClaim (deposit-auth credentials); AMMClawback
//! (no typed model, see `coverage.rs`).

use color_eyre::eyre::eyre;
use serde_json::Value;
use xrpl::models::transactions::AuthAccount;
use xrpl::models::transactions::amm_bid::AMMBid;
use xrpl::models::transactions::amm_create::AMMCreate;
use xrpl::models::transactions::amm_delete::AMMDelete;
use xrpl::models::transactions::amm_vote::AMMVote;
use xrpl::models::transactions::payment_channel_claim::{
    PaymentChannelClaim, PaymentChannelClaimFlag,
};
use xrpl::models::transactions::payment_channel_create::PaymentChannelCreate;
use xrpl::models::transactions::payment_channel_fund::PaymentChannelFund;
use xrpl::models::{Amount, IssuedCurrencyAmount, Model, XRPAmount};

use super::{Confirm, FieldKind, FieldSpec, Fields, TxSpec, common_fields, flag_bits, val};
use crate::signing::{amm_currency, parse_amm_asset, parse_offer_amount, sign_blob};

const FAMILY: &str = "Channel / AMM";
const ID_LEN: usize = 64;
/// rippled `AUCTION_SLOT_MAX_AUTH_ACCOUNTS`.
const MAX_AUTH_ACCOUNTS: usize = 4;
/// rippled `AMM_MAX_TRADING_FEE` (1/100,000; 1000 = 1 %).
const MAX_TRADING_FEE: u32 = 1000;
/// Total XRP supply in drops (1e17): upper bound for any drops amount.
const MAX_DROPS: u64 = 100_000_000_000_000_000;

const CLAIM_FLAGS: &[(&str, u32)] = &[("renew", 0x0001_0000), ("close", 0x0002_0000)];
const RENEW: u32 = 0x0001_0000;
const CLOSE: u32 = 0x0002_0000;

pub static SPECS: &[TxSpec] = &[
    CHANNEL_CREATE,
    CHANNEL_FUND,
    CHANNEL_CLAIM,
    AMM_CREATE,
    AMM_VOTE,
    AMM_BID,
    AMM_DELETE,
];

fn eyre_model<E: std::fmt::Debug>(e: E) -> color_eyre::Report {
    eyre!("{e:?}")
}

fn to_json<T: serde::Serialize>(tx: &T, what: &str) -> color_eyre::Result<Value> {
    serde_json::to_value(tx).map_err(|e| eyre!("{what} tx_json: {e}"))
}

fn required<'a>(fields: &'a Fields, name: &str) -> color_eyre::Result<&'a str> {
    val(fields, name).ok_or_else(|| eyre!("{name}: required"))
}

fn upper(fields: &Fields, name: &str) -> Option<String> {
    val(fields, name).map(str::to_ascii_uppercase)
}

fn uint(fields: &Fields, name: &str) -> color_eyre::Result<Option<u32>> {
    val(fields, name)
        .map(|v| v.parse::<u32>().map_err(|e| eyre!("{name}: {e}")))
        .transpose()
}

fn no_check(_: &Fields) -> Result<(), String> {
    Ok(())
}

// ------------------------------------------------------------- field grammars

fn parse_drops(v: &str) -> Result<u64, String> {
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
        return Err("expected XRP drops (a positive integer)".into());
    }
    match v.parse::<u64>() {
        Ok(n) if (1..=MAX_DROPS).contains(&n) => Ok(n),
        _ => Err(format!("drops must be 1..={MAX_DROPS}")),
    }
}

fn drops_ok(v: &str) -> Result<(), String> {
    parse_drops(v).map(|_| ())
}

/// Hex string of even length, `min..=max` characters.
fn hex_between(v: &str, min: usize, max: usize) -> Result<(), String> {
    if !v.len().is_multiple_of(2) || !v.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("expected an even number of hex characters".into());
    }
    if !(min..=max).contains(&v.len()) {
        return Err(format!(
            "{} hex characters, expected {min}..={max}",
            v.len()
        ));
    }
    Ok(())
}

/// 33-byte public key: `ED…` (ed25519) or `02` / `03…` (secp256k1).
fn public_key_ok(v: &str) -> Result<(), String> {
    hex_between(v, 66, 66)?;
    let head = v[..2].to_ascii_uppercase();
    if matches!(head.as_str(), "ED" | "02" | "03") {
        Ok(())
    } else {
        Err("a 33-byte key starts with ED, 02 or 03".into())
    }
}

fn signature_ok(v: &str) -> Result<(), String> {
    hex_between(v, 2, 144)
}

/// `XRP` or `CUR:issuer` (same parser as AMMWithdraw).
fn asset_ok(v: &str) -> Result<(), String> {
    parse_amm_asset(v).map(|_| ()).map_err(|e| e.to_string())
}

/// LP token amount `CUR:issuer:value` (an issued currency; never XRP).
fn lp_amount(v: &str) -> color_eyre::Result<IssuedCurrencyAmount<'static>> {
    match parse_offer_amount(v)? {
        Amount::IssuedCurrencyAmount(a) => Ok(a),
        _ => Err(eyre!("expected LP token CUR:issuer:value, not XRP / MPT")),
    }
}

fn lp_amount_ok(v: &str) -> Result<(), String> {
    lp_amount(v).map(|_| ()).map_err(|e| e.to_string())
}

fn split_accounts(v: &str) -> Vec<&str> {
    v.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .collect()
}

fn auth_accounts_ok(v: &str) -> Result<(), String> {
    let list = split_accounts(v);
    if list.is_empty() || list.len() > MAX_AUTH_ACCOUNTS {
        return Err(format!("expected 1..={MAX_AUTH_ACCOUNTS} addresses"));
    }
    for a in &list {
        FieldKind::Address.validate(a)?;
    }
    let mut seen = list.clone();
    seen.sort_unstable();
    if seen.windows(2).any(|w| w[0] == w[1]) {
        return Err("addresses must be unique".into());
    }
    Ok(())
}

fn asset_pair(
    f: &Fields,
) -> color_eyre::Result<(
    xrpl::models::Currency<'static>,
    xrpl::models::Currency<'static>,
)> {
    Ok((
        amm_currency(parse_amm_asset(required(f, "Asset")?)?),
        amm_currency(parse_amm_asset(required(f, "Asset2")?)?),
    ))
}

/// Asset and Asset2 must name two different pool assets.
fn assets_differ(f: &Fields) -> Result<(), String> {
    match (val(f, "Asset"), val(f, "Asset2")) {
        (Some(a), Some(b)) => {
            let (a, b) = (
                parse_amm_asset(a).map_err(|e| e.to_string())?,
                parse_amm_asset(b).map_err(|e| e.to_string())?,
            );
            if a == b {
                return Err("Asset and Asset2 must be different assets".into());
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

const fn asset_field(name: &'static str, help: &'static str) -> FieldSpec {
    FieldSpec {
        name,
        kind: FieldKind::Custom(asset_ok),
        required: true,
        help,
        default: "",
    }
}

const ASSET: FieldSpec = asset_field("Asset", "pool asset: XRP or CUR:issuer");
const ASSET2: FieldSpec = asset_field("Asset2", "the pool's other asset: XRP or CUR:issuer");

const CHANNEL: FieldSpec = FieldSpec {
    name: "Channel",
    kind: FieldKind::Hex(ID_LEN),
    required: true,
    help: "channel id, 64 hex characters",
    default: "",
};

// ------------------------------------------------------- PaymentChannelCreate

pub const CHANNEL_CREATE: TxSpec = TxSpec {
    tx_type: "PaymentChannelCreate",
    family: FAMILY,
    summary: "lock XRP in a payment channel ⚠ funds locked",
    fields: &[
        FieldSpec {
            name: "Destination",
            kind: FieldKind::Address,
            required: true,
            help: "payee (classic address)",
            default: "",
        },
        FieldSpec {
            name: "Amount",
            kind: FieldKind::Custom(drops_ok),
            required: true,
            help: "XRP drops to lock (1 XRP = 1000000)",
            default: "",
        },
        FieldSpec {
            name: "SettleDelay",
            kind: FieldKind::Uint(u32::MAX),
            required: true,
            help: "seconds the payee keeps to claim after a close request",
            default: "86400",
        },
        FieldSpec {
            name: "PublicKey",
            kind: FieldKind::Custom(public_key_ok),
            required: true,
            help: "channel signing key (66 hex: ED… / 02… / 03…); claims are signed with its secret",
            default: "",
        },
        FieldSpec {
            name: "CancelAfter",
            kind: FieldKind::Time,
            required: false,
            help: "immutable expiry: 7d / 12h / 90s from now, or Ripple-epoch seconds",
            default: "",
        },
        FieldSpec {
            name: "DestinationTag",
            kind: FieldKind::Uint(u32::MAX),
            required: false,
            help: "payee tag",
            default: "",
        },
    ],
    confirm: Some(Confirm {
        phrase: "LOCK",
        hint: "The XRP leaves your balance until the channel is claimed or closed",
    }),
    check: no_check,
    build_json: channel_create_json,
    sign: channel_create_sign,
};

fn channel_create_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<PaymentChannelCreate<'static>> {
    Ok(PaymentChannelCreate {
        common_fields: common_fields!(account, PaymentChannelCreate, sequence, fee_lls),
        amount: XRPAmount::from(required(f, "Amount")?.to_string()),
        destination: required(f, "Destination")?.to_string().into(),
        settle_delay: uint(f, "SettleDelay")?.ok_or_else(|| eyre!("SettleDelay: required"))?,
        public_key: upper(f, "PublicKey")
            .ok_or_else(|| eyre!("PublicKey: required"))?
            .into(),
        cancel_after: uint(f, "CancelAfter")?,
        destination_tag: uint(f, "DestinationTag")?,
    })
}

fn channel_create_json(account: &str, f: &Fields, sequence: u32) -> color_eyre::Result<Value> {
    let tx = channel_create_tx(account, f, sequence, None)?;
    tx.validate().map_err(eyre_model)?;
    to_json(&tx, "PaymentChannelCreate")
}

fn channel_create_sign(
    wallet: &xrpl::wallet::Wallet,
    account: &str,
    f: &Fields,
    sequence: u32,
    fee: u32,
    lls: u32,
) -> color_eyre::Result<String> {
    let mut tx = channel_create_tx(account, f, sequence, Some((fee, lls)))?;
    sign_blob!(tx, wallet)
}

// -------------------------------------------------------- PaymentChannelFund

pub const CHANNEL_FUND: TxSpec = TxSpec {
    tx_type: "PaymentChannelFund",
    family: FAMILY,
    summary: "add XRP to a payment channel ⚠ funds locked",
    fields: &[
        CHANNEL,
        FieldSpec {
            name: "Amount",
            kind: FieldKind::Custom(drops_ok),
            required: true,
            help: "XRP drops to add",
            default: "",
        },
        FieldSpec {
            name: "Expiration",
            kind: FieldKind::Time,
            required: false,
            help: "new channel expiry: 7d / 12h / 90s from now, or Ripple-epoch seconds",
            default: "",
        },
    ],
    confirm: Some(Confirm {
        phrase: "FUND",
        hint: "The XRP leaves your balance until the channel is claimed or closed",
    }),
    check: no_check,
    build_json: channel_fund_json,
    sign: channel_fund_sign,
};

fn channel_fund_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<PaymentChannelFund<'static>> {
    Ok(PaymentChannelFund {
        common_fields: common_fields!(account, PaymentChannelFund, sequence, fee_lls),
        amount: XRPAmount::from(required(f, "Amount")?.to_string()),
        channel: upper(f, "Channel")
            .ok_or_else(|| eyre!("Channel: required"))?
            .into(),
        expiration: uint(f, "Expiration")?,
    })
}

fn channel_fund_json(account: &str, f: &Fields, sequence: u32) -> color_eyre::Result<Value> {
    let tx = channel_fund_tx(account, f, sequence, None)?;
    tx.validate().map_err(eyre_model)?;
    to_json(&tx, "PaymentChannelFund")
}

fn channel_fund_sign(
    wallet: &xrpl::wallet::Wallet,
    account: &str,
    f: &Fields,
    sequence: u32,
    fee: u32,
    lls: u32,
) -> color_eyre::Result<String> {
    let mut tx = channel_fund_tx(account, f, sequence, Some((fee, lls)))?;
    sign_blob!(tx, wallet)
}

// ------------------------------------------------------- PaymentChannelClaim

pub const CHANNEL_CLAIM: TxSpec = TxSpec {
    tx_type: "PaymentChannelClaim",
    family: FAMILY,
    summary: "claim from / close / renew a payment channel ⚠ moves XRP",
    fields: &[
        CHANNEL,
        FieldSpec {
            name: "Balance",
            kind: FieldKind::Custom(drops_ok),
            required: false,
            help: "cumulative XRP drops delivered after this claim; needed unless only closing / renewing",
            default: "",
        },
        FieldSpec {
            name: "Amount",
            kind: FieldKind::Custom(drops_ok),
            required: false,
            help: "drops the pasted Signature authorizes (with Signature)",
            default: "",
        },
        FieldSpec {
            name: "Signature",
            kind: FieldKind::Custom(signature_ok),
            required: false,
            help: "claim signature (hex) from the channel owner; pasted, never generated here",
            default: "",
        },
        FieldSpec {
            name: "PublicKey",
            kind: FieldKind::Custom(public_key_ok),
            required: false,
            help: "channel public key (66 hex); required with Signature",
            default: "",
        },
        FieldSpec {
            name: "Flags",
            kind: FieldKind::Flags(CLAIM_FLAGS),
            required: false,
            help: "renew (source only: clear expiration), close (request close); not both",
            default: "",
        },
    ],
    confirm: Some(Confirm {
        phrase: "CLAIM",
        hint: "Irreversible: XRP is delivered and / or the channel is closed",
    }),
    check: channel_claim_check,
    build_json: channel_claim_json,
    sign: channel_claim_sign,
};

fn channel_claim_bits(f: &Fields) -> u32 {
    val(f, "Flags").map_or(0, |v| flag_bits(CLAIM_FLAGS, v))
}

fn channel_claim_check(f: &Fields) -> Result<(), String> {
    let bits = channel_claim_bits(f);
    if bits & RENEW != 0 && bits & CLOSE != 0 {
        return Err("Flags: renew and close cannot be combined".into());
    }
    if val(f, "Balance").is_none() && bits == 0 {
        return Err("Balance: required unless Flags is close or renew".into());
    }
    if val(f, "Signature").is_some() {
        for name in ["Amount", "Balance", "PublicKey"] {
            if val(f, name).is_none() {
                return Err(format!("{name}: required with Signature"));
            }
        }
    }
    if let (Some(b), Some(a)) = (val(f, "Balance"), val(f, "Amount")) {
        // Both passed `drops_ok`, so the parses cannot fail.
        if parse_drops(b)? > parse_drops(a)? {
            return Err("Balance: must not exceed Amount".into());
        }
    }
    Ok(())
}

fn channel_claim_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<PaymentChannelClaim<'static>> {
    let bits = channel_claim_bits(f);
    let mut flags = Vec::new();
    if bits & RENEW != 0 {
        flags.push(PaymentChannelClaimFlag::TfRenew);
    }
    if bits & CLOSE != 0 {
        flags.push(PaymentChannelClaimFlag::TfClose);
    }
    let text = |name: &str| val(f, name).map(|v| v.to_string().into());
    Ok(PaymentChannelClaim {
        common_fields: common_fields!(account, PaymentChannelClaim, sequence, fee_lls)
            .with_flags(flags),
        channel: upper(f, "Channel")
            .ok_or_else(|| eyre!("Channel: required"))?
            .into(),
        balance: text("Balance"),
        amount: text("Amount"),
        signature: upper(f, "Signature").map(Into::into),
        public_key: upper(f, "PublicKey").map(Into::into),
        credential_ids: None,
    })
}

fn channel_claim_json(account: &str, f: &Fields, sequence: u32) -> color_eyre::Result<Value> {
    let tx = channel_claim_tx(account, f, sequence, None)?;
    tx.validate().map_err(eyre_model)?;
    to_json(&tx, "PaymentChannelClaim")
}

fn channel_claim_sign(
    wallet: &xrpl::wallet::Wallet,
    account: &str,
    f: &Fields,
    sequence: u32,
    fee: u32,
    lls: u32,
) -> color_eyre::Result<String> {
    let mut tx = channel_claim_tx(account, f, sequence, Some((fee, lls)))?;
    sign_blob!(tx, wallet)
}

// ------------------------------------------------------------------ AMMCreate

pub const AMM_CREATE: TxSpec = TxSpec {
    tx_type: "AMMCreate",
    family: FAMILY,
    summary: "create an AMM pool ⚠ funds both assets, burns a special fee",
    fields: &[
        FieldSpec {
            name: "Amount",
            kind: FieldKind::Amount,
            required: true,
            help: "first asset to deposit: XRP:drops or CUR:issuer:value",
            default: "",
        },
        FieldSpec {
            name: "Amount2",
            kind: FieldKind::Amount,
            required: true,
            help: "second asset (different from Amount)",
            default: "",
        },
        FieldSpec {
            name: "TradingFee",
            kind: FieldKind::Uint(MAX_TRADING_FEE),
            required: true,
            help: "0..1000 (1/100,000; 1000 = 1 %)",
            default: "500",
        },
    ],
    confirm: Some(Confirm {
        phrase: "CREATE",
        hint: "Both assets are deposited and the AMM creation fee (≈ one owner reserve) is burned",
    }),
    check: amm_create_check,
    build_json: amm_create_json,
    sign: amm_create_sign,
};

fn amm_create_check(f: &Fields) -> Result<(), String> {
    match (val(f, "Amount"), val(f, "Amount2")) {
        (Some(a), Some(b)) => {
            crate::signing::validate_amm_deposit_specs(a, b).map_err(|e| e.to_string())
        }
        _ => Ok(()),
    }
}

fn amm_create_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<AMMCreate<'static>> {
    Ok(AMMCreate {
        common_fields: common_fields!(account, AMMCreate, sequence, fee_lls),
        amount: parse_offer_amount(required(f, "Amount")?)?,
        amount2: parse_offer_amount(required(f, "Amount2")?)?,
        trading_fee: u16::try_from(
            uint(f, "TradingFee")?.ok_or_else(|| eyre!("TradingFee: required"))?,
        )?,
    })
}

fn amm_create_json(account: &str, f: &Fields, sequence: u32) -> color_eyre::Result<Value> {
    let tx = amm_create_tx(account, f, sequence, None)?;
    tx.validate().map_err(eyre_model)?;
    to_json(&tx, "AMMCreate")
}

fn amm_create_sign(
    wallet: &xrpl::wallet::Wallet,
    account: &str,
    f: &Fields,
    sequence: u32,
    fee: u32,
    lls: u32,
) -> color_eyre::Result<String> {
    let mut tx = amm_create_tx(account, f, sequence, Some((fee, lls)))?;
    sign_blob!(tx, wallet)
}

// -------------------------------------------------------------------- AMMVote

pub const AMM_VOTE: TxSpec = TxSpec {
    tx_type: "AMMVote",
    family: FAMILY,
    summary: "vote on an AMM trading fee (LP holders)",
    fields: &[
        ASSET,
        ASSET2,
        FieldSpec {
            name: "TradingFee",
            kind: FieldKind::Uint(MAX_TRADING_FEE),
            required: true,
            help: "0..1000 (1/100,000; 1000 = 1 %)",
            default: "",
        },
    ],
    confirm: None,
    check: assets_differ,
    build_json: amm_vote_json,
    sign: amm_vote_sign,
};

fn amm_vote_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<AMMVote<'static>> {
    let (asset, asset2) = asset_pair(f)?;
    Ok(AMMVote {
        common_fields: common_fields!(account, AMMVote, sequence, fee_lls),
        asset,
        asset2,
        trading_fee: Some(u16::try_from(
            uint(f, "TradingFee")?.ok_or_else(|| eyre!("TradingFee: required"))?,
        )?),
    })
}

fn amm_vote_json(account: &str, f: &Fields, sequence: u32) -> color_eyre::Result<Value> {
    let tx = amm_vote_tx(account, f, sequence, None)?;
    tx.validate().map_err(eyre_model)?;
    to_json(&tx, "AMMVote")
}

fn amm_vote_sign(
    wallet: &xrpl::wallet::Wallet,
    account: &str,
    f: &Fields,
    sequence: u32,
    fee: u32,
    lls: u32,
) -> color_eyre::Result<String> {
    let mut tx = amm_vote_tx(account, f, sequence, Some((fee, lls)))?;
    sign_blob!(tx, wallet)
}

// --------------------------------------------------------------------- AMMBid

pub const AMM_BID: TxSpec = TxSpec {
    tx_type: "AMMBid",
    family: FAMILY,
    summary: "bid LP tokens for an AMM auction slot ⚠ spends LP tokens",
    fields: &[
        ASSET,
        ASSET2,
        FieldSpec {
            name: "BidMin",
            kind: FieldKind::Custom(lp_amount_ok),
            required: false,
            help: "pay at least this LP token amount: CUR:issuer:value (from amm_info lp_token)",
            default: "",
        },
        FieldSpec {
            name: "BidMax",
            kind: FieldKind::Custom(lp_amount_ok),
            required: false,
            help: "pay at most this LP token amount: CUR:issuer:value",
            default: "",
        },
        FieldSpec {
            name: "AuthAccounts",
            kind: FieldKind::Custom(auth_accounts_ok),
            required: false,
            help: "up to 4 accounts (comma-separated, not you) that get the discounted fee",
            default: "",
        },
    ],
    confirm: Some(Confirm {
        phrase: "BID",
        hint: "The winning bid's LP tokens are spent (part refunded to the previous slot holder)",
    }),
    check: amm_bid_check,
    build_json: amm_bid_json,
    sign: amm_bid_sign,
};

fn amm_bid_check(f: &Fields) -> Result<(), String> {
    assets_differ(f)?;
    if let (Some(min), Some(max)) = (val(f, "BidMin"), val(f, "BidMax")) {
        let (min, max) = (
            lp_amount(min).map_err(|e| e.to_string())?,
            lp_amount(max).map_err(|e| e.to_string())?,
        );
        if (&min.currency, &min.issuer) != (&max.currency, &max.issuer) {
            return Err("BidMin and BidMax must be the same LP token".into());
        }
        // Both passed `validate_iou_fields`, so the values parse.
        if min.value.parse::<f64>().unwrap_or(0.0) > max.value.parse::<f64>().unwrap_or(0.0) {
            return Err("BidMin: must not exceed BidMax".into());
        }
    }
    Ok(())
}

fn amm_bid_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<AMMBid<'static>> {
    let (asset, asset2) = asset_pair(f)?;
    let auth_accounts = val(f, "AuthAccounts")
        .map(|v| {
            let list = split_accounts(v);
            if list.contains(&account) {
                return Err(eyre!("AuthAccounts: must not include the sender"));
            }
            Ok(list
                .into_iter()
                .map(|a| AuthAccount::new(a.to_string()))
                .collect())
        })
        .transpose()?;
    Ok(AMMBid {
        common_fields: common_fields!(account, AMMBid, sequence, fee_lls),
        asset,
        asset2,
        bid_min: val(f, "BidMin").map(lp_amount).transpose()?,
        bid_max: val(f, "BidMax").map(lp_amount).transpose()?,
        auth_accounts,
    })
}

fn amm_bid_json(account: &str, f: &Fields, sequence: u32) -> color_eyre::Result<Value> {
    let tx = amm_bid_tx(account, f, sequence, None)?;
    tx.validate().map_err(eyre_model)?;
    to_json(&tx, "AMMBid")
}

fn amm_bid_sign(
    wallet: &xrpl::wallet::Wallet,
    account: &str,
    f: &Fields,
    sequence: u32,
    fee: u32,
    lls: u32,
) -> color_eyre::Result<String> {
    let mut tx = amm_bid_tx(account, f, sequence, Some((fee, lls)))?;
    sign_blob!(tx, wallet)
}

// ------------------------------------------------------------------ AMMDelete

pub const AMM_DELETE: TxSpec = TxSpec {
    tx_type: "AMMDelete",
    family: FAMILY,
    summary: "delete an empty AMM pool ⚠ irreversible",
    fields: &[ASSET, ASSET2],
    confirm: Some(Confirm {
        phrase: "DELETE",
        hint: "Irreversible: removes the AMM and its empty ledger objects (only works once all LP tokens are withdrawn)",
    }),
    check: assets_differ,
    build_json: amm_delete_json,
    sign: amm_delete_sign,
};

fn amm_delete_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<AMMDelete<'static>> {
    let (asset, asset2) = asset_pair(f)?;
    Ok(AMMDelete {
        common_fields: common_fields!(account, AMMDelete, sequence, fee_lls),
        asset,
        asset2,
    })
}

fn amm_delete_json(account: &str, f: &Fields, sequence: u32) -> color_eyre::Result<Value> {
    let tx = amm_delete_tx(account, f, sequence, None)?;
    tx.validate().map_err(eyre_model)?;
    to_json(&tx, "AMMDelete")
}

fn amm_delete_sign(
    wallet: &xrpl::wallet::Wallet,
    account: &str,
    f: &Fields,
    sequence: u32,
    fee: u32,
    lls: u32,
) -> color_eyre::Result<String> {
    let mut tx = amm_delete_tx(account, f, sequence, Some((fee, lls)))?;
    sign_blob!(tx, wallet)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use xrpl::core::binarycodec::decode;

    use super::*;

    const ACCOUNT_B: &str = "rN7n7otQDd6FczFgLdSqtcsAUxDkw6fzRH";
    const ACCOUNT_C: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
    const ID_A: &str = "5DB01B7FFED6B67E6B0414DED11E051D2EE2B7619CE0EAA6286D67A3A4D5BDB3";
    const PUBKEY: &str = "ED5F5AC8B98974A3CA843326D9B88CEBD0560177B973EE0B149F782911F6B0B0E7";
    const SIG: &str = "7A5B0C1D2E3F405162738495A6B7C8D9E0F1021324354657687980A1B2C3D4E5\
                       F60718293A4B5C6D7E8F90A1B2C3D4E5F60718293A4B5C6D7E8F90A1B2C3D4E5";
    const LP: &str = "03930D02208264E2E40EC1B0C09E4DB96EE197B1";

    fn f(pairs: &[(&str, &str)]) -> Fields {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn wallet() -> xrpl::wallet::Wallet {
        crate::signing::credential_from_secrets(
            Some(&secrecy::SecretString::from(
                "sEdSkooMk31MeTjbHVE7vLvgCpEMAdB",
            )),
            None,
        )
        .expect("credential")
        .expect("seed set")
        .wallet()
        .expect("wallet")
    }

    fn spec(name: &str) -> &'static TxSpec {
        super::super::lookup(name).unwrap_or_else(|| panic!("{name} registered"))
    }

    fn usd() -> String {
        format!("USD:{ACCOUNT_B}")
    }

    fn create_fields() -> Fields {
        f(&[
            ("Destination", ACCOUNT_B),
            ("Amount", "1000000"),
            ("SettleDelay", "86400"),
            ("PublicKey", PUBKEY),
        ])
    }

    /// TC-280: PaymentChannelCreate / Fund validation (drops 1 / 1e17 / 1e17+1 / 0, SettleDelay
    /// u32 max, public key shape, id lengths, Time resolution, required fields, unknown field).
    #[test]
    fn channel_create_fund_validation() {
        let with = |k: &str, v: &str| {
            let mut m = create_fields();
            m.insert(k.into(), v.into());
            m
        };
        let drop_key = |k: &str| {
            let mut m = create_fields();
            m.remove(k);
            m
        };
        let id_63 = &ID_A[..63];
        let cases: Vec<(&str, Fields, Option<&str>)> = vec![
            ("PaymentChannelCreate", create_fields(), None),
            ("PaymentChannelCreate", with("Amount", "1"), None),
            (
                "PaymentChannelCreate",
                with("Amount", "100000000000000000"),
                None,
            ),
            (
                "PaymentChannelCreate",
                with("Amount", "100000000000000001"),
                Some("Amount"),
            ),
            ("PaymentChannelCreate", with("Amount", "0"), Some("Amount")),
            (
                "PaymentChannelCreate",
                with("Amount", "1.5"),
                Some("Amount"),
            ),
            (
                "PaymentChannelCreate",
                with("Amount", "XRP:5"),
                Some("Amount"),
            ),
            ("PaymentChannelCreate", with("SettleDelay", "0"), None),
            (
                "PaymentChannelCreate",
                with("SettleDelay", "4294967295"),
                None,
            ),
            (
                "PaymentChannelCreate",
                with("SettleDelay", "4294967296"),
                Some("SettleDelay"),
            ),
            (
                "PaymentChannelCreate",
                with("Destination", "rBad"),
                Some("Destination"),
            ),
            (
                "PaymentChannelCreate",
                with("PublicKey", &PUBKEY[..64]),
                Some("PublicKey"),
            ),
            (
                "PaymentChannelCreate",
                with("PublicKey", &format!("04{}", &PUBKEY[2..])),
                Some("ED, 02 or 03"),
            ),
            (
                "PaymentChannelCreate",
                with("PublicKey", &format!("02{}", &PUBKEY[2..])),
                None,
            ),
            (
                "PaymentChannelCreate",
                with("PublicKey", &format!("ZZ{}", &PUBKEY[2..])),
                Some("hex"),
            ),
            ("PaymentChannelCreate", with("CancelAfter", "7d"), None),
            (
                "PaymentChannelCreate",
                with("CancelAfter", "4294967296"),
                Some("CancelAfter"),
            ),
            (
                "PaymentChannelCreate",
                with("DestinationTag", "4294967296"),
                Some("DestinationTag"),
            ),
            (
                "PaymentChannelCreate",
                drop_key("Destination"),
                Some("Destination: required"),
            ),
            (
                "PaymentChannelCreate",
                drop_key("PublicKey"),
                Some("PublicKey: required"),
            ),
            (
                "PaymentChannelCreate",
                drop_key("SettleDelay"),
                Some("SettleDelay: required"),
            ),
            (
                "PaymentChannelCreate",
                with("Bogus", "1"),
                Some("unknown field Bogus"),
            ),
            (
                "PaymentChannelFund",
                f(&[("Channel", ID_A), ("Amount", "5")]),
                None,
            ),
            (
                "PaymentChannelFund",
                f(&[("Channel", ID_A), ("Amount", "5"), ("Expiration", "2h")]),
                None,
            ),
            (
                "PaymentChannelFund",
                f(&[("Channel", id_63), ("Amount", "5")]),
                Some("63"),
            ),
            (
                "PaymentChannelFund",
                f(&[("Channel", ID_A), ("Amount", "0")]),
                Some("Amount"),
            ),
            (
                "PaymentChannelFund",
                f(&[("Channel", ID_A)]),
                Some("Amount: required"),
            ),
            (
                "PaymentChannelFund",
                f(&[("Amount", "5")]),
                Some("Channel: required"),
            ),
        ];
        run_cases(&cases);
    }

    fn run_cases(cases: &[(&str, Fields, Option<&str>)]) {
        for (tx_type, fields, want) in cases {
            let got = spec(tx_type).validate(fields);
            match want {
                None => assert!(got.is_ok(), "{tx_type} {fields:?}: {got:?}"),
                Some(needle) => {
                    let e = got.expect_err(&format!("{tx_type} {fields:?} must be rejected"));
                    assert!(
                        e.contains(needle),
                        "{tx_type} {fields:?}: {e} lacks {needle:?}"
                    );
                }
            }
        }
    }

    /// TC-281: PaymentChannelClaim cross-field rules — Signature needs Amount / Balance /
    /// PublicKey, Balance ≤ Amount, renew + close exclusive, something must be claimed or closed,
    /// signature length bounds.
    #[test]
    fn channel_claim_validation() {
        let sig_144 = "A".repeat(144);
        let sig_146 = "A".repeat(146);
        let signed = f(&[
            ("Channel", ID_A),
            ("Balance", "500"),
            ("Amount", "1000"),
            ("Signature", SIG),
            ("PublicKey", PUBKEY),
        ]);
        let without = |k: &str| {
            let mut m = signed.clone();
            m.remove(k);
            m
        };
        let with = |k: &str, v: &str| {
            let mut m = signed.clone();
            m.insert(k.into(), v.into());
            m
        };
        let cases: Vec<(&str, Fields, Option<&str>)> = vec![
            ("PaymentChannelClaim", signed.clone(), None),
            (
                "PaymentChannelClaim",
                f(&[("Channel", ID_A), ("Balance", "1")]),
                None,
            ),
            (
                "PaymentChannelClaim",
                f(&[("Channel", ID_A), ("Flags", "close")]),
                None,
            ),
            (
                "PaymentChannelClaim",
                f(&[("Channel", ID_A), ("Flags", "tfRenew")]),
                None,
            ),
            (
                "PaymentChannelClaim",
                f(&[("Channel", ID_A), ("Flags", "renew,close")]),
                Some("cannot be combined"),
            ),
            (
                "PaymentChannelClaim",
                f(&[("Channel", ID_A)]),
                Some("Balance: required"),
            ),
            (
                "PaymentChannelClaim",
                f(&[("Channel", ID_A), ("Flags", "bogus")]),
                Some("unknown flag"),
            ),
            (
                "PaymentChannelClaim",
                without("Amount"),
                Some("Amount: required with Signature"),
            ),
            (
                "PaymentChannelClaim",
                without("PublicKey"),
                Some("PublicKey: required with Signature"),
            ),
            (
                "PaymentChannelClaim",
                without("Balance"),
                Some("Balance: required"),
            ),
            ("PaymentChannelClaim", with("Balance", "1000"), None),
            (
                "PaymentChannelClaim",
                with("Balance", "1001"),
                Some("must not exceed Amount"),
            ),
            ("PaymentChannelClaim", with("Signature", &sig_144), None),
            (
                "PaymentChannelClaim",
                with("Signature", &sig_146),
                Some("Signature"),
            ),
            (
                "PaymentChannelClaim",
                with("Signature", &SIG[..127]),
                Some("even"),
            ),
            (
                "PaymentChannelClaim",
                with("Signature", "not-hex!"),
                Some("Signature"),
            ),
            ("PaymentChannelClaim", with("Balance", "0"), Some("Balance")),
            (
                "PaymentChannelClaim",
                f(&[("Balance", "1")]),
                Some("Channel: required"),
            ),
        ];
        run_cases(&cases);
    }

    /// TC-282: AMMCreate / Vote / Bid / Delete validation — TradingFee 1000 / 1001, identical
    /// amounts / assets refused, asset grammar, BidMin ≤ BidMax same LP token, XRP bid refused,
    /// AuthAccounts 4 / 5 / duplicates.
    #[test]
    fn amm_validation() {
        let usd = usd();
        let iou = format!("{usd}:10");
        let lp = format!("{LP}:{ACCOUNT_C}");
        let four = [
            ACCOUNT_B,
            ACCOUNT_C,
            "rPT1Sjq2YGrBMTttX4GZHjKu9dyfzbpAYe",
            "rLHzPsX6oXkzU2qL12kHCH8G8cnZv1rBJh",
        ]
        .join(",");
        let five = format!("{four},rBSpBBSz7twmfTZoB7zybTxp1a4rVs9rMx");
        let dup = format!("{ACCOUNT_B},{ACCOUNT_B}");
        let cases: Vec<(&str, Fields, Option<&str>)> = vec![
            (
                "AMMCreate",
                f(&[
                    ("Amount", "1000000"),
                    ("Amount2", &iou),
                    ("TradingFee", "0"),
                ]),
                Some("Amount"),
            ),
            (
                "AMMCreate",
                f(&[
                    ("Amount", "XRP:1000000"),
                    ("Amount2", &iou),
                    ("TradingFee", "0"),
                ]),
                None,
            ),
            (
                "AMMCreate",
                f(&[
                    ("Amount", "XRP:1000000"),
                    ("Amount2", &iou),
                    ("TradingFee", "1000"),
                ]),
                None,
            ),
            (
                "AMMCreate",
                f(&[
                    ("Amount", "XRP:1000000"),
                    ("Amount2", &iou),
                    ("TradingFee", "1001"),
                ]),
                Some("TradingFee"),
            ),
            (
                "AMMCreate",
                f(&[("Amount", "XRP:1000000"), ("Amount2", &iou)]),
                Some("TradingFee: required"),
            ),
            (
                "AMMCreate",
                f(&[
                    ("Amount", "XRP:1"),
                    ("Amount2", "XRP:2"),
                    ("TradingFee", "5"),
                ]),
                Some("different assets"),
            ),
            (
                "AMMCreate",
                f(&[("Amount", "XRP:0"), ("Amount2", &iou), ("TradingFee", "5")]),
                Some("Amount"),
            ),
            (
                "AMMCreate",
                f(&[("Amount", &iou), ("TradingFee", "5")]),
                Some("Amount2: required"),
            ),
            (
                "AMMVote",
                f(&[("Asset", "XRP"), ("Asset2", &usd), ("TradingFee", "1000")]),
                None,
            ),
            (
                "AMMVote",
                f(&[("Asset", "XRP"), ("Asset2", &usd), ("TradingFee", "0")]),
                None,
            ),
            (
                "AMMVote",
                f(&[("Asset", "XRP"), ("Asset2", &usd), ("TradingFee", "1001")]),
                Some("TradingFee"),
            ),
            (
                "AMMVote",
                f(&[("Asset", "XRP"), ("Asset2", "XRP"), ("TradingFee", "1")]),
                Some("different assets"),
            ),
            (
                "AMMVote",
                f(&[("Asset", "XRP"), ("Asset2", "USD"), ("TradingFee", "1")]),
                Some("Asset2"),
            ),
            (
                "AMMVote",
                f(&[
                    ("Asset", "XRP"),
                    ("Asset2", "USD:rBad"),
                    ("TradingFee", "1"),
                ]),
                Some("Asset2"),
            ),
            (
                "AMMVote",
                f(&[("Asset", "XRP"), ("Asset2", &usd)]),
                Some("TradingFee: required"),
            ),
            ("AMMDelete", f(&[("Asset", "XRP"), ("Asset2", &usd)]), None),
            (
                "AMMDelete",
                f(&[("Asset", &usd), ("Asset2", &usd)]),
                Some("different assets"),
            ),
            (
                "AMMDelete",
                f(&[("Asset", "XRP")]),
                Some("Asset2: required"),
            ),
            ("AMMBid", f(&[("Asset", "XRP"), ("Asset2", &usd)]), None),
            (
                "AMMBid",
                f(&[
                    ("Asset", "XRP"),
                    ("Asset2", &usd),
                    ("BidMin", &format!("{lp}:1")),
                    ("BidMax", &format!("{lp}:1")),
                ]),
                None,
            ),
            (
                "AMMBid",
                f(&[
                    ("Asset", "XRP"),
                    ("Asset2", &usd),
                    ("BidMin", &format!("{lp}:2")),
                    ("BidMax", &format!("{lp}:1")),
                ]),
                Some("BidMin: must not exceed BidMax"),
            ),
            (
                "AMMBid",
                f(&[
                    ("Asset", "XRP"),
                    ("Asset2", &usd),
                    ("BidMin", &format!("{lp}:1")),
                    ("BidMax", &format!("{LP}:{ACCOUNT_B}:5")),
                ]),
                Some("same LP token"),
            ),
            (
                "AMMBid",
                f(&[("Asset", "XRP"), ("Asset2", &usd), ("BidMax", "XRP:5")]),
                Some("not XRP"),
            ),
            (
                "AMMBid",
                f(&[
                    ("Asset", "XRP"),
                    ("Asset2", &usd),
                    ("BidMax", &format!("{lp}:0")),
                ]),
                Some("BidMax"),
            ),
            (
                "AMMBid",
                f(&[("Asset", "XRP"), ("Asset2", &usd), ("AuthAccounts", &four)]),
                None,
            ),
            (
                "AMMBid",
                f(&[("Asset", "XRP"), ("Asset2", &usd), ("AuthAccounts", &five)]),
                Some("1..=4"),
            ),
            (
                "AMMBid",
                f(&[("Asset", "XRP"), ("Asset2", &usd), ("AuthAccounts", &dup)]),
                Some("unique"),
            ),
            (
                "AMMBid",
                f(&[("Asset", "XRP"), ("Asset2", &usd), ("AuthAccounts", "rBad")]),
                Some("AuthAccounts"),
            ),
            (
                "AMMBid",
                f(&[("Asset", &usd), ("Asset2", &usd)]),
                Some("different assets"),
            ),
        ];
        run_cases(&cases);
    }

    /// Representative valid forms (as typed), one per type.
    fn samples() -> Vec<(&'static str, Fields)> {
        let lp = format!("{LP}:{ACCOUNT_C}");
        vec![
            ("PaymentChannelCreate", {
                let mut m = create_fields();
                m.insert("CancelAfter".into(), "800000000".into());
                m.insert("DestinationTag".into(), "9".into());
                m.insert("PublicKey".into(), PUBKEY.to_lowercase());
                m
            }),
            (
                "PaymentChannelFund",
                f(&[
                    ("Channel", &ID_A.to_lowercase()),
                    ("Amount", "250"),
                    ("Expiration", "800000001"),
                ]),
            ),
            (
                "PaymentChannelClaim",
                f(&[
                    ("Channel", ID_A),
                    ("Balance", "500"),
                    ("Amount", "1000"),
                    ("Signature", &SIG.to_lowercase()),
                    ("PublicKey", PUBKEY),
                    ("Flags", "close"),
                ]),
            ),
            (
                "AMMCreate",
                f(&[
                    ("Amount", "XRP:1000000"),
                    ("Amount2", &format!("USD:{ACCOUNT_B}:10.5")),
                    ("TradingFee", "500"),
                ]),
            ),
            (
                "AMMVote",
                f(&[("Asset", "XRP"), ("Asset2", &usd()), ("TradingFee", "250")]),
            ),
            (
                "AMMBid",
                f(&[
                    ("Asset", "XRP"),
                    ("Asset2", &usd()),
                    ("BidMin", &format!("{lp}:1.5")),
                    ("BidMax", &format!("{lp}:3")),
                    ("AuthAccounts", &format!("{ACCOUNT_B}, {ACCOUNT_C}")),
                ]),
            ),
            ("AMMDelete", f(&[("Asset", "XRP"), ("Asset2", &usd())])),
        ]
    }

    fn lp_json() -> Value {
        json!({"currency": LP, "issuer": ACCOUNT_C})
    }

    /// TC-283: unsigned `tx_json` per type — Sequence set, no Fee / LastLedgerSequence, drops as
    /// strings, ids / keys upper-cased, claim flag bits, Asset objects, nested AuthAccounts.
    #[test]
    fn unsigned_tx_json_shape() {
        let account = wallet().classic_address.clone();
        for (tx_type, fields) in samples() {
            let s = spec(tx_type);
            let prepared = s.prepare(&fields, 0).expect("valid");
            let j = (s.build_json)(&account, &prepared, 41).expect(tx_type);
            assert_eq!(j["TransactionType"], tx_type);
            assert_eq!(j["Account"], account.as_str());
            assert_eq!(j["Sequence"], 41, "{tx_type}");
            assert!(j.get("Fee").is_none(), "{tx_type}: simulate fills Fee");
            assert!(j.get("LastLedgerSequence").is_none(), "{tx_type}");
            match tx_type {
                "PaymentChannelCreate" => {
                    assert_eq!(j["Amount"], "1000000");
                    assert_eq!(j["Destination"], ACCOUNT_B);
                    assert_eq!(j["SettleDelay"], 86400);
                    assert_eq!(j["PublicKey"], PUBKEY);
                    assert_eq!(j["CancelAfter"], 800_000_000);
                    assert_eq!(j["DestinationTag"], 9);
                }
                "PaymentChannelFund" => {
                    assert_eq!(j["Channel"], ID_A);
                    assert_eq!(j["Amount"], "250");
                    assert_eq!(j["Expiration"], 800_000_001);
                }
                "PaymentChannelClaim" => {
                    assert_eq!(j["Channel"], ID_A);
                    assert_eq!(j["Balance"], "500");
                    assert_eq!(j["Amount"], "1000");
                    assert_eq!(j["Signature"], SIG);
                    assert_eq!(j["PublicKey"], PUBKEY);
                    assert_eq!(j["Flags"], 0x0002_0000);
                }
                "AMMCreate" => {
                    assert_eq!(j["Amount"], "1000000");
                    assert_eq!(
                        j["Amount2"],
                        json!({"currency": "USD", "issuer": ACCOUNT_B, "value": "10.5"})
                    );
                    assert_eq!(j["TradingFee"], 500);
                }
                "AMMVote" => {
                    assert_eq!(j["Asset"], json!({"currency": "XRP"}));
                    assert_eq!(j["Asset2"], json!({"currency": "USD", "issuer": ACCOUNT_B}));
                    assert_eq!(j["TradingFee"], 250);
                }
                "AMMBid" => {
                    assert_eq!(j["Asset"], json!({"currency": "XRP"}));
                    assert_eq!(j["BidMin"]["value"], "1.5");
                    assert_eq!(j["BidMax"]["value"], "3");
                    assert_eq!(j["BidMax"]["currency"], LP);
                    assert_eq!(j["BidMax"]["issuer"], ACCOUNT_C);
                    assert_eq!(
                        j["AuthAccounts"],
                        json!([
                            {"AuthAccount": {"Account": ACCOUNT_B}},
                            {"AuthAccount": {"Account": ACCOUNT_C}},
                        ])
                    );
                }
                "AMMDelete" => {
                    assert_eq!(j["Asset"], json!({"currency": "XRP"}));
                    assert_eq!(j["Asset2"], json!({"currency": "USD", "issuer": ACCOUNT_B}));
                }
                other => panic!("sample for {other} is not asserted"),
            }
        }
        // Claim without flags: no signature fields, Flags 0; renew sets only tfRenew.
        let close_only = spec("PaymentChannelClaim")
            .prepare(&f(&[("Channel", ID_A), ("Flags", "renew")]), 0)
            .unwrap();
        let j = (CHANNEL_CLAIM.build_json)(&account, &close_only, 1).unwrap();
        assert_eq!(j["Flags"], 0x0001_0000);
        assert!(j.get("Balance").is_none() && j.get("Signature").is_none());
        // BidMin alone, no AuthAccounts / BidMax.
        let min_only = spec("AMMBid")
            .prepare(
                &f(&[
                    ("Asset", "XRP"),
                    ("Asset2", &usd()),
                    ("BidMin", &format!("{LP}:{ACCOUNT_C}:2")),
                ]),
                0,
            )
            .unwrap();
        let j = (AMM_BID.build_json)(&account, &min_only, 1).unwrap();
        assert_eq!(j["BidMin"]["issuer"], lp_json()["issuer"]);
        assert!(j.get("BidMax").is_none() && j.get("AuthAccounts").is_none());
    }

    /// TC-284: signed blob per type decodes with distinct Sequence 41 / Fee 13 /
    /// LastLedgerSequence 987654 (swapped args fail) and the typed fields round-trip.
    #[test]
    fn signed_blobs_decode_per_type() {
        let wallet = wallet();
        let account = wallet.classic_address.clone();
        let (sequence, fee, lls) = (41_u32, 13_u32, 987_654_u32);
        for (tx_type, fields) in samples() {
            let s = spec(tx_type);
            let prepared = s.prepare(&fields, 0).expect("valid");
            let blob = (s.sign)(&wallet, &account, &prepared, sequence, fee, lls).expect(tx_type);
            let d = decode(&blob).expect("decode");
            assert_eq!(d["TransactionType"], tx_type);
            assert_eq!(d["Account"], account.as_str());
            assert_eq!(d["Sequence"], sequence, "{tx_type}");
            assert_eq!(d["Fee"], fee.to_string(), "{tx_type}");
            assert_eq!(d["LastLedgerSequence"], lls, "{tx_type}");
            assert!(d["TxnSignature"].as_str().is_some_and(|s| !s.is_empty()));
            assert_eq!(
                d["SigningPubKey"],
                wallet.public_key.to_uppercase().as_str()
            );
            match tx_type {
                "PaymentChannelCreate" => {
                    assert_eq!(d["Amount"], "1000000");
                    assert_eq!(d["Destination"], ACCOUNT_B);
                    assert_eq!(d["SettleDelay"], 86400);
                    assert_eq!(d["PublicKey"], PUBKEY);
                    assert_eq!(d["CancelAfter"], 800_000_000);
                    assert_eq!(d["DestinationTag"], 9);
                }
                "PaymentChannelFund" => {
                    assert_eq!(d["Channel"], ID_A);
                    assert_eq!(d["Amount"], "250");
                    assert_eq!(d["Expiration"], 800_000_001);
                }
                "PaymentChannelClaim" => {
                    assert_eq!(d["Channel"], ID_A);
                    assert_eq!(d["Balance"], "500");
                    assert_eq!(d["Amount"], "1000");
                    assert_eq!(d["Signature"], SIG);
                    assert_eq!(d["PublicKey"], PUBKEY);
                    assert_eq!(d["Flags"], 0x0002_0000);
                }
                "AMMCreate" => {
                    assert_eq!(d["Amount"], "1000000");
                    assert_eq!(d["Amount2"]["currency"], "USD");
                    assert_eq!(d["TradingFee"], 500);
                }
                "AMMVote" => {
                    assert_eq!(d["Asset"]["currency"], "XRP");
                    assert_eq!(d["Asset2"]["issuer"], ACCOUNT_B);
                    assert_eq!(d["TradingFee"], 250);
                }
                "AMMBid" => {
                    assert_eq!(d["BidMin"]["currency"], LP);
                    assert_eq!(d["BidMax"]["issuer"], ACCOUNT_C);
                    assert_eq!(d["AuthAccounts"].as_array().map(Vec::len), Some(2));
                }
                "AMMDelete" => {
                    assert_eq!(d["Asset"]["currency"], "XRP");
                    assert_eq!(d["Asset2"]["issuer"], ACCOUNT_B);
                }
                other => panic!("sample for {other} is not asserted"),
            }
        }
    }

    /// Builder-time rules: the sender may not be its own AuthAccount; the crate model's
    /// own checks run before any RPC.
    #[test]
    fn builder_rules() {
        let account = wallet().classic_address.clone();
        let own = spec("AMMBid")
            .prepare(
                &f(&[
                    ("Asset", "XRP"),
                    ("Asset2", &usd()),
                    ("AuthAccounts", &account),
                ]),
                0,
            )
            .unwrap();
        let e = (AMM_BID.build_json)(&account, &own, 1).expect_err("sender as AuthAccount");
        assert!(e.to_string().contains("sender"), "{e}");
    }

    /// TC-285: every spec in the family is irreversible / fund-moving except AMMVote, and those
    /// carry a typed confirm phrase; the family is registered under one name.
    #[test]
    fn confirm_phrases_and_registry() {
        for s in SPECS {
            assert_eq!(s.family, FAMILY);
            assert!(super::super::lookup(s.tx_type).is_some(), "{}", s.tx_type);
            match (s.tx_type, s.confirm) {
                ("AMMVote", None) => {}
                (t, Some(c)) => assert!(!c.phrase.is_empty() && !c.hint.is_empty(), "{t}"),
                (t, None) => panic!("{t} must carry a Confirm phrase"),
            }
        }
        let phrases: Vec<&str> = SPECS
            .iter()
            .filter_map(|s| s.confirm.map(|c| c.phrase))
            .collect();
        assert_eq!(
            phrases,
            ["LOCK", "FUND", "CLAIM", "CREATE", "BID", "DELETE"]
        );
    }

    /// TC-286: generic submit through a local RPC mock — `server_info → account_info → simulate
    /// → submit` for a PaymentChannelCreate (relative `CancelAfter` fixed once, so simulate and
    /// the signed blob agree) and an AMMDelete; invalid forms never reach the RPC.
    #[tokio::test]
    async fn generic_submit_through_mock() {
        use tokio::net::TcpListener;
        use tokio::sync::mpsc;

        use crate::action::Action;
        use crate::network::Network;
        use crate::test_support::rpc_mock;
        use crate::xrpl::client::RpcClient;
        use crate::xrpl::submit::submit_generic_transaction;
        use crate::xrpl::types::GenericTxSubmitParams;

        fn result_for(method: &str) -> Value {
            match method {
                "server_info" => json!({"info": {"network_id": 1, "validated_ledger": {"seq": 1}}}),
                "account_info" => json!({"account_data": {
                    "Account": ACCOUNT_C, "Balance": "10000000",
                    "Sequence": 7, "OwnerCount": 0, "Flags": 0
                }}),
                "simulate" => json!({
                    "engine_result": "tesSUCCESS",
                    "ledger_index": 80_000_000,
                    "tx_json": {"Sequence": 7, "Fee": "15"}
                }),
                "submit" => json!({"engine_result": "tesSUCCESS", "tx_json": {"hash": "ABC"}}),
                other => panic!("unexpected local RPC method: {other}"),
            }
        }
        let params = |tx_type: &str, fields: Fields| GenericTxSubmitParams {
            tx_type: tx_type.into(),
            fields,
            skip_mainnet_prompt: false,
        };
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(rpc_mock::serve(listener, 8, result_for));
        let rpc = RpcClient::connect(&url).expect("rpc client");
        let credential = crate::signing::credential_from_secrets(
            Some(&secrecy::SecretString::from(
                "sEdSkooMk31MeTjbHVE7vLvgCpEMAdB",
            )),
            None,
        )
        .expect("credential")
        .expect("seed set");
        let (tx, mut rx) = mpsc::unbounded_channel();

        // Refusal first: nothing reaches the RPC (the mock expects exactly 8 requests).
        let mut bad = create_fields();
        bad.insert("Amount".into(), "0".into());
        submit_generic_transaction(
            &rpc,
            &Network::Testnet,
            params("PaymentChannelCreate", bad),
            &tx,
            Some(&credential),
        )
        .await;
        assert!(matches!(
            rx.try_recv(),
            Ok(Action::GenericTxSubmitErr(m)) if m.contains("Amount")
        ));

        let mut create = create_fields();
        create.insert("CancelAfter".into(), "1h".into());
        for (tx_type, fields) in [
            ("PaymentChannelCreate", create),
            ("AMMDelete", f(&[("Asset", "XRP"), ("Asset2", &usd())])),
        ] {
            submit_generic_transaction(
                &rpc,
                &Network::Testnet,
                params(tx_type, fields),
                &tx,
                Some(&credential),
            )
            .await;
            let got: Vec<Action> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
            let Some(Action::GenericTxSubmitOk(shown)) = got.first() else {
                panic!("{tx_type}: expected GenericTxSubmitOk, got {got:?}");
            };
            assert!(shown.starts_with(&format!("{tx_type} · ")), "{shown}");
            assert!(shown.ends_with("fee 15 drops"), "{shown}");
        }

        let requests = server.await.expect("join").expect("serve");
        assert_eq!(
            rpc_mock::methods(&requests),
            [
                "server_info",
                "account_info",
                "simulate",
                "submit",
                "server_info",
                "account_info",
                "simulate",
                "submit"
            ]
        );
        let sim = &requests[2]["params"][0]["tx_json"];
        assert_eq!(sim["TransactionType"], "PaymentChannelCreate");
        assert_eq!(sim["Sequence"], 7);
        let blob = requests[3]["params"][0]["tx_blob"].as_str().expect("blob");
        let d = decode(blob).expect("decode");
        assert_eq!(d["Sequence"], 7);
        assert_eq!(d["Fee"], "15");
        assert_eq!(d["LastLedgerSequence"], 80_000_020);
        assert_eq!(
            d["CancelAfter"], sim["CancelAfter"],
            "relative time fixed once"
        );
    }
}
