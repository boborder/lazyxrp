//! Multi-purpose token family: MPTokenIssuanceCreate, MPTokenIssuanceDestroy, MPTokenIssuanceSet,
//! MPTokenAuthorize. Helpers are shared with `identity.rs`.
//!
//! Deliberately omitted (modelled by xrpl-rust 1.3.0, outside the baseline MPT transactions):
//! `tfMPTCanHoldConfidentialBalance`, `ImmutableFlags`, the `MPTokenIssuanceSet` mutation flags
//! (`tfMPTSet*`), `TransferFee` / `MPTokenMetadata` mutation and the confidential-MPT encryption
//! keys. `MPTokenMetadata` is plain text, hex-encoded here.

use color_eyre::eyre::eyre;
use serde_json::Value;
use xrpl::models::Model;
use xrpl::models::transactions::mptoken_authorize::{MPTokenAuthorize, MPTokenAuthorizeFlag};
use xrpl::models::transactions::mptoken_issuance_create::{
    MPTokenIssuanceCreate, MPTokenIssuanceCreateFlag,
};
use xrpl::models::transactions::mptoken_issuance_destroy::MPTokenIssuanceDestroy;
use xrpl::models::transactions::mptoken_issuance_set::{
    MPTokenIssuanceSet, MPTokenIssuanceSetFlag,
};

use super::identity::{
    address, eyre_model, no_check, non_zero_id, text_hex, to_json, tx_fns, uint, upper,
};
use super::{Confirm, FieldKind, FieldSpec, Fields, TxSpec, common_fields, flag_bits, val};
use crate::signing::sign_blob;

const FAMILY: &str = "MPT";
/// `MPTokenIssuanceID`: 192-bit (sequence + issuer) id.
const ISSUANCE_ID_LEN: usize = 48;
/// rippled `maxTransferFee`.
const MAX_TRANSFER_FEE: u32 = 50_000;
/// rippled `maxMPTokenMetadataLength`.
const MAX_METADATA_BYTES: usize = 1024;
const MAX_ASSET_SCALE: u32 = 255;

const CREATE_FLAGS: &[(&str, u32)] = &[
    ("canLock", 0x2),
    ("requireAuth", 0x4),
    ("canEscrow", 0x8),
    ("canTrade", 0x10),
    ("canTransfer", 0x20),
    ("canClawback", 0x40),
];
const CAN_TRANSFER: u32 = 0x20;
const REQUIRE_AUTH: u32 = 0x4;
const SET_FLAGS: &[(&str, u32)] = &[("lock", 0x1), ("unlock", 0x2)];
const AUTHORIZE_FLAGS: &[(&str, u32)] = &[("unauthorize", 0x1)];

pub static SPECS: &[TxSpec] = &[CREATE, DESTROY, SET, AUTHORIZE];

const ISSUANCE_ID: FieldSpec = FieldSpec {
    name: "MPTokenIssuanceID",
    kind: FieldKind::Hex(ISSUANCE_ID_LEN),
    required: true,
    help: "48 hex characters",
    default: "",
};

const HOLDER: FieldSpec = FieldSpec {
    name: "Holder",
    kind: FieldKind::Address,
    required: false,
    help: "the token holder's account",
    default: "",
};

fn issuance_id(f: &Fields) -> color_eyre::Result<String> {
    upper(f, "MPTokenIssuanceID").ok_or_else(|| eyre!("MPTokenIssuanceID: required"))
}

// ----------------------------------------------------------- MPTokenIssuanceCreate

