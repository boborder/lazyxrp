//! Identity family: CredentialCreate / CredentialAccept / CredentialDelete, DIDSet / DIDDelete,
//! OracleSet / OracleDelete, PermissionedDomainSet / PermissionedDomainDelete. Same pattern as
//! `nft.rs`; this file also owns the small helpers `mpt.rs` reuses.
//!
//! Blob fields (CredentialType, URI, DID Data / DIDDocument / URI, Oracle Provider / URI /
//! AssetClass) are typed as plain text and hex-encoded; blank cannot clear a field (DIDSet's
//! "empty string removes the field" is not exposed).
//!
//! Limits follow the locked xrpl-rust 1.3.0 model where it is stricter than rippled: its
//! `CredentialCreate` caps the URI at 256 *hex* characters, i.e. 128 bytes.
//!
//! Deliberately flat grammars: `PriceDataSeries` = `BASE/QUOTE=price@scale[,…]` (decimal price,
//! sent as hex; `BASE/QUOTE` alone deletes that pair), `AcceptedCredentials` =
//! `rIssuer:CredentialType[,…]` (type text without spaces / commas).

use std::collections::BTreeSet;

use color_eyre::eyre::eyre;
use serde_json::Value;
use xrpl::models::Model;
use xrpl::models::transactions::credential_accept::CredentialAccept;
use xrpl::models::transactions::credential_create::CredentialCreate;
use xrpl::models::transactions::credential_delete::CredentialDelete;
use xrpl::models::transactions::did_delete::DIDDelete;
use xrpl::models::transactions::did_set::DIDSet;
use xrpl::models::transactions::oracle_delete::OracleDelete;
use xrpl::models::transactions::oracle_set::OracleSet;
use xrpl::models::transactions::permissioned_domain_delete::PermissionedDomainDelete;
use xrpl::models::transactions::permissioned_domain_set::PermissionedDomainSet;
use xrpl::models::transactions::{Credential, PriceData};

use super::{Confirm, FieldKind, FieldSpec, Fields, TxSpec, common_fields, split_list, val};
use crate::signing::sign_blob;

const FAMILY: &str = "Identity";
const ID_LEN: usize = 64;
/// rippled `maxCredentialTypeLength` (bytes).
const MAX_CREDENTIAL_TYPE_BYTES: usize = 64;
/// xrpl-rust model cap (128 bytes); rippled allows 256.
const MAX_CREDENTIAL_URI_BYTES: usize = 128;
/// rippled `maxDIDDataLength` / `maxDIDURILength` / `maxDIDDocumentLength`.
const MAX_DID_BYTES: usize = 256;
const MAX_ORACLE_BLOB_BYTES: usize = 256;
const MAX_ORACLE_ASSET_CLASS_BYTES: usize = 16;
const MAX_PRICE_SERIES: usize = 10;
const MAX_PRICE_SCALE: u8 = 20;
const MAX_DOMAIN_CREDENTIALS: usize = 10;

pub static SPECS: &[TxSpec] = &[
    CREDENTIAL_CREATE,
    CREDENTIAL_ACCEPT,
    CREDENTIAL_DELETE,
    DID_SET,
    DID_DELETE,
    ORACLE_SET,
    ORACLE_DELETE,
    DOMAIN_SET,
    DOMAIN_DELETE,
];

pub(super) fn eyre_model<E: std::fmt::Debug>(e: E) -> color_eyre::Report {
    eyre!("{e:?}")
}

pub(super) fn to_json<T: serde::Serialize>(tx: &T, what: &str) -> color_eyre::Result<Value> {
    serde_json::to_value(tx).map_err(|e| eyre!("{what} tx_json: {e}"))
}

pub(super) fn required<'a>(fields: &'a Fields, name: &str) -> color_eyre::Result<&'a str> {
    val(fields, name).ok_or_else(|| eyre!("{name}: required"))
}

pub(super) fn upper(fields: &Fields, name: &str) -> Option<String> {
    val(fields, name).map(str::to_ascii_uppercase)
}

pub(super) fn address(fields: &Fields, name: &str) -> Option<String> {
    val(fields, name).map(str::to_string)
}

pub(super) fn uint(fields: &Fields, name: &str) -> color_eyre::Result<Option<u32>> {
    val(fields, name)
        .map(|v| v.parse::<u32>().map_err(|e| eyre!("{name}: {e}")))
        .transpose()
}

pub(super) fn text_hex(fields: &Fields, name: &str) -> Option<String> {
    val(fields, name).map(|t| hex::encode_upper(t.as_bytes()))
}

pub(super) fn no_check(_: &Fields) -> Result<(), String> {
    Ok(())
}

/// 64-hex ledger-object id that is not all zeros (the crate rejects the zero hash).
pub(super) fn non_zero_id(v: &str) -> Result<(), String> {
    FieldKind::Hex(ID_LEN).validate(v)?;
    if v.bytes().all(|b| b == b'0') {
        return Err("must not be the zero hash".into());
    }
    Ok(())
}

