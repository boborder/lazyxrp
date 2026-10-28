//! NFToken family: NFTokenMint, NFTokenBurn, NFTokenCreateOffer, NFTokenAcceptOffer,
//! NFTokenCancelOffer. Each `*_tx` builds the typed `xrpl::models` struct once; `*_json` and
//! `*_sign` are its simulate / signed forms (same pattern as `signing.rs`).
//!
//! Not supported (xrpl-rust `NFTokenMintFlag` has no `tfMutable`): minting dynamic NFTs; a
//! zero-price sell offer (the shared amount parser rejects zero).

use color_eyre::eyre::eyre;
use serde_json::Value;
use xrpl::models::Model;
use xrpl::models::transactions::nftoken_accept_offer::NFTokenAcceptOffer;
use xrpl::models::transactions::nftoken_burn::NFTokenBurn;
use xrpl::models::transactions::nftoken_cancel_offer::NFTokenCancelOffer;
use xrpl::models::transactions::nftoken_create_offer::{
    NFTokenCreateOffer, NFTokenCreateOfferFlag,
};
use xrpl::models::transactions::nftoken_mint::{NFTokenMint, NFTokenMintFlag};

use super::{
    Confirm, FieldKind, FieldSpec, Fields, TxSpec, common_fields, flag_bits, id_list, val,
};
use crate::signing::{parse_offer_amount, sign_blob};

const FAMILY: &str = "NFT";
const ID_LEN: usize = 64;
/// rippled `maxTokenURILength`.
const MAX_URI_BYTES: usize = 256;
/// rippled `maxTransferFee` (units of 1/1000 %: 50000 = 50 %).
const MAX_TRANSFER_FEE: u32 = 50_000;
/// rippled `maxTokenOfferCancelCount`.
const MAX_CANCEL_OFFERS: usize = 500;

const MINT_FLAGS: &[(&str, u32)] = &[("burnable", 0x1), ("onlyXRP", 0x2), ("transferable", 0x8)];
const SELL_FLAG: &[(&str, u32)] = &[("sell", 0x1)];
const TRANSFERABLE: u32 = 0x8;

pub static SPECS: &[TxSpec] = &[MINT, BURN, CREATE_OFFER, ACCEPT_OFFER, CANCEL_OFFER];

fn eyre_model<E: std::fmt::Debug>(e: E) -> color_eyre::Report {
    eyre!("{e:?}")
}

fn to_json<T: serde::Serialize>(tx: &T, what: &str) -> color_eyre::Result<Value> {
    serde_json::to_value(tx).map_err(|e| eyre!("{what} tx_json: {e}"))
}

fn id(fields: &Fields, name: &str) -> Option<String> {
    val(fields, name).map(str::to_ascii_uppercase)
}

fn address(fields: &Fields, name: &str) -> Option<String> {
    val(fields, name).map(str::to_string)
}

fn required<'a>(fields: &'a Fields, name: &str) -> color_eyre::Result<&'a str> {
    val(fields, name).ok_or_else(|| eyre!("{name}: required"))
}

fn uint(fields: &Fields, name: &str) -> color_eyre::Result<Option<u32>> {
    val(fields, name)
        .map(|v| v.parse::<u32>().map_err(|e| eyre!("{name}: {e}")))
        .transpose()
}

fn no_check(_: &Fields) -> Result<(), String> {
    Ok(())
}

// ---------------------------------------------------------------- NFTokenMint

pub const MINT: TxSpec = TxSpec {
    tx_type: "NFTokenMint",
    family: FAMILY,
    summary: "mint an NFT",
    fields: &[
        FieldSpec {
            name: "NFTokenTaxon",
            kind: FieldKind::Uint(u32::MAX),
            required: true,
            help: "collection id (integer, 0 is fine)",
            default: "0",
        },
        FieldSpec {
            name: "URI",
            kind: FieldKind::Text(MAX_URI_BYTES),
            required: false,
            help: "plain text, stored hex-encoded (≤256 bytes)",
            default: "",
        },
        FieldSpec {
            name: "Flags",
            kind: FieldKind::Flags(MINT_FLAGS),
            required: false,
            help: "burnable, onlyXRP, transferable (comma-separated)",
            default: "",
        },
        FieldSpec {
            name: "TransferFee",
            kind: FieldKind::Uint(MAX_TRANSFER_FEE),
            required: false,
            help: "0..50000 (1/1000 %); needs transferable",
            default: "",
        },
        FieldSpec {
            name: "Issuer",
            kind: FieldKind::Address,
            required: false,
            help: "mint on behalf of this issuer (needs its NFTokenMinter grant)",
            default: "",
        },
    ],
    confirm: None,
    check: mint_check,
    build_json: mint_json,
    sign: mint_sign,
};