pub const CREATE: TxSpec = TxSpec {
    tx_type: "MPTokenIssuanceCreate",
    family: FAMILY,
    summary: "create a multi-purpose token issuance",
    fields: &[
        FieldSpec {
            name: "AssetScale",
            kind: FieldKind::Uint(MAX_ASSET_SCALE),
            required: false,
            help: "decimal places, 0..255 (display hint; default 0)",
            default: "",
        },
        FieldSpec {
            name: "MaximumAmount",
            kind: FieldKind::Custom(maximum_amount_ok),
            required: false,
            help: "supply cap in base units, 1..9223372036854775807 (default: the maximum)",
            default: "",
        },
        FieldSpec {
            name: "TransferFee",
            kind: FieldKind::Uint(MAX_TRANSFER_FEE),
            required: false,
            help: "0..50000 (1/1000 %); needs canTransfer",
            default: "",
        },
        FieldSpec {
            name: "MPTokenMetadata",
            kind: FieldKind::Text(MAX_METADATA_BYTES),
            required: false,
            help: "plain text, stored hex-encoded (≤1024 bytes)",
            default: "",
        },
        FieldSpec {
            name: "Flags",
            kind: FieldKind::Flags(CREATE_FLAGS),
            required: false,
            help: "canLock, requireAuth, canEscrow, canTrade, canTransfer, canClawback (comma-separated)",
            default: "",
        },
        FieldSpec {
            name: "DomainID",
            kind: FieldKind::Custom(non_zero_id),
            required: false,
            help: "permissioned domain (64 hex); needs requireAuth",
            default: "",
        },
    ],
    confirm: None,
    check: create_check,
    build_json: create_json,
    sign: create_sign,
};

fn maximum_amount_ok(v: &str) -> Result<(), String> {
    match v.parse::<u64>() {
        Ok(n) if v.bytes().all(|b| b.is_ascii_digit()) && (1..=i64::MAX as u64).contains(&n) => {
            Ok(())
        }
        _ => Err(format!("expected an integer 1..={}", i64::MAX)),
    }
}

fn create_check(f: &Fields) -> Result<(), String> {
    let bits = val(f, "Flags").map_or(0, |v| flag_bits(CREATE_FLAGS, v));
    if val(f, "TransferFee").is_some_and(|v| v != "0") && bits & CAN_TRANSFER == 0 {
        return Err("TransferFee: a non-zero fee needs the canTransfer flag".into());
    }
    if val(f, "DomainID").is_some() && bits & REQUIRE_AUTH == 0 {
        return Err("DomainID: needs the requireAuth flag".into());
    }
    Ok(())
}

fn create_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<MPTokenIssuanceCreate<'static>> {
    let bits = val(f, "Flags").map_or(0, |v| flag_bits(CREATE_FLAGS, v));
    Ok(MPTokenIssuanceCreate {
        common_fields: common_fields!(account, MPTokenIssuanceCreate, sequence, fee_lls)
            .with_flags(MPTokenIssuanceCreateFlag::from_bits(bits)),
        asset_scale: uint(f, "AssetScale")?
            .map(u8::try_from)
            .transpose()
            .map_err(|e| eyre!("AssetScale: {e}"))?,
        maximum_amount: val(f, "MaximumAmount").map(|v| v.to_string().into()),
        transfer_fee: uint(f, "TransferFee")?
            .map(u16::try_from)
            .transpose()
            .map_err(|e| eyre!("TransferFee: {e}"))?,
        mptoken_metadata: text_hex(f, "MPTokenMetadata").map(Into::into),
        domain_id: upper(f, "DomainID").map(Into::into),
        immutable_flags: None,
    })
}
tx_fns!(create_json, create_sign, create_tx, "MPTokenIssuanceCreate");

// ---------------------------------------------------------- MPTokenIssuanceDestroy

pub const DESTROY: TxSpec = TxSpec {
    tx_type: "MPTokenIssuanceDestroy",
    family: FAMILY,
    summary: "destroy an issuance ⚠ irreversible",
    fields: &[ISSUANCE_ID],
    confirm: Some(Confirm {
        phrase: "DESTROY",
        hint: "Irreversible: the issuance is removed (needs zero outstanding supply)",
    }),
    check: no_check,
    build_json: destroy_json,
    sign: destroy_sign,
};

fn destroy_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<MPTokenIssuanceDestroy<'static>> {
    Ok(MPTokenIssuanceDestroy {
        common_fields: common_fields!(account, MPTokenIssuanceDestroy, sequence, fee_lls),
        mptoken_issuance_id: issuance_id(f)?.into(),
    })
}
tx_fns!(
    destroy_json,
    destroy_sign,
    destroy_tx,
    "MPTokenIssuanceDestroy"
);

// ------------------------------------------------------------- MPTokenIssuanceSet