/// `<name>_json` (unsigned, model-validated) and `<name>_sign` from a `fn tx(account, fields,
/// sequence, fee_lls) -> Result<Tx>`.
macro_rules! tx_fns {
    ($json:ident, $sign:ident, $tx:ident, $what:literal) => {
        fn $json(account: &str, f: &Fields, sequence: u32) -> color_eyre::Result<Value> {
            let tx = $tx(account, f, sequence, None)?;
            tx.validate().map_err(eyre_model)?;
            to_json(&tx, $what)
        }

        fn $sign(
            wallet: &xrpl::wallet::Wallet,
            account: &str,
            f: &Fields,
            sequence: u32,
            fee: u32,
            lls: u32,
        ) -> color_eyre::Result<String> {
            let mut tx = $tx(account, f, sequence, Some((fee, lls)))?;
            sign_blob!(tx, wallet)
        }
    };
}
pub(super) use tx_fns;

const fn field(
    name: &'static str,
    kind: FieldKind,
    required: bool,
    help: &'static str,
) -> FieldSpec {
    FieldSpec {
        name,
        kind,
        required,
        help,
        default: "",
    }
}

const CREDENTIAL_TYPE: FieldSpec = field(
    "CredentialType",
    FieldKind::Text(MAX_CREDENTIAL_TYPE_BYTES),
    true,
    "plain text, stored hex-encoded (1..64 bytes)",
);

// ----------------------------------------------------------- CredentialCreate

pub const CREDENTIAL_CREATE: TxSpec = TxSpec {
    tx_type: "CredentialCreate",
    family: FAMILY,
    summary: "issue a credential to a subject",
    fields: &[
        field(
            "Subject",
            FieldKind::Address,
            true,
            "account the credential is for",
        ),
        CREDENTIAL_TYPE,
        field(
            "Expiration",
            FieldKind::Time,
            false,
            "90s / 30m / 2h / 7d from now, or Ripple-epoch seconds",
        ),
        field(
            "URI",
            FieldKind::Text(MAX_CREDENTIAL_URI_BYTES),
            false,
            "plain text, stored hex-encoded (≤128 bytes)",
        ),
    ],
    confirm: None,
    check: no_check,
    build_json: credential_create_json,
    sign: credential_create_sign,
};

fn credential_create_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<CredentialCreate<'static>> {
    Ok(CredentialCreate {
        common_fields: common_fields!(account, CredentialCreate, sequence, fee_lls),
        subject: required(f, "Subject")?.to_string().into(),
        credential_type: text_hex(f, "CredentialType")
            .ok_or_else(|| eyre!("CredentialType: required"))?
            .into(),
        expiration: uint(f, "Expiration")?,
        uri: text_hex(f, "URI").map(Into::into),
    })
}
tx_fns!(
    credential_create_json,
    credential_create_sign,
    credential_create_tx,
    "CredentialCreate"
);

// ----------------------------------------------------------- CredentialAccept

pub const CREDENTIAL_ACCEPT: TxSpec = TxSpec {
    tx_type: "CredentialAccept",
    family: FAMILY,
    summary: "accept a credential issued to you",
    fields: &[
        field(
            "Issuer",
            FieldKind::Address,
            true,
            "account that issued the credential",
        ),
        CREDENTIAL_TYPE,
    ],
    confirm: None,
    check: no_check,
    build_json: credential_accept_json,
    sign: credential_accept_sign,
};

fn credential_accept_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<CredentialAccept<'static>> {
    Ok(CredentialAccept {
        common_fields: common_fields!(account, CredentialAccept, sequence, fee_lls),
        issuer: required(f, "Issuer")?.to_string().into(),
        credential_type: text_hex(f, "CredentialType")
            .ok_or_else(|| eyre!("CredentialType: required"))?
            .into(),
    })
}
tx_fns!(
    credential_accept_json,
    credential_accept_sign,
    credential_accept_tx,
    "CredentialAccept"
);

// ----------------------------------------------------------- CredentialDelete

pub const CREDENTIAL_DELETE: TxSpec = TxSpec {
    tx_type: "CredentialDelete",
    family: FAMILY,
    summary: "delete a credential ⚠ irreversible",
    fields: &[
        field(
            "Subject",
            FieldKind::Address,
            false,
            "credential holder (default: you)",
        ),
        field(
            "Issuer",
            FieldKind::Address,
            false,
            "credential issuer (default: you)",
        ),
        CREDENTIAL_TYPE,
    ],
    confirm: Some(Confirm {
        phrase: "DELETE",
        hint: "Irreversible: the credential is removed from the ledger",
    }),
    check: credential_delete_check,
    build_json: credential_delete_json,
    sign: credential_delete_sign,
};

fn credential_delete_check(f: &Fields) -> Result<(), String> {
    if val(f, "Subject").is_none() && val(f, "Issuer").is_none() {
        return Err("Subject or Issuer: set at least one".into());
    }
    Ok(())
}

fn credential_delete_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<CredentialDelete<'static>> {
    Ok(CredentialDelete {
        common_fields: common_fields!(account, CredentialDelete, sequence, fee_lls),
        subject: address(f, "Subject").map(Into::into),
        issuer: address(f, "Issuer").map(Into::into),
        credential_type: text_hex(f, "CredentialType")
            .ok_or_else(|| eyre!("CredentialType: required"))?
            .into(),
    })
}
tx_fns!(
    credential_delete_json,
    credential_delete_sign,
    credential_delete_tx,
    "CredentialDelete"
);

// --------------------------------------------------------------------- DIDSet

const DID_TEXT: FieldKind = FieldKind::Text(MAX_DID_BYTES);