fn mint_check(f: &Fields) -> Result<(), String> {
    let transferable =
        val(f, "Flags").is_some_and(|v| flag_bits(MINT_FLAGS, v) & TRANSFERABLE != 0);
    if val(f, "TransferFee").is_some_and(|v| v != "0") && !transferable {
        return Err("TransferFee: a non-zero fee needs the transferable flag".into());
    }
    Ok(())
}

fn mint_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<NFTokenMint<'static>> {
    let bits = val(f, "Flags").map_or(0, |v| flag_bits(MINT_FLAGS, v));
    Ok(NFTokenMint {
        common_fields: common_fields!(account, NFTokenMint, sequence, fee_lls)
            .with_flags(NFTokenMintFlag::from_bits(bits)),
        nftoken_taxon: uint(f, "NFTokenTaxon")?.ok_or_else(|| eyre!("NFTokenTaxon: required"))?,
        issuer: address(f, "Issuer").map(Into::into),
        transfer_fee: uint(f, "TransferFee")?,
        uri: val(f, "URI").map(|u| hex::encode_upper(u.as_bytes()).into()),
    })
}

fn mint_json(account: &str, f: &Fields, sequence: u32) -> color_eyre::Result<Value> {
    let tx = mint_tx(account, f, sequence, None)?;
    tx.validate().map_err(eyre_model)?;
    to_json(&tx, "NFTokenMint")
}

fn mint_sign(
    wallet: &xrpl::wallet::Wallet,
    account: &str,
    f: &Fields,
    sequence: u32,
    fee: u32,
    lls: u32,
) -> color_eyre::Result<String> {
    let mut tx = mint_tx(account, f, sequence, Some((fee, lls)))?;
    sign_blob!(tx, wallet)
}

// ---------------------------------------------------------------- NFTokenBurn

pub const BURN: TxSpec = TxSpec {
    tx_type: "NFTokenBurn",
    family: FAMILY,
    summary: "destroy an NFT ⚠ irreversible",
    fields: &[
        FieldSpec {
            name: "NFTokenID",
            kind: FieldKind::Hex(ID_LEN),
            required: true,
            help: "64 hex characters",
            default: "",
        },
        FieldSpec {
            name: "Owner",
            kind: FieldKind::Address,
            required: false,
            help: "current holder, when burning someone else's burnable NFT as issuer",
            default: "",
        },
    ],
    confirm: Some(Confirm {
        phrase: "BURN",
        hint: "Irreversible: the NFT is destroyed",
    }),
    check: no_check,
    build_json: burn_json,
    sign: burn_sign,
};

fn burn_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<NFTokenBurn<'static>> {
    Ok(NFTokenBurn {
        common_fields: common_fields!(account, NFTokenBurn, sequence, fee_lls),
        nftoken_id: id(f, "NFTokenID")
            .ok_or_else(|| eyre!("NFTokenID: required"))?
            .into(),
        owner: address(f, "Owner").map(Into::into),
    })
}

fn burn_json(account: &str, f: &Fields, sequence: u32) -> color_eyre::Result<Value> {
    let tx = burn_tx(account, f, sequence, None)?;
    tx.validate().map_err(eyre_model)?;
    to_json(&tx, "NFTokenBurn")
}

fn burn_sign(
    wallet: &xrpl::wallet::Wallet,
    account: &str,
    f: &Fields,
    sequence: u32,
    fee: u32,
    lls: u32,
) -> color_eyre::Result<String> {
    let mut tx = burn_tx(account, f, sequence, Some((fee, lls)))?;
    sign_blob!(tx, wallet)
}

// ---------------------------------------------------------- NFTokenCreateOffer