pub const SET: TxSpec = TxSpec {
    tx_type: "MPTokenIssuanceSet",
    family: FAMILY,
    summary: "lock / unlock an issuance or one holder",
    fields: &[
        ISSUANCE_ID,
        FieldSpec {
            help: "lock / unlock only this holder (omit: the whole issuance)",
            ..HOLDER
        },
        FieldSpec {
            name: "Flags",
            kind: FieldKind::Flags(SET_FLAGS),
            required: true,
            help: "lock or unlock (needs the issuance's canLock)",
            default: "lock",
        },
    ],
    confirm: None,
    check: set_check,
    build_json: set_json,
    sign: set_sign,
};

fn set_check(f: &Fields) -> Result<(), String> {
    match val(f, "Flags").map_or(0, |v| flag_bits(SET_FLAGS, v)) {
        0b01 | 0b10 => Ok(()),
        0 => Err("Flags: choose lock or unlock".into()),
        _ => Err("Flags: lock and unlock are mutually exclusive".into()),
    }
}

fn set_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<MPTokenIssuanceSet<'static>> {
    let bits = val(f, "Flags").map_or(0, |v| flag_bits(SET_FLAGS, v));
    Ok(MPTokenIssuanceSet {
        common_fields: common_fields!(account, MPTokenIssuanceSet, sequence, fee_lls)
            .with_flags(MPTokenIssuanceSetFlag::from_bits(bits)),
        mptoken_issuance_id: issuance_id(f)?.into(),
        holder: address(f, "Holder").map(Into::into),
        issuer_encryption_key: None,
        auditor_encryption_key: None,
        domain_id: None,
        mptoken_metadata: None,
        transfer_fee: None,
        immutable_flags: None,
    })
}
tx_fns!(set_json, set_sign, set_tx, "MPTokenIssuanceSet");

// ----------------------------------------------------------------- MPTokenAuthorize

pub const AUTHORIZE: TxSpec = TxSpec {
    tx_type: "MPTokenAuthorize",
    family: FAMILY,
    summary: "opt in to a token / authorize a holder",
    fields: &[
        ISSUANCE_ID,
        FieldSpec {
            help: "issuer only: the holder to (un)authorize; holders omit it",
            ..HOLDER
        },
        FieldSpec {
            name: "Flags",
            kind: FieldKind::Flags(AUTHORIZE_FLAGS),
            required: false,
            help: "unauthorize: holder opts out / issuer revokes authorization",
            default: "",
        },
    ],
    confirm: None,
    check: no_check,
    build_json: authorize_json,
    sign: authorize_sign,
};

fn authorize_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<MPTokenAuthorize<'static>> {
    let bits = val(f, "Flags").map_or(0, |v| flag_bits(AUTHORIZE_FLAGS, v));
    Ok(MPTokenAuthorize {
        common_fields: common_fields!(account, MPTokenAuthorize, sequence, fee_lls)
            .with_flags(MPTokenAuthorizeFlag::from_bits(bits)),
        mptoken_issuance_id: issuance_id(f)?.into(),
        holder: address(f, "Holder").map(Into::into),
    })
}
tx_fns!(
    authorize_json,
    authorize_sign,
    authorize_tx,
    "MPTokenAuthorize"
);

#[cfg(test)]
mod tests {
    use serde_json::json;
    use xrpl::core::binarycodec::decode;

    use super::super::identity::tests::{B, f, run_cases, spec, wallet};
    use super::*;

    const ID: &str = "0000012FFD9EE5DA93AC614B4DB94D7E0FCE415CA51BED47";
    const DOMAIN: &str = "5E9C7E5C1A3F4B2D8A6C0E9F1B3D5A7C9E1F3B5D7A9C1E3F5B7D9A1C3E5F7B9D";