pub const DID_SET: TxSpec = TxSpec {
    tx_type: "DIDSet",
    family: FAMILY,
    summary: "create or update your DID",
    fields: &[
        field(
            "Data",
            DID_TEXT,
            false,
            "public attestations, plain text hex-encoded (≤256 bytes)",
        ),
        field(
            "DIDDocument",
            DID_TEXT,
            false,
            "DID document, plain text hex-encoded (≤256 bytes)",
        ),
        field(
            "URI",
            DID_TEXT,
            false,
            "URI of the DID document, plain text hex-encoded (≤256 bytes)",
        ),
    ],
    confirm: None,
    check: did_set_check,
    build_json: did_set_json,
    sign: did_set_sign,
};

fn did_set_check(f: &Fields) -> Result<(), String> {
    if ["Data", "DIDDocument", "URI"]
        .iter()
        .all(|n| val(f, n).is_none())
    {
        return Err("set at least one of Data, DIDDocument, URI".into());
    }
    Ok(())
}

fn did_set_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<DIDSet<'static>> {
    Ok(DIDSet {
        common_fields: common_fields!(account, DIDSet, sequence, fee_lls),
        data: text_hex(f, "Data").map(Into::into),
        did_document: text_hex(f, "DIDDocument").map(Into::into),
        uri: text_hex(f, "URI").map(Into::into),
    })
}
tx_fns!(did_set_json, did_set_sign, did_set_tx, "DIDSet");

// ------------------------------------------------------------------ DIDDelete

pub const DID_DELETE: TxSpec = TxSpec {
    tx_type: "DIDDelete",
    family: FAMILY,
    summary: "delete your DID ⚠ irreversible",
    fields: &[],
    confirm: Some(Confirm {
        phrase: "DELETE",
        hint: "Irreversible: your DID ledger entry is removed",
    }),
    check: no_check,
    build_json: did_delete_json,
    sign: did_delete_sign,
};

fn did_delete_tx(
    account: &str,
    _: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<DIDDelete<'static>> {
    Ok(DIDDelete {
        common_fields: common_fields!(account, DIDDelete, sequence, fee_lls),
    })
}
tx_fns!(did_delete_json, did_delete_sign, did_delete_tx, "DIDDelete");

// ------------------------------------------------------------------ OracleSet

const ORACLE_DOCUMENT_ID: FieldSpec = field(
    "OracleDocumentID",
    FieldKind::Uint(u32::MAX),
    true,
    "your id for this oracle (integer)",
);

pub const ORACLE_SET: TxSpec = TxSpec {
    tx_type: "OracleSet",
    family: FAMILY,
    summary: "create or update a price oracle",
    fields: &[
        ORACLE_DOCUMENT_ID,
        field(
            "LastUpdateTime",
            FieldKind::Uint(u32::MAX),
            true,
            "Unix seconds of this update (ledger rejects far-off times)",
        ),
        field(
            "PriceDataSeries",
            FieldKind::Custom(price_series_ok),
            true,
            "BASE/QUOTE=price@scale, comma-separated (1..10); BASE/QUOTE alone deletes the pair; price is decimal, scale 0..20",
        ),
        field(
            "Provider",
            FieldKind::Text(MAX_ORACLE_BLOB_BYTES),
            false,
            "data provider, plain text hex-encoded (required on create, ≤256 bytes)",
        ),
        field(
            "AssetClass",
            FieldKind::Text(MAX_ORACLE_ASSET_CLASS_BYTES),
            false,
            "e.g. currency, commodity; hex-encoded (required on create, ≤16 bytes)",
        ),
        field(
            "URI",
            FieldKind::Text(MAX_ORACLE_BLOB_BYTES),
            false,
            "extra info URI, plain text hex-encoded (≤256 bytes)",
        ),
    ],
    confirm: None,
    check: no_check,
    build_json: oracle_set_json,
    sign: oracle_set_sign,
};

fn price_series_ok(v: &str) -> Result<(), String> {
    parse_price_series(v).map(|_| ())
}

/// `BASE/QUOTE=price@scale` entries → crate `PriceData` (price decimal → hex, as the ledger wants).
fn parse_price_series(v: &str) -> Result<Vec<PriceData>, String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in split_list(v) {
        let (pair, rest) = match entry.split_once('=') {
            Some((p, r)) => (p, Some(r)),
            None => (entry, None),
        };
        let (base, quote) = pair
            .split_once('/')
            .ok_or_else(|| format!("{entry:?}: expected BASE/QUOTE[=price@scale]"))?;
        for c in [base, quote] {
            if !(xrpl::utils::is_iso_code(c) || xrpl::utils::is_iso_hex(c)) {
                return Err(format!("{c:?}: expected a 3-character code or 40 hex"));
            }
        }
        if base == quote {
            return Err(format!("{pair}: base and quote must differ"));
        }
        if !seen.insert((base, quote)) {
            return Err(format!("{pair}: listed twice"));
        }
        let (asset_price, scale) = match rest {
            None => (None, None),
            Some(r) => {
                let (price, scale) = r
                    .split_once('@')
                    .ok_or_else(|| format!("{entry:?}: expected price@scale"))?;
                let price = price
                    .parse::<u64>()
                    .map_err(|_| format!("{price:?}: price must be an integer 0..=2^64-1"))?;
                let scale = scale
                    .parse::<u8>()
                    .ok()
                    .filter(|s| *s <= MAX_PRICE_SCALE)
                    .ok_or_else(|| format!("{scale:?}: scale must be 0..={MAX_PRICE_SCALE}"))?;
                (Some(format!("{price:X}")), Some(scale))
            }
        };
        out.push(PriceData {
            base_asset: base.to_string(),
            quote_asset: quote.to_string(),
            asset_price,
            scale,
        });
    }
    if out.is_empty() || out.len() > MAX_PRICE_SERIES {
        return Err(format!("expected 1..={MAX_PRICE_SERIES} entries"));
    }
    Ok(out)
}