pub const CREATE_OFFER: TxSpec = TxSpec {
    tx_type: "NFTokenCreateOffer",
    family: FAMILY,
    summary: "offer an NFT for sale / bid on one",
    fields: &[
        FieldSpec {
            name: "NFTokenID",
            kind: FieldKind::Hex(ID_LEN),
            required: true,
            help: "64 hex characters",
            default: "",
        },
        FieldSpec {
            name: "Amount",
            kind: FieldKind::Amount,
            required: true,
            help: "XRP:drops or CUR:issuer:value (> 0)",
            default: "XRP:1000000",
        },
        FieldSpec {
            name: "Flags",
            kind: FieldKind::Flags(SELL_FLAG),
            required: false,
            help: "sell = you own it (sell offer); blank = buy offer",
            default: "",
        },
        FieldSpec {
            name: "Owner",
            kind: FieldKind::Address,
            required: false,
            help: "buy offers only: current holder",
            default: "",
        },
        FieldSpec {
            name: "Destination",
            kind: FieldKind::Address,
            required: false,
            help: "only this account may accept",
            default: "",
        },
        FieldSpec {
            name: "Expiration",
            kind: FieldKind::Time,
            required: false,
            help: "7d / 12h / 90s from now, or Ripple-epoch seconds",
            default: "",
        },
    ],
    confirm: None,
    check: create_offer_check,
    build_json: create_offer_json,
    sign: create_offer_sign,
};

fn is_sell(f: &Fields) -> bool {
    val(f, "Flags").is_some_and(|v| flag_bits(SELL_FLAG, v) & 1 != 0)
}

fn create_offer_check(f: &Fields) -> Result<(), String> {
    match (is_sell(f), val(f, "Owner")) {
        (true, Some(_)) => Err("Owner: only for buy offers (a sell offer is your own NFT)".into()),
        (false, None) => Err("Owner: required for a buy offer (the current holder)".into()),
        _ => Ok(()),
    }
}

fn create_offer_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<NFTokenCreateOffer<'static>> {
    let flags = if is_sell(f) {
        vec![NFTokenCreateOfferFlag::TfSellOffer]
    } else {
        Vec::new()
    };
    Ok(NFTokenCreateOffer {
        common_fields: common_fields!(account, NFTokenCreateOffer, sequence, fee_lls)
            .with_flags(flags),
        nftoken_id: id(f, "NFTokenID")
            .ok_or_else(|| eyre!("NFTokenID: required"))?
            .into(),
        amount: parse_offer_amount(required(f, "Amount")?)?,
        owner: address(f, "Owner").map(Into::into),
        expiration: uint(f, "Expiration")?,
        destination: address(f, "Destination").map(Into::into),
    })
}

fn create_offer_json(account: &str, f: &Fields, sequence: u32) -> color_eyre::Result<Value> {
    let tx = create_offer_tx(account, f, sequence, None)?;
    tx.validate().map_err(eyre_model)?;
    to_json(&tx, "NFTokenCreateOffer")
}

fn create_offer_sign(
    wallet: &xrpl::wallet::Wallet,
    account: &str,
    f: &Fields,
    sequence: u32,
    fee: u32,
    lls: u32,
) -> color_eyre::Result<String> {
    let mut tx = create_offer_tx(account, f, sequence, Some((fee, lls)))?;
    sign_blob!(tx, wallet)
}

// ---------------------------------------------------------- NFTokenAcceptOffer

pub const ACCEPT_OFFER: TxSpec = TxSpec {
    tx_type: "NFTokenAcceptOffer",
    family: FAMILY,
    summary: "accept an NFT offer ⚠ transfers value",
    fields: &[
        FieldSpec {
            name: "NFTokenSellOffer",
            kind: FieldKind::Hex(ID_LEN),
            required: false,
            help: "sell offer id (buy this NFT); 64 hex",
            default: "",
        },
        FieldSpec {
            name: "NFTokenBuyOffer",
            kind: FieldKind::Hex(ID_LEN),
            required: false,
            help: "buy offer id (sell your NFT); 64 hex",
            default: "",
        },
        FieldSpec {
            name: "NFTokenBrokerFee",
            kind: FieldKind::Amount,
            required: false,
            help: "broker mode only (both offers): your cut, XRP:drops / CUR:issuer:value",
            default: "",
        },
    ],
    confirm: Some(Confirm {
        phrase: "ACCEPT",
        hint: "Irreversible: the NFT changes hands and the offer amount is paid",
    }),
    check: accept_offer_check,
    build_json: accept_offer_json,
    sign: accept_offer_sign,
};