    /// TC-301: per-type field validation — TransferFee 50000 / 50001, AssetScale 255 / 256,
    /// MaximumAmount 0 / 1 / i64::MAX / i64::MAX + 1, metadata 1024 / 1025 bytes, id lengths
    /// 47 / 48 / 49, transfer-fee / domain / lock flag combinations.
    #[test]
    fn field_validation_tables() {
        let max = i64::MAX.to_string();
        let over = (i64::MAX as u64 + 1).to_string();
        let cases: Vec<(&str, _, Option<&str>)> = vec![
            ("MPTokenIssuanceCreate", f(&[]), None),
            (
                "MPTokenIssuanceCreate",
                f(&[
                    ("AssetScale", "255"),
                    ("MaximumAmount", &max),
                    ("TransferFee", "50000"),
                    (
                        "Flags",
                        "canTransfer,tfCanLock,requireAuth,canEscrow,canTrade,canClawback",
                    ),
                    ("MPTokenMetadata", &"m".repeat(1024)),
                    ("DomainID", DOMAIN),
                ]),
                None,
            ),
            (
                "MPTokenIssuanceCreate",
                f(&[("AssetScale", "256")]),
                Some("AssetScale"),
            ),
            (
                "MPTokenIssuanceCreate",
                f(&[("MaximumAmount", "0")]),
                Some("MaximumAmount"),
            ),
            ("MPTokenIssuanceCreate", f(&[("MaximumAmount", "1")]), None),
            (
                "MPTokenIssuanceCreate",
                f(&[("MaximumAmount", &over)]),
                Some("MaximumAmount"),
            ),
            (
                "MPTokenIssuanceCreate",
                f(&[("MaximumAmount", "-1")]),
                Some("MaximumAmount"),
            ),
            (
                "MPTokenIssuanceCreate",
                f(&[("MaximumAmount", "1.5")]),
                Some("MaximumAmount"),
            ),
            (
                "MPTokenIssuanceCreate",
                f(&[("TransferFee", "50001"), ("Flags", "canTransfer")]),
                Some("TransferFee"),
            ),
            (
                "MPTokenIssuanceCreate",
                f(&[("TransferFee", "1")]),
                Some("canTransfer"),
            ),
            ("MPTokenIssuanceCreate", f(&[("TransferFee", "0")]), None),
            (
                "MPTokenIssuanceCreate",
                f(&[("MPTokenMetadata", &"m".repeat(1025))]),
                Some("MPTokenMetadata"),
            ),
            (
                "MPTokenIssuanceCreate",
                f(&[("DomainID", DOMAIN)]),
                Some("requireAuth"),
            ),
            (
                "MPTokenIssuanceCreate",
                f(&[("DomainID", DOMAIN), ("Flags", "requireAuth")]),
                None,
            ),
            (
                "MPTokenIssuanceCreate",
                f(&[("DomainID", &"0".repeat(64)), ("Flags", "requireAuth")]),
                Some("zero hash"),
            ),
            (
                "MPTokenIssuanceCreate",
                f(&[("Flags", "burnable")]),
                Some("unknown flag"),
            ),
            (
                "MPTokenIssuanceDestroy",
                f(&[("MPTokenIssuanceID", ID)]),
                None,
            ),
            (
                "MPTokenIssuanceDestroy",
                f(&[("MPTokenIssuanceID", &ID[..47])]),
                Some("48 hex"),
            ),
            (
                "MPTokenIssuanceDestroy",
                f(&[("MPTokenIssuanceID", &format!("{ID}0"))]),
                Some("48 hex"),
            ),
            ("MPTokenIssuanceDestroy", f(&[]), Some("required")),
            (
                "MPTokenIssuanceSet",
                f(&[("MPTokenIssuanceID", ID), ("Flags", "lock")]),
                None,
            ),
            (
                "MPTokenIssuanceSet",
                f(&[
                    ("MPTokenIssuanceID", ID),
                    ("Flags", "unlock"),
                    ("Holder", B),
                ]),
                None,
            ),
            (
                "MPTokenIssuanceSet",
                f(&[("MPTokenIssuanceID", ID), ("Flags", "lock,unlock")]),
                Some("mutually exclusive"),
            ),
            (
                "MPTokenIssuanceSet",
                f(&[("MPTokenIssuanceID", ID)]),
                Some("Flags: required"),
            ),
            (
                "MPTokenIssuanceSet",
                f(&[
                    ("MPTokenIssuanceID", ID),
                    ("Flags", "lock"),
                    ("Holder", "rBad"),
                ]),
                Some("Holder"),
            ),
            ("MPTokenAuthorize", f(&[("MPTokenIssuanceID", ID)]), None),
            (
                "MPTokenAuthorize",
                f(&[
                    ("MPTokenIssuanceID", ID),
                    ("Holder", B),
                    ("Flags", "unauthorize"),
                ]),
                None,
            ),
            (
                "MPTokenAuthorize",
                f(&[("MPTokenIssuanceID", ID), ("Flags", "authorize")]),
                Some("unknown flag"),
            ),
            (
                "MPTokenAuthorize",
                f(&[("Holder", B)]),
                Some("MPTokenIssuanceID"),
            ),
        ];
        run_cases(cases);
    }