fn oracle_set_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<OracleSet<'static>> {
    Ok(OracleSet {
        common_fields: common_fields!(account, OracleSet, sequence, fee_lls),
        oracle_document_id: uint(f, "OracleDocumentID")?
            .ok_or_else(|| eyre!("OracleDocumentID: required"))?,
        provider: text_hex(f, "Provider").map(Into::into),
        uri: text_hex(f, "URI").map(Into::into),
        asset_class: text_hex(f, "AssetClass").map(Into::into),
        last_update_time: uint(f, "LastUpdateTime")?
            .ok_or_else(|| eyre!("LastUpdateTime: required"))?,
        price_data_series: parse_price_series(required(f, "PriceDataSeries")?)
            .map_err(|e| eyre!("PriceDataSeries: {e}"))?,
    })
}
tx_fns!(oracle_set_json, oracle_set_sign, oracle_set_tx, "OracleSet");

// --------------------------------------------------------------- OracleDelete

pub const ORACLE_DELETE: TxSpec = TxSpec {
    tx_type: "OracleDelete",
    family: FAMILY,
    summary: "delete one of your oracles ⚠ irreversible",
    fields: &[ORACLE_DOCUMENT_ID],
    confirm: Some(Confirm {
        phrase: "DELETE",
        hint: "Irreversible: the oracle and its price data are removed",
    }),
    check: no_check,
    build_json: oracle_delete_json,
    sign: oracle_delete_sign,
};

fn oracle_delete_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<OracleDelete<'static>> {
    Ok(OracleDelete {
        common_fields: common_fields!(account, OracleDelete, sequence, fee_lls),
        oracle_document_id: uint(f, "OracleDocumentID")?
            .ok_or_else(|| eyre!("OracleDocumentID: required"))?,
    })
}
tx_fns!(
    oracle_delete_json,
    oracle_delete_sign,
    oracle_delete_tx,
    "OracleDelete"
);

// ------------------------------------------------------- PermissionedDomainSet

const DOMAIN_ID_FIELD: FieldSpec = field(
    "DomainID",
    FieldKind::Custom(non_zero_id),
    true,
    "64 hex characters (non-zero)",
);

pub const DOMAIN_SET: TxSpec = TxSpec {
    tx_type: "PermissionedDomainSet",
    family: FAMILY,
    summary: "create or update a permissioned domain",
    fields: &[
        field(
            "AcceptedCredentials",
            FieldKind::Custom(credentials_ok),
            true,
            "rIssuer:CredentialType, comma-separated (1..10, no spaces in the type); replaces the list",
        ),
        FieldSpec {
            required: false,
            help: "existing domain to update (omit to create)",
            ..DOMAIN_ID_FIELD
        },
    ],
    confirm: None,
    check: no_check,
    build_json: domain_set_json,
    sign: domain_set_sign,
};

fn credentials_ok(v: &str) -> Result<(), String> {
    parse_credentials(v).map(|_| ())
}

/// `rIssuer:CredentialType` entries → crate `Credential` (type hex-encoded from text).
fn parse_credentials(v: &str) -> Result<Vec<Credential>, String> {
    let mut out: Vec<Credential> = Vec::new();
    for entry in split_list(v) {
        let (issuer, ctype) = entry
            .split_once(':')
            .ok_or_else(|| format!("{entry:?}: expected rIssuer:CredentialType"))?;
        FieldKind::Address
            .validate(issuer)
            .map_err(|e| format!("{issuer:?}: {e}"))?;
        if ctype.is_empty() || ctype.len() > MAX_CREDENTIAL_TYPE_BYTES {
            return Err(format!(
                "{entry:?}: CredentialType must be 1..={MAX_CREDENTIAL_TYPE_BYTES} bytes"
            ));
        }
        let credential_type = hex::encode_upper(ctype.as_bytes());
        if out
            .iter()
            .any(|c| c.issuer == issuer && c.credential_type == credential_type)
        {
            return Err(format!("{entry}: listed twice"));
        }
        out.push(Credential {
            issuer: issuer.to_string(),
            credential_type,
        });
    }
    if out.is_empty() || out.len() > MAX_DOMAIN_CREDENTIALS {
        return Err(format!("expected 1..={MAX_DOMAIN_CREDENTIALS} credentials"));
    }
    Ok(out)
}

fn domain_set_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<PermissionedDomainSet<'static>> {
    Ok(PermissionedDomainSet {
        common_fields: common_fields!(account, PermissionedDomainSet, sequence, fee_lls),
        domain_id: upper(f, "DomainID").map(Into::into),
        accepted_credentials: parse_credentials(required(f, "AcceptedCredentials")?)
            .map_err(|e| eyre!("AcceptedCredentials: {e}"))?,
    })
}
tx_fns!(
    domain_set_json,
    domain_set_sign,
    domain_set_tx,
    "PermissionedDomainSet"
);

// ---------------------------------------------------- PermissionedDomainDelete

pub const DOMAIN_DELETE: TxSpec = TxSpec {
    tx_type: "PermissionedDomainDelete",
    family: FAMILY,
    summary: "delete a permissioned domain ⚠ irreversible",
    fields: &[DOMAIN_ID_FIELD],
    confirm: Some(Confirm {
        phrase: "DELETE",
        hint: "Irreversible: the domain is removed from the ledger",
    }),
    check: no_check,
    build_json: domain_delete_json,
    sign: domain_delete_sign,
};