fn accept_offer_check(f: &Fields) -> Result<(), String> {
    let (sell, buy, fee) = (
        val(f, "NFTokenSellOffer"),
        val(f, "NFTokenBuyOffer"),
        val(f, "NFTokenBrokerFee"),
    );
    if sell.is_none() && buy.is_none() {
        return Err("NFTokenSellOffer or NFTokenBuyOffer: at least one is required".into());
    }
    if fee.is_some() && (sell.is_none() || buy.is_none()) {
        return Err("NFTokenBrokerFee: brokered mode needs both offers".into());
    }
    Ok(())
}

fn accept_offer_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<NFTokenAcceptOffer<'static>> {
    Ok(NFTokenAcceptOffer {
        common_fields: common_fields!(account, NFTokenAcceptOffer, sequence, fee_lls),
        nftoken_sell_offer: id(f, "NFTokenSellOffer").map(Into::into),
        nftoken_buy_offer: id(f, "NFTokenBuyOffer").map(Into::into),
        nftoken_broker_fee: val(f, "NFTokenBrokerFee")
            .map(parse_offer_amount)
            .transpose()?,
    })
}

fn accept_offer_json(account: &str, f: &Fields, sequence: u32) -> color_eyre::Result<Value> {
    let tx = accept_offer_tx(account, f, sequence, None)?;
    tx.validate().map_err(eyre_model)?;
    to_json(&tx, "NFTokenAcceptOffer")
}

fn accept_offer_sign(
    wallet: &xrpl::wallet::Wallet,
    account: &str,
    f: &Fields,
    sequence: u32,
    fee: u32,
    lls: u32,
) -> color_eyre::Result<String> {
    let mut tx = accept_offer_tx(account, f, sequence, Some((fee, lls)))?;
    sign_blob!(tx, wallet)
}

// ---------------------------------------------------------- NFTokenCancelOffer

pub const CANCEL_OFFER: TxSpec = TxSpec {
    tx_type: "NFTokenCancelOffer",
    family: FAMILY,
    summary: "cancel NFT offers",
    fields: &[FieldSpec {
        name: "NFTokenOffers",
        kind: FieldKind::HexList {
            len: ID_LEN,
            max: MAX_CANCEL_OFFERS,
        },
        required: true,
        help: "offer ids (64 hex each), comma- or space-separated",
        default: "",
    }],
    confirm: None,
    check: no_check,
    build_json: cancel_offer_json,
    sign: cancel_offer_sign,
};

fn cancel_offer_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<NFTokenCancelOffer<'static>> {
    Ok(NFTokenCancelOffer {
        common_fields: common_fields!(account, NFTokenCancelOffer, sequence, fee_lls),
        nftoken_offers: id_list(required(f, "NFTokenOffers")?)
            .into_iter()
            .map(Into::into)
            .collect(),
    })
}

fn cancel_offer_json(account: &str, f: &Fields, sequence: u32) -> color_eyre::Result<Value> {
    let tx = cancel_offer_tx(account, f, sequence, None)?;
    tx.validate().map_err(eyre_model)?;
    to_json(&tx, "NFTokenCancelOffer")
}