    fn samples() -> Vec<(&'static str, Fields)> {
        vec![
            (
                "MPTokenIssuanceCreate",
                f(&[
                    ("AssetScale", "2"),
                    ("MaximumAmount", "1000000"),
                    ("TransferFee", "314"),
                    ("MPTokenMetadata", "gold"),
                    ("Flags", "canTransfer,canLock,requireAuth"),
                    ("DomainID", DOMAIN),
                ]),
            ),
            (
                "MPTokenIssuanceDestroy",
                f(&[("MPTokenIssuanceID", &ID.to_lowercase())]),
            ),
            (
                "MPTokenIssuanceSet",
                f(&[("MPTokenIssuanceID", ID), ("Holder", B), ("Flags", "lock")]),
            ),
            (
                "MPTokenAuthorize",
                f(&[
                    ("MPTokenIssuanceID", ID),
                    ("Holder", B),
                    ("Flags", "unauthorize"),
                ]),
            ),
        ]
    }

    /// TC-303: unsigned `tx_json` per type — flag bits, metadata hex-encoded from text, ids
    /// upper-cased, no `Fee` / `LastLedgerSequence`, no stray optional fields.
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
            assert!(j.get("Fee").is_none() && j.get("LastLedgerSequence").is_none());
            match tx_type {
                "MPTokenIssuanceCreate" => {
                    assert_eq!(
                        (&j["AssetScale"], &j["MaximumAmount"], &j["TransferFee"]),
                        (&json!(2), &json!("1000000"), &json!(314))
                    );
                    assert_eq!(j["Flags"], 0x20 | 0x2 | 0x4);
                    assert_eq!(j["MPTokenMetadata"], hex::encode_upper("gold"));
                    assert_eq!(j["DomainID"], DOMAIN);
                    assert!(j.get("ImmutableFlags").is_none());
                }
                "MPTokenIssuanceDestroy" => assert_eq!(j["MPTokenIssuanceID"], ID),
                "MPTokenIssuanceSet" => {
                    assert_eq!(
                        (&j["MPTokenIssuanceID"], &j["Holder"]),
                        (&json!(ID), &json!(B))
                    );
                    assert_eq!(j["Flags"], 1);
                    assert!(j.get("TransferFee").is_none() && j.get("DomainID").is_none());
                }
                "MPTokenAuthorize" => {
                    assert_eq!(
                        (&j["MPTokenIssuanceID"], &j["Holder"]),
                        (&json!(ID), &json!(B))
                    );
                    assert_eq!(j["Flags"], 1);
                }
                other => panic!("sample for {other} is not asserted"),
            }
        }
    }

    /// TC-305: signed blobs decode to the same fields with distinct Sequence / Fee /
    /// LastLedgerSequence and a signature.
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
            assert_eq!(d["Sequence"], sequence, "{tx_type}");
            assert_eq!(d["Fee"], fee.to_string(), "{tx_type}");
            assert_eq!(d["LastLedgerSequence"], lls, "{tx_type}");
            assert!(d["TxnSignature"].as_str().is_some_and(|s| !s.is_empty()));
            match tx_type {
                "MPTokenIssuanceCreate" => {
                    assert_eq!(
                        (&d["AssetScale"], &d["TransferFee"]),
                        (&json!(2), &json!(314))
                    );
                    assert_eq!(d["MaximumAmount"], "1000000");
                    assert_eq!(d["Flags"], 0x26);
                    assert_eq!(d["MPTokenMetadata"], hex::encode_upper("gold"));
                    assert_eq!(d["DomainID"], DOMAIN);
                }
                "MPTokenIssuanceDestroy" => assert_eq!(d["MPTokenIssuanceID"], ID),
                "MPTokenIssuanceSet" => {
                    assert_eq!(
                        (&d["MPTokenIssuanceID"], &d["Holder"]),
                        (&json!(ID), &json!(B))
                    );
                    assert_eq!(d["Flags"], 1);
                }
                "MPTokenAuthorize" => {
                    assert_eq!(
                        (&d["MPTokenIssuanceID"], &d["Holder"]),
                        (&json!(ID), &json!(B))
                    );
                    assert_eq!(d["Flags"], 1);
                }
                other => panic!("sample for {other} is not asserted"),
            }
        }
    }

    /// TC-306: typed confirm — `DESTROY` on MPTokenIssuanceDestroy only.
    #[test]
    fn confirm_phrases() {
        for s in SPECS {
            let want = (s.tx_type == "MPTokenIssuanceDestroy").then_some("DESTROY");
            assert_eq!(s.confirm.map(|c| c.phrase), want, "{}", s.tx_type);
        }
    }

    /// TC-307: the crate's model rules run when the unsigned JSON is built — a Holder equal to
    /// the signer is refused for Set and Authorize.
    #[test]
    fn builder_applies_model_rules() {
        let account = wallet().classic_address.clone();
        let own = |t: &str, extra: &[(&str, &str)]| {
            let mut pairs = vec![("MPTokenIssuanceID", ID), ("Holder", account.as_str())];
            pairs.extend_from_slice(extra);
            spec(t).prepare(&f(&pairs), 0).unwrap()
        };
        assert!(
            (SET.build_json)(
                &account,
                &own("MPTokenIssuanceSet", &[("Flags", "lock")]),
                1
            )
            .is_err()
        );
        assert!((AUTHORIZE.build_json)(&account, &own("MPTokenAuthorize", &[]), 1).is_err());
    }

    /// TC-310: the generic submit path carries this family end to end against a local RPC mock —
    /// MPTokenIssuanceCreate: a refused form (transfer fee without canTransfer) never reaches the
    /// RPC; simulate and the signed blob carry flags / metadata and the simulate-chosen
    /// Sequence / Fee / LastLedgerSequence.
    #[tokio::test]
    async fn generic_submit_mpt_create_against_local_rpc() {
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
            tx_type: "MPTokenIssuanceCreate".into(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            skip_mainnet_prompt: false,
        };

        submit_generic_transaction(
            &rpc,
            &Network::Testnet,
            params(&[("TransferFee", "5")]),
            &tx,
            Some(&credential),
        )
        .await;
        assert!(matches!(
            rx.try_recv(),
            Ok(Action::GenericTxSubmitErr(e)) if e.contains("canTransfer")
        ));

        submit_generic_transaction(
            &rpc,
            &Network::Testnet,
            params(&[
                ("Flags", "canTransfer,canLock"),
                ("TransferFee", "5"),
                ("MPTokenMetadata", "hi"),
            ]),
            &tx,
            Some(&credential),
        )
        .await;
        assert!(matches!(
            rx.try_recv(),
            Ok(Action::GenericTxSubmitOk(s)) if s.starts_with("MPTokenIssuanceCreate · ")
        ));

        let requests = server.await.expect("join").expect("serve");
        assert_eq!(
            rpc_mock::methods(&requests),
            ["server_info", "account_info", "simulate", "submit"]
        );
        let sim = &requests[2]["params"][0]["tx_json"];
        assert_eq!(
            (
                &sim["Flags"],
                &sim["TransferFee"],
                &sim["MPTokenMetadata"],
                &sim["Sequence"]
            ),
            (&json!(0x22), &json!(5), &json!("6869"), &json!(7))
        );
        let blob = requests[3]["params"][0]["tx_blob"].as_str().expect("blob");
        let signed = decode(blob).expect("decode");
        assert_eq!(
            (
                &signed["Flags"],
                &signed["MPTokenMetadata"],
                &signed["Sequence"],
                &signed["Fee"],
                &signed["LastLedgerSequence"]
            ),
            (
                &json!(0x22),
                &json!("6869"),
                &json!(7),
                &json!("15"),
                &json!(80_000_020)
            )
        );
    }
}