fn domain_delete_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<PermissionedDomainDelete<'static>> {
    Ok(PermissionedDomainDelete {
        common_fields: common_fields!(account, PermissionedDomainDelete, sequence, fee_lls),
        domain_id: upper(f, "DomainID")
            .ok_or_else(|| eyre!("DomainID: required"))?
            .into(),
    })
}
tx_fns!(
    domain_delete_json,
    domain_delete_sign,
    domain_delete_tx,
    "PermissionedDomainDelete"
);

#[cfg(test)]
pub(super) mod tests {
    use serde_json::json;
    use xrpl::core::binarycodec::decode;

    use super::*;

    pub(in crate::xrpl::txspec) const B: &str = "rN7n7otQDd6FczFgLdSqtcsAUxDkw6fzRH";
    const C: &str = "rsA2LpzuawewSBQXkiju3YQTMzW13pAAdW";
    const DOMAIN: &str = "5E9C7E5C1A3F4B2D8A6C0E9F1B3D5A7C9E1F3B5D7A9C1E3F5B7D9A1C3E5F7B9D";

    pub(in crate::xrpl::txspec) fn f(pairs: &[(&str, &str)]) -> Fields {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    pub(in crate::xrpl::txspec) fn wallet() -> xrpl::wallet::Wallet {
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

    pub(in crate::xrpl::txspec) fn spec(name: &str) -> &'static TxSpec {
        super::super::lookup(name).unwrap_or_else(|| panic!("{name} registered"))
    }

    /// Run every `(type, fields, None = accepted / Some(error substring))` through `validate`.
    pub(in crate::xrpl::txspec) fn run_cases(cases: Vec<(&str, Fields, Option<&str>)>) {
        for (tx_type, fields, want) in cases {
            let got = spec(tx_type).validate(&fields);
            match want {
                None => assert!(got.is_ok(), "{tx_type} {fields:?} should pass: {got:?}"),
                Some(needle) => {
                    let err = got.expect_err(&format!("{tx_type} {fields:?} should fail"));
                    assert!(err.contains(needle), "{tx_type} {fields:?}: {err:?}");
                }
            }
        }
    }

    /// TC-300: per-type field validation — CredentialType 64 / 65 bytes, URI 128 / 129, DID field
    /// 256 / 257 bytes, at-least-one rules, Oracle id / provider / asset class / series limits,
    /// domain ids (length, hex, zero hash), AcceptedCredentials count 10 / 11.
    #[test]
    fn field_validation_tables() {
        let t64 = "a".repeat(64);
        let t65 = "a".repeat(65);
        let uri_128 = "u".repeat(128);
        let uri_129 = "u".repeat(129);
        let did_256 = "d".repeat(256);
        let did_257 = "d".repeat(257);
        let series_10 = (0..10)
            .map(|i| format!("AA{}/USD=1@0", (b'A' + i) as char))
            .collect::<Vec<_>>()
            .join(",");
        let series_11 = format!("{series_10},ZZZ/USD=1@0");
        let creds_10 = (0..10)
            .map(|i| format!("{B}:t{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let creds_11 = format!("{creds_10},{B}:t10");
        let zero = "0".repeat(64);
        let cases: Vec<(&str, Fields, Option<&str>)> = vec![
            (
                "CredentialCreate",
                f(&[("Subject", B), ("CredentialType", "KYC")]),
                None,
            ),
            (
                "CredentialCreate",
                f(&[("Subject", B), ("CredentialType", &t64), ("URI", &uri_128)]),
                None,
            ),
            (
                "CredentialCreate",
                f(&[("Subject", B), ("CredentialType", &t65)]),
                Some("CredentialType"),
            ),
            (
                "CredentialCreate",
                f(&[("Subject", B), ("CredentialType", "k"), ("URI", &uri_129)]),
                Some("URI"),
            ),
            (
                "CredentialCreate",
                f(&[("CredentialType", "KYC")]),
                Some("Subject: required"),
            ),
            (
                "CredentialCreate",
                f(&[("Subject", "rBad"), ("CredentialType", "KYC")]),
                Some("Subject"),
            ),
            (
                "CredentialCreate",
                f(&[
                    ("Subject", B),
                    ("CredentialType", "KYC"),
                    ("Expiration", "1h"),
                ]),
                None,
            ),
            (
                "CredentialCreate",
                f(&[
                    ("Subject", B),
                    ("CredentialType", "KYC"),
                    ("Expiration", "x"),
                ]),
                Some("Expiration"),
            ),
            (
                "CredentialAccept",
                f(&[("Issuer", B), ("CredentialType", "KYC")]),
                None,
            ),
            (
                "CredentialAccept",
                f(&[("CredentialType", "KYC")]),
                Some("Issuer: required"),
            ),
            (
                "CredentialDelete",
                f(&[("Subject", B), ("CredentialType", "KYC")]),
                None,
            ),
            (
                "CredentialDelete",
                f(&[("Issuer", B), ("CredentialType", "KYC")]),
                None,
            ),
            (
                "CredentialDelete",
                f(&[("CredentialType", "KYC")]),
                Some("Subject or Issuer"),
            ),
            ("DIDSet", f(&[("Data", "x")]), None),
            ("DIDSet", f(&[("Data", &did_256), ("URI", "u")]), None),
            (
                "DIDSet",
                f(&[("DIDDocument", &did_257)]),
                Some("DIDDocument"),
            ),
            ("DIDSet", f(&[("URI", &did_257)]), Some("URI")),
            ("DIDSet", f(&[]), Some("at least one")),
            ("DIDSet", f(&[("Data", "  ")]), Some("at least one")),
            ("DIDDelete", f(&[]), None),
            ("DIDDelete", f(&[("Data", "x")]), Some("unknown field")),
            (
                "OracleSet",
                f(&[
                    ("OracleDocumentID", "1"),
                    ("LastUpdateTime", "1700000000"),
                    ("PriceDataSeries", "XRP/USD=740@2"),
                ]),
                None,
            ),
            (
                "OracleSet",
                f(&[
                    ("OracleDocumentID", "4294967295"),
                    ("LastUpdateTime", "4294967295"),
                    ("PriceDataSeries", &series_10),
                    ("Provider", "p"),
                    ("AssetClass", "currency"),
                    ("URI", "u"),
                ]),
                None,
            ),
            (
                "OracleSet",
                f(&[
                    ("OracleDocumentID", "4294967296"),
                    ("LastUpdateTime", "1"),
                    ("PriceDataSeries", "XRP/USD=1@0"),
                ]),
                Some("OracleDocumentID"),
            ),
            (
                "OracleSet",
                f(&[
                    ("OracleDocumentID", "1"),
                    ("LastUpdateTime", "1"),
                    ("PriceDataSeries", &series_11),
                ]),
                Some("1..=10"),
            ),
            (
                "OracleSet",
                f(&[
                    ("OracleDocumentID", "1"),
                    ("LastUpdateTime", "1"),
                    ("PriceDataSeries", "XRP/USD=1@0"),
                    ("AssetClass", &"c".repeat(17)),
                ]),
                Some("AssetClass"),
            ),
            (
                "OracleSet",
                f(&[
                    ("OracleDocumentID", "1"),
                    ("LastUpdateTime", "1"),
                    ("PriceDataSeries", "XRP/USD=1@0"),
                    ("Provider", &"p".repeat(257)),
                ]),
                Some("Provider"),
            ),
            (
                "OracleSet",
                f(&[
                    ("OracleDocumentID", "1"),
                    ("PriceDataSeries", "XRP/USD=1@0"),
                ]),
                Some("LastUpdateTime: required"),
            ),
            ("OracleDelete", f(&[("OracleDocumentID", "0")]), None),
            ("OracleDelete", f(&[]), Some("OracleDocumentID: required")),
            (
                "PermissionedDomainSet",
                f(&[("AcceptedCredentials", &format!("{B}:KYC"))]),
                None,
            ),
            (
                "PermissionedDomainSet",
                f(&[("AcceptedCredentials", &creds_10), ("DomainID", DOMAIN)]),
                None,
            ),
            (
                "PermissionedDomainSet",
                f(&[("AcceptedCredentials", &creds_11)]),
                Some("1..=10"),
            ),
            (
                "PermissionedDomainSet",
                f(&[
                    ("AcceptedCredentials", &format!("{B}:KYC")),
                    ("DomainID", &zero),
                ]),
                Some("zero hash"),
            ),
            (
                "PermissionedDomainSet",
                f(&[
                    ("AcceptedCredentials", &format!("{B}:KYC")),
                    ("DomainID", &DOMAIN[..63]),
                ]),
                Some("64 hex"),
            ),
            ("PermissionedDomainSet", f(&[]), Some("required")),
            ("PermissionedDomainDelete", f(&[("DomainID", DOMAIN)]), None),
            (
                "PermissionedDomainDelete",
                f(&[("DomainID", &zero)]),
                Some("zero hash"),
            ),
            (
                "PermissionedDomainDelete",
                f(&[]),
                Some("DomainID: required"),
            ),
        ];
        run_cases(cases);
    }

    /// TC-308: the flat grammars — `PriceDataSeries` (decimal price → hex, delete form, bad
    /// currency / duplicate pair / base = quote / scale 21 / missing scale) and
    /// `AcceptedCredentials` (type hex-encoded, duplicates, bad issuer, empty / oversized type).
    #[test]
    fn flat_grammars() {
        let series = parse_price_series("XRP/USD=740@2, BTC/USD=65000@0 EUR/USD").unwrap();
        assert_eq!(
            series
                .iter()
                .map(|p| (p.base_asset.as_str(), p.asset_price.as_deref(), p.scale))
                .collect::<Vec<_>>(),
            [
                ("XRP", Some("2E4"), Some(2)),
                ("BTC", Some("FDE8"), Some(0)),
                ("EUR", None, None)
            ]
        );
        assert_eq!(
            parse_price_series("XRP/USD=18446744073709551615@20").unwrap()[0]
                .asset_price
                .as_deref(),
            Some("FFFFFFFFFFFFFFFF")
        );
        for (bad, needle) in [
            ("XRP/USD=1@21", "scale"),
            ("XRP/USD=1", "price@scale"),
            ("XRP/USD=-1@0", "price"),
            ("XRP/USD=18446744073709551616@0", "price"),
            ("XRP/XRP=1@0", "differ"),
            ("XRP/USD=1@0,XRP/USD", "twice"),
            ("XRPX/USD=1@0", "3-character"),
            ("XRPUSD=1@0", "BASE/QUOTE"),
            ("", "1..=10"),
        ] {
            let err = parse_price_series(bad).unwrap_err();
            assert!(err.contains(needle), "{bad:?}: {err}");
        }
        let creds = parse_credentials(&format!("{B}:KYC,{C}:KYC")).unwrap();
        assert_eq!(
            (creds[0].issuer.as_str(), creds[0].credential_type.as_str()),
            (B, "4B5943")
        );
        for (bad, needle) in [
            (format!("{B}:KYC,{B}:KYC"), "twice"),
            (B.to_string(), "rIssuer:CredentialType"),
            (format!("{B}:"), "CredentialType"),
            (format!("{B}:{}", "x".repeat(65)), "CredentialType"),
            ("rBad:KYC".to_string(), "rBad"),
        ] {
            let err = parse_credentials(&bad).unwrap_err();
            assert!(err.contains(needle), "{bad:?}: {err}");
        }
    }

    fn samples() -> Vec<(&'static str, Fields)> {
        vec![
            (
                "CredentialCreate",
                f(&[
                    ("Subject", B),
                    ("CredentialType", "KYC"),
                    ("Expiration", "800000000"),
                    ("URI", "https://x"),
                ]),
            ),
            (
                "CredentialAccept",
                f(&[("Issuer", B), ("CredentialType", "KYC")]),
            ),
            (
                "CredentialDelete",
                f(&[("Subject", B), ("CredentialType", "KYC")]),
            ),
            (
                "DIDSet",
                f(&[("Data", "att"), ("DIDDocument", "doc"), ("URI", "did:x")]),
            ),
            ("DIDDelete", f(&[])),
            (
                "OracleSet",
                f(&[
                    ("OracleDocumentID", "34"),
                    ("LastUpdateTime", "1700000000"),
                    ("PriceDataSeries", "XRP/USD=740@2"),
                    ("Provider", "chainlink"),
                    ("AssetClass", "currency"),
                    ("URI", "https://o"),
                ]),
            ),
            ("OracleDelete", f(&[("OracleDocumentID", "34")])),
            (
                "PermissionedDomainSet",
                f(&[
                    ("AcceptedCredentials", &format!("{B}:KYC,{C}:AML")),
                    ("DomainID", &DOMAIN.to_lowercase()),
                ]),
            ),
            (
                "PermissionedDomainDelete",
                f(&[("DomainID", &DOMAIN.to_lowercase())]),
            ),
        ]
    }

    fn hx(s: &str) -> String {
        hex::encode_upper(s.as_bytes())
    }

    /// TC-302: unsigned `tx_json` per type — type, account, `Sequence`, no `Fee` /
    /// `LastLedgerSequence`, text fields hex-encoded, price series in PriceData form.
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
                "CredentialCreate" => {
                    assert_eq!(j["Subject"], B);
                    assert_eq!(j["CredentialType"], hx("KYC"));
                    assert_eq!(j["Expiration"], 800_000_000);
                    assert_eq!(j["URI"], hx("https://x"));
                }
                "CredentialAccept" => {
                    assert_eq!(
                        (&j["Issuer"], &j["CredentialType"]),
                        (&json!(B), &json!(hx("KYC")))
                    );
                }
                "CredentialDelete" => {
                    assert_eq!(j["Subject"], B);
                    assert!(j.get("Issuer").is_none());
                    assert_eq!(j["CredentialType"], hx("KYC"));
                }
                "DIDSet" => {
                    assert_eq!(
                        (&j["Data"], &j["DIDDocument"], &j["URI"]),
                        (&json!(hx("att")), &json!(hx("doc")), &json!(hx("did:x")))
                    );
                }
                "DIDDelete" => {}
                "OracleSet" => {
                    assert_eq!(
                        (&j["OracleDocumentID"], &j["LastUpdateTime"]),
                        (&json!(34), &json!(1_700_000_000))
                    );
                    assert_eq!(
                        (&j["Provider"], &j["AssetClass"], &j["URI"]),
                        (
                            &json!(hx("chainlink")),
                            &json!(hx("currency")),
                            &json!(hx("https://o"))
                        )
                    );
                    assert_eq!(
                        j["PriceDataSeries"],
                        json!([{"PriceData": {
                            "BaseAsset": "XRP", "QuoteAsset": "USD",
                            "AssetPrice": "2E4", "Scale": 2
                        }}])
                    );
                }
                "OracleDelete" => assert_eq!(j["OracleDocumentID"], 34),
                "PermissionedDomainSet" => {
                    assert_eq!(j["DomainID"], DOMAIN);
                    assert_eq!(
                        j["AcceptedCredentials"],
                        json!([
                            {"Credential": {"Issuer": B, "CredentialType": hx("KYC")}},
                            {"Credential": {"Issuer": C, "CredentialType": hx("AML")}}
                        ])
                    );
                }
                "PermissionedDomainDelete" => assert_eq!(j["DomainID"], DOMAIN),
                other => panic!("sample for {other} is not asserted"),
            }
        }
    }