fn cancel_offer_sign(
    wallet: &xrpl::wallet::Wallet,
    account: &str,
    f: &Fields,
    sequence: u32,
    fee: u32,
    lls: u32,
) -> color_eyre::Result<String> {
    let mut tx = cancel_offer_tx(account, f, sequence, Some((fee, lls)))?;
    sign_blob!(tx, wallet)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use xrpl::core::binarycodec::decode;

    use super::*;

    const ACCOUNT_B: &str = "rN7n7otQDd6FczFgLdSqtcsAUxDkw6fzRH";
    const ID_A: &str = "000B013A95F14B0044F78A264E41713C64B5F89242540EE208C3098E00000D65";
    const ID_B: &str = "00081B58AC1BD1A0DB8D0C9F7A5A5F0E3E4F12E2C6E6E4D3A5C70D2A00000001";

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

    /// TC-249: per-type field validation — accept / reject with boundaries (TransferFee 50000 /
    /// 50001, URI 256 / 257 bytes incl. multibyte, id lengths 63 / 64 / 65, Taxon u32 max).
    #[test]
    fn field_validation_tables() {
        let uri_256 = "a".repeat(256);
        let uri_257 = "a".repeat(257);
        let uri_255_multibyte = "あ".repeat(85);
        let uri_258_multibyte = "あ".repeat(86);
        let id_63 = &ID_A[..63];
        let id_65 = format!("{ID_A}0");
        let id_non_hex = format!("G{}", &ID_A[1..]);
        let offers_500 = (0..500)
            .map(|i| format!("{i:064X}"))
            .collect::<Vec<_>>()
            .join(",");
        let offers_501 = format!("{offers_500},{:064X}", 500);
        let iou = format!("USD:{ACCOUNT_B}:10.5");

        // (tx type, fields, None = accepted / Some(substring of the error))
        let cases: Vec<(&str, Fields, Option<&str>)> = vec![
            ("NFTokenMint", f(&[("NFTokenTaxon", "0")]), None),
            ("NFTokenMint", f(&[("NFTokenTaxon", "4294967295")]), None),
            (
                "NFTokenMint",
                f(&[("NFTokenTaxon", "4294967296")]),
                Some("NFTokenTaxon"),
            ),
            (
                "NFTokenMint",
                f(&[("NFTokenTaxon", "-1")]),
                Some("NFTokenTaxon"),
            ),
            (
                "NFTokenMint",
                f(&[("NFTokenTaxon", "  ")]),
                Some("NFTokenTaxon: required"),
            ),
            ("NFTokenMint", f(&[]), Some("NFTokenTaxon: required")),
            (
                "NFTokenMint",
                f(&[("NFTokenTaxon", "1"), ("URI", &uri_256)]),
                None,
            ),
            (
                "NFTokenMint",
                f(&[("NFTokenTaxon", "1"), ("URI", &uri_257)]),
                Some("URI"),
            ),
            (
                "NFTokenMint",
                f(&[("NFTokenTaxon", "1"), ("URI", &uri_255_multibyte)]),
                None,
            ),
            (
                "NFTokenMint",
                f(&[("NFTokenTaxon", "1"), ("URI", &uri_258_multibyte)]),
                Some("258 bytes"),
            ),
            (
                "NFTokenMint",
                f(&[
                    ("NFTokenTaxon", "1"),
                    ("Flags", "burnable, onlyXRP,transferable"),
                ]),
                None,
            ),
            (
                "NFTokenMint",
                f(&[("NFTokenTaxon", "1"), ("Flags", "tfBurnable")]),
                None,
            ),
            (
                "NFTokenMint",
                f(&[("NFTokenTaxon", "1"), ("Flags", "mutable")]),
                Some("unknown flag"),
            ),
            (
                "NFTokenMint",
                f(&[
                    ("NFTokenTaxon", "1"),
                    ("Flags", "transferable"),
                    ("TransferFee", "50000"),
                ]),
                None,
            ),
            (
                "NFTokenMint",
                f(&[
                    ("NFTokenTaxon", "1"),
                    ("Flags", "transferable"),
                    ("TransferFee", "50001"),
                ]),
                Some("TransferFee"),
            ),
            (
                "NFTokenMint",
                f(&[("NFTokenTaxon", "1"), ("TransferFee", "100")]),
                Some("transferable"),
            ),
            (
                "NFTokenMint",
                f(&[("NFTokenTaxon", "1"), ("TransferFee", "0")]),
                None,
            ),
            (
                "NFTokenMint",
                f(&[("NFTokenTaxon", "1"), ("Issuer", ACCOUNT_B)]),
                None,
            ),
            (
                "NFTokenMint",
                f(&[("NFTokenTaxon", "1"), ("Issuer", "rBad")]),
                Some("Issuer"),
            ),
            (
                "NFTokenMint",
                f(&[("NFTokenTaxon", "1"), ("Bogus", "1")]),
                Some("unknown field Bogus"),
            ),
            ("NFTokenBurn", f(&[("NFTokenID", ID_A)]), None),
            (
                "NFTokenBurn",
                f(&[("NFTokenID", &ID_A.to_lowercase())]),
                None,
            ),
            ("NFTokenBurn", f(&[("NFTokenID", id_63)]), Some("63")),
            ("NFTokenBurn", f(&[("NFTokenID", &id_65)]), Some("65")),
            ("NFTokenBurn", f(&[("NFTokenID", &id_non_hex)]), Some("hex")),
            ("NFTokenBurn", f(&[]), Some("NFTokenID: required")),
            (
                "NFTokenBurn",
                f(&[("NFTokenID", ID_A), ("Owner", ACCOUNT_B)]),
                None,
            ),
            (
                "NFTokenBurn",
                f(&[("NFTokenID", ID_A), ("Owner", "nope")]),
                Some("Owner"),
            ),
            (
                "NFTokenCreateOffer",
                f(&[
                    ("NFTokenID", ID_A),
                    ("Amount", "XRP:1000000"),
                    ("Flags", "sell"),
                ]),
                None,
            ),
            (
                "NFTokenCreateOffer",
                f(&[("NFTokenID", ID_A), ("Amount", &iou), ("Owner", ACCOUNT_B)]),
                None,
            ),
            (
                "NFTokenCreateOffer",
                f(&[
                    ("NFTokenID", ID_A),
                    ("Amount", "XRP:1"),
                    ("Flags", "sell"),
                    ("Owner", ACCOUNT_B),
                ]),
                Some("only for buy offers"),
            ),
            (
                "NFTokenCreateOffer",
                f(&[("NFTokenID", ID_A), ("Amount", "XRP:1")]),
                Some("Owner: required"),
            ),
            (
                "NFTokenCreateOffer",
                f(&[("NFTokenID", ID_A), ("Amount", "XRP:0"), ("Flags", "sell")]),
                Some("Amount"),
            ),
            (
                "NFTokenCreateOffer",
                f(&[("NFTokenID", ID_A), ("Amount", "1.5"), ("Flags", "sell")]),
                Some("Amount"),
            ),
            (
                "NFTokenCreateOffer",
                f(&[("NFTokenID", ID_A), ("Flags", "sell")]),
                Some("Amount: required"),
            ),
            (
                "NFTokenCreateOffer",
                f(&[
                    ("NFTokenID", ID_A),
                    ("Amount", "XRP:1"),
                    ("Flags", "sell"),
                    ("Expiration", "7d"),
                ]),
                None,
            ),
            (
                "NFTokenCreateOffer",
                f(&[
                    ("NFTokenID", ID_A),
                    ("Amount", "XRP:1"),
                    ("Flags", "sell"),
                    ("Expiration", "4294967295"),
                ]),
                None,
            ),
            (
                "NFTokenCreateOffer",
                f(&[
                    ("NFTokenID", ID_A),
                    ("Amount", "XRP:1"),
                    ("Flags", "sell"),
                    ("Expiration", "4294967296"),
                ]),
                Some("Expiration"),
            ),
            (
                "NFTokenCreateOffer",
                f(&[
                    ("NFTokenID", ID_A),
                    ("Amount", "XRP:1"),
                    ("Flags", "sell"),
                    ("Expiration", "0d"),
                ]),
                Some("Expiration"),
            ),
            (
                "NFTokenCreateOffer",
                f(&[
                    ("NFTokenID", ID_A),
                    ("Amount", "XRP:1"),
                    ("Flags", "sell"),
                    ("Destination", ACCOUNT_B),
                ]),
                None,
            ),
            ("NFTokenAcceptOffer", f(&[]), Some("at least one")),
            ("NFTokenAcceptOffer", f(&[("NFTokenSellOffer", ID_A)]), None),
            ("NFTokenAcceptOffer", f(&[("NFTokenBuyOffer", ID_B)]), None),
            (
                "NFTokenAcceptOffer",
                f(&[
                    ("NFTokenSellOffer", ID_A),
                    ("NFTokenBuyOffer", ID_B),
                    ("NFTokenBrokerFee", "XRP:10"),
                ]),
                None,
            ),
            (
                "NFTokenAcceptOffer",
                f(&[("NFTokenSellOffer", ID_A), ("NFTokenBrokerFee", "XRP:10")]),
                Some("both offers"),
            ),
            (
                "NFTokenAcceptOffer",
                f(&[
                    ("NFTokenSellOffer", ID_A),
                    ("NFTokenBuyOffer", ID_B),
                    ("NFTokenBrokerFee", "XRP:0"),
                ]),
                Some("NFTokenBrokerFee"),
            ),
            (
                "NFTokenAcceptOffer",
                f(&[("NFTokenSellOffer", id_63)]),
                Some("NFTokenSellOffer"),
            ),
            ("NFTokenCancelOffer", f(&[("NFTokenOffers", ID_A)]), None),
            (
                "NFTokenCancelOffer",
                f(&[("NFTokenOffers", &format!("{ID_A}, {ID_B}"))]),
                None,
            ),
            (
                "NFTokenCancelOffer",
                f(&[("NFTokenOffers", &format!("{ID_A} {ID_B}"))]),
                None,
            ),
            (
                "NFTokenCancelOffer",
                f(&[("NFTokenOffers", ",")]),
                Some("1..=500"),
            ),
            (
                "NFTokenCancelOffer",
                f(&[]),
                Some("NFTokenOffers: required"),
            ),
            (
                "NFTokenCancelOffer",
                f(&[("NFTokenOffers", &format!("{ID_A},{}", ID_A.to_lowercase()))]),
                Some("unique"),
            ),
            (
                "NFTokenCancelOffer",
                f(&[("NFTokenOffers", &offers_500)]),
                None,
            ),
            (
                "NFTokenCancelOffer",
                f(&[("NFTokenOffers", &offers_501)]),
                Some("1..=500"),
            ),
            (
                "NFTokenCancelOffer",
                f(&[("NFTokenOffers", &format!("{ID_A},{id_63}"))]),
                Some("63"),
            ),
        ];
        for (tx_type, fields, want) in cases {
            let got = spec(tx_type).validate(&fields);
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

    /// Representative valid forms (fields as the user would type them), one per type.
    fn samples() -> Vec<(&'static str, Fields)> {
        vec![
            (
                "NFTokenMint",
                f(&[
                    ("NFTokenTaxon", "7"),
                    ("URI", "ipfs://bafy/メタ.json"),
                    ("Flags", "burnable,transferable"),
                    ("TransferFee", "5000"),
                    ("Issuer", ACCOUNT_B),
                ]),
            ),
            (
                "NFTokenBurn",
                f(&[("NFTokenID", &ID_A.to_lowercase()), ("Owner", ACCOUNT_B)]),
            ),
            (
                "NFTokenCreateOffer",
                f(&[
                    ("NFTokenID", ID_A),
                    ("Amount", &format!("USD:{ACCOUNT_B}:10.5")),
                    ("Owner", ACCOUNT_B),
                    ("Destination", ACCOUNT_B),
                    ("Expiration", "800000000"),
                ]),
            ),
            (
                "NFTokenAcceptOffer",
                f(&[
                    ("NFTokenSellOffer", ID_A),
                    ("NFTokenBuyOffer", ID_B),
                    ("NFTokenBrokerFee", "XRP:250"),
                ]),
            ),
            (
                "NFTokenCancelOffer",
                f(&[("NFTokenOffers", &format!("{ID_A},{}", ID_B.to_lowercase()))]),
            ),
        ]
    }

    /// TC-250: unsigned `tx_json` per type — type name, account, `Sequence`, typed fields
    /// (URI hex-encoded from text, ids upper-cased, flag bits), and no `Fee` / `LastLedgerSequence`.
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
                "NFTokenMint" => {
                    assert_eq!(j["NFTokenTaxon"], 7);
                    assert_eq!(j["TransferFee"], 5000);
                    assert_eq!(j["Flags"], 0x1 | 0x8);
                    assert_eq!(j["Issuer"], ACCOUNT_B);
                    assert_eq!(
                        j["URI"],
                        hex::encode_upper("ipfs://bafy/メタ.json".as_bytes())
                    );
                }
                "NFTokenBurn" => {
                    assert_eq!(j["NFTokenID"], ID_A);
                    assert_eq!(j["Owner"], ACCOUNT_B);
                }
                "NFTokenCreateOffer" => {
                    assert_eq!(j["NFTokenID"], ID_A);
                    assert_eq!(
                        j["Amount"],
                        json!({"currency": "USD", "issuer": ACCOUNT_B, "value": "10.5"})
                    );
                    assert_eq!(j["Owner"], ACCOUNT_B);
                    assert_eq!(j["Destination"], ACCOUNT_B);
                    assert_eq!(j["Expiration"], 800_000_000);
                    assert_eq!(j["Flags"], 0, "buy offer has no tfSellNFToken");
                }
                "NFTokenAcceptOffer" => {
                    assert_eq!(j["NFTokenSellOffer"], ID_A);
                    assert_eq!(j["NFTokenBuyOffer"], ID_B);
                    assert_eq!(j["NFTokenBrokerFee"], "250");
                }
                "NFTokenCancelOffer" => {
                    assert_eq!(j["NFTokenOffers"], json!([ID_A, ID_B]));
                }
                other => panic!("sample for {other} is not asserted"),
            }
        }
        // Sell offer sets tfSellNFToken and carries a drops string amount.
        let sell = spec("NFTokenCreateOffer")
            .prepare(
                &f(&[
                    ("NFTokenID", ID_A),
                    ("Amount", "XRP:1000000"),
                    ("Flags", "sell"),
                ]),
                0,
            )
            .unwrap();
        let j = (CREATE_OFFER.build_json)(&account, &sell, 1).unwrap();
        assert_eq!((&j["Flags"], &j["Amount"]), (&json!(1), &json!("1000000")));
        assert!(j.get("Owner").is_none());
    }

    /// TC-251: signed blobs decode to the same fields with the simulate-chosen Sequence / Fee /
    /// LastLedgerSequence (distinct values, so swapped arguments fail) and carry a signature.
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
                "NFTokenMint" => {
                    assert_eq!(d["NFTokenTaxon"], 7);
                    assert_eq!(d["TransferFee"], 5000);
                    assert_eq!(d["Flags"], 0x9);
                    assert_eq!(d["Issuer"], ACCOUNT_B);
                    assert_eq!(
                        d["URI"],
                        hex::encode_upper("ipfs://bafy/メタ.json".as_bytes())
                    );
                }
                "NFTokenBurn" => {
                    assert_eq!(d["NFTokenID"], ID_A);
                    assert_eq!(d["Owner"], ACCOUNT_B);
                }
                "NFTokenCreateOffer" => {
                    assert_eq!(d["NFTokenID"], ID_A);
                    assert_eq!(d["Amount"]["currency"], "USD");
                    assert_eq!(d["Expiration"], 800_000_000);
                    assert_eq!(d["Destination"], ACCOUNT_B);
                }
                "NFTokenAcceptOffer" => {
                    assert_eq!(d["NFTokenSellOffer"], ID_A);
                    assert_eq!(d["NFTokenBuyOffer"], ID_B);
                    assert_eq!(d["NFTokenBrokerFee"], "250");
                }
                "NFTokenCancelOffer" => {
                    assert_eq!(d["NFTokenOffers"], json!([ID_A, ID_B]));
                }
                other => panic!("sample for {other} is not asserted"),
            }
        }
    }

    /// The crate's own model rules run when the unsigned JSON is built, before any RPC
    /// (Issuer = signer, Owner = signer on a buy offer).
    #[test]
    fn builder_applies_model_rules() {
        let account = wallet().classic_address.clone();
        let own_issuer = spec("NFTokenMint")
            .prepare(&f(&[("NFTokenTaxon", "1"), ("Issuer", &account)]), 0)
            .unwrap();
        let err = (MINT.build_json)(&account, &own_issuer, 1).unwrap_err();
        assert!(err.to_string().contains("ValueEqualsValue"), "{err}");
        let own_owner = spec("NFTokenCreateOffer")
            .prepare(
                &f(&[
                    ("NFTokenID", ID_A),
                    ("Amount", "XRP:1"),
                    ("Owner", &account),
                ]),
                0,
            )
            .unwrap();
        let err = (CREATE_OFFER.build_json)(&account, &own_owner, 1).unwrap_err();
        assert!(err.to_string().contains("ValueEqualsValue"), "{err}");
    }
}