    /// TC-304: signed blobs decode to the same fields with the simulate-chosen Sequence / Fee /
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
            match tx_type {
                "CredentialCreate" => {
                    assert_eq!(
                        (&d["Subject"], &d["Expiration"]),
                        (&json!(B), &json!(800_000_000))
                    );
                    assert_eq!(d["CredentialType"], hx("KYC"));
                    assert_eq!(d["URI"], hx("https://x"));
                }
                "CredentialAccept" => assert_eq!(d["Issuer"], B),
                "CredentialDelete" => assert_eq!(d["Subject"], B),
                "DIDSet" => {
                    assert_eq!(d["Data"], hx("att"));
                    assert_eq!(d["DIDDocument"], hx("doc"));
                    assert_eq!(d["URI"], hx("did:x"));
                }
                "DIDDelete" => {}
                "OracleSet" => {
                    assert_eq!(d["OracleDocumentID"], 34);
                    assert_eq!(d["LastUpdateTime"], 1_700_000_000);
                    assert_eq!(d["Provider"], hx("chainlink"));
                    let p = &d["PriceDataSeries"][0]["PriceData"];
                    assert_eq!(p["BaseAsset"], "XRP");
                    assert_eq!(p["Scale"], 2);
                    assert_eq!(
                        u64::from_str_radix(p["AssetPrice"].as_str().expect("price"), 16).unwrap(),
                        740
                    );
                }
                "OracleDelete" => assert_eq!(d["OracleDocumentID"], 34),
                "PermissionedDomainSet" => {
                    assert_eq!(d["DomainID"], DOMAIN);
                    assert_eq!(d["AcceptedCredentials"][1]["Credential"]["Issuer"], C);
                }
                "PermissionedDomainDelete" => assert_eq!(d["DomainID"], DOMAIN),
                other => panic!("sample for {other} is not asserted"),
            }
        }
    }

    /// TC-306: typed confirm phrases — `DELETE` on every irreversible identity type (credential,
    /// DID, oracle, domain deletes), none on create / set / accept.
    #[test]
    fn confirm_phrases() {
        for s in SPECS {
            let want = s.tx_type.ends_with("Delete").then_some("DELETE");
            assert_eq!(s.confirm.map(|c| c.phrase), want, "{}", s.tx_type);
        }
    }

    /// TC-307: the crate's own model rules run when the unsigned JSON is built, before any RPC
    /// (an oracle price entry without a scale cannot be expressed; a zero-length series is refused).
    #[test]
    fn builder_applies_model_rules() {
        let account = wallet().classic_address.clone();
        let ok = spec("OracleSet")
            .prepare(
                &f(&[
                    ("OracleDocumentID", "1"),
                    ("LastUpdateTime", "1"),
                    ("PriceDataSeries", "XRP/USD=1@0"),
                ]),
                0,
            )
            .unwrap();
        assert!((ORACLE_SET.build_json)(&account, &ok, 1).is_ok());
        let mut missing = ok.clone();
        missing.remove("PriceDataSeries");
        assert!((ORACLE_SET.build_json)(&account, &missing, 1).is_err());
    }

    /// TC-309: the generic submit path carries this family end to end against a local RPC mock —
    /// `server_info → account_info → simulate → submit` for DIDSet (a refused empty form never
    /// reaches the RPC); simulate and the signed blob carry the hex-encoded fields and the
    /// simulate-chosen Sequence / Fee / LastLedgerSequence.
    #[tokio::test]
    async fn generic_submit_did_set_against_local_rpc() {
        use crate::action::Action;
        use crate::network::Network;
        use crate::test_support::rpc_mock;
        use crate::xrpl::client::RpcClient;
        use crate::xrpl::submit::submit_generic_transaction;
        use crate::xrpl::types::GenericTxSubmitParams;
        use tokio::net::TcpListener;
        use tokio::sync::mpsc;

        fn result_for(method: &str) -> Value {
            match method {
                "server_info" => json!({"info": {"network_id": 1, "validated_ledger": {"seq": 1}}}),
                "account_info" => json!({"account_data": {
                    "Account": "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh", "Balance": "10000000",
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
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let server = tokio::spawn(rpc_mock::serve(listener, 4, result_for));
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
        let params = |fields: &[(&str, &str)]| GenericTxSubmitParams {
            tx_type: "DIDSet".into(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            skip_mainnet_prompt: false,
            network: Network::Testnet,
        };

        submit_generic_transaction(&rpc, &Network::Testnet, params(&[]), &tx, Some(&credential))
            .await;
        assert!(matches!(
            rx.try_recv(),
            Ok(Action::GenericTxSubmitErr(e)) if e.contains("at least one")
        ));

        submit_generic_transaction(
            &rpc,
            &Network::Testnet,
            params(&[("URI", "did:example:1")]),
            &tx,
            Some(&credential),
        )
        .await;
        assert!(matches!(
            rx.try_recv(),
            Ok(Action::GenericTxSubmitOk(s)) if s.starts_with("DIDSet · ")
        ));

        let requests = server.await.expect("join").expect("serve");
        assert_eq!(
            rpc_mock::methods(&requests),
            ["server_info", "account_info", "simulate", "submit"]
        );
        let sim = &requests[2]["params"][0]["tx_json"];
        assert_eq!(
            (&sim["TransactionType"], &sim["URI"], &sim["Sequence"]),
            (&json!("DIDSet"), &json!(hx("did:example:1")), &json!(7))
        );
        let blob = requests[3]["params"][0]["tx_blob"].as_str().expect("blob");
        let signed = decode(blob).expect("decode");
        assert_eq!(
            (
                &signed["URI"],
                &signed["Sequence"],
                &signed["Fee"],
                &signed["LastLedgerSequence"]
            ),
            (
                &json!(hx("did:example:1")),
                &json!(7),
                &json!("15"),
                &json!(80_000_020)
            )
        );
    }
}
