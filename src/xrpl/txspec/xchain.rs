//! Cross-chain bridge family (XLS-38d): XChainCreateBridge, XChainModifyBridge,
//! XChainCreateClaimID, XChainCommit, XChainClaim, XChainAccountCreateCommit.
//!
//! The `XChainBridge` object is one flat field, `lockDoor,lockIssue,issueDoor,issueIssue`
//! (LockingChainDoor, LockingChainIssue, IssuingChainDoor, IssuingChainIssue), where an issue is
//! `XRP` or `CUR:issuer`. XRP-only amounts (rewards, account-create minimum) are plain drops.
//!
//! Not supported: XChainAddClaimAttestation / XChainAddAccountCreateAttestation (witness-server
//! signatures cannot be authored in a form; see `coverage.rs`).

use color_eyre::eyre::eyre;
use serde_json::Value;
use xrpl::models::transactions::xchain_account_create_commit::XChainAccountCreateCommit;
use xrpl::models::transactions::xchain_claim::XChainClaim;
use xrpl::models::transactions::xchain_commit::XChainCommit;
use xrpl::models::transactions::xchain_create_bridge::XChainCreateBridge;
use xrpl::models::transactions::xchain_create_claim_id::XChainCreateClaimID;
use xrpl::models::transactions::xchain_modify_bridge::{
    XChainModifyBridge, XChainModifyBridgeFlags,
};
use xrpl::models::{Amount, Currency, IssuedCurrency, Model, XChainBridge, XRP, XRPAmount};

use super::{
    Confirm, FieldKind, FieldSpec, Fields, TxSpec, common_fields, flag_bits, parse_drops, val,
};
use crate::signing::{parse_offer_amount, sign_blob};

const FAMILY: &str = "XChain";
const MODIFY_FLAGS: &[(&str, u32)] = &[("ClearAccountCreateAmount", 0x0001_0000)];

pub static SPECS: &[TxSpec] = &[
    CREATE_BRIDGE,
    MODIFY_BRIDGE,
    CREATE_CLAIM_ID,
    COMMIT,
    CLAIM,
    ACCOUNT_CREATE_COMMIT,
];

// ------------------------------------------------------------------ bridge grammar

#[derive(Debug, Clone, PartialEq, Eq)]
enum Issue {
    Xrp,
    Iou { currency: String, issuer: String },
}

struct Bridge {
    lock_door: String,
    lock_issue: Issue,
    issue_door: String,
    issue_issue: Issue,
}

impl Bridge {
    fn is_xrp(&self) -> bool {
        self.lock_issue == Issue::Xrp
    }

    fn has_issue(&self, issue: &Issue) -> bool {
        &self.lock_issue == issue || &self.issue_issue == issue
    }
}

impl From<&Issue> for Currency<'static> {
    fn from(i: &Issue) -> Self {
        match i {
            Issue::Xrp => XRP::new().into(),
            Issue::Iou { currency, issuer } => {
                IssuedCurrency::new(currency.clone().into(), issuer.clone().into()).into()
            }
        }
    }
}

fn parse_issue(s: &str) -> Result<Issue, String> {
    if s == "XRP" {
        return Ok(Issue::Xrp);
    }
    let (currency, issuer) = s
        .split_once(':')
        .ok_or_else(|| format!("issue {s:?}: expected XRP or CUR:issuer"))?;
    if issuer.contains(':') {
        return Err(format!("issue {s:?}: expected XRP or CUR:issuer"));
    }
    // Reuse the Amount grammar for currency-code and issuer-checksum rules.
    parse_offer_amount(&format!("{currency}:{issuer}:1"))
        .map_err(|e| format!("issue {s:?}: {e}"))?;
    Ok(Issue::Iou {
        currency: currency.into(),
        issuer: issuer.into(),
    })
}

fn parse_door(s: &str, what: &str) -> Result<String, String> {
    if xrpl::core::addresscodec::is_valid_classic_address(s) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "{what} {s:?}: expected a valid classic address (r…)"
        ))
    }
}

fn parse_bridge(v: &str) -> Result<Bridge, String> {
    let parts: Vec<&str> = v.split(',').map(str::trim).collect();
    let [lock_door, lock_issue, issue_door, issue_issue] = parts[..] else {
        return Err(format!(
            "expected 4 comma-separated parts lockDoor,lockIssue,issueDoor,issueIssue (got {})",
            parts.len()
        ));
    };
    let bridge = Bridge {
        lock_door: parse_door(lock_door, "lockDoor")?,
        lock_issue: parse_issue(lock_issue)?,
        issue_door: parse_door(issue_door, "issueDoor")?,
        issue_issue: parse_issue(issue_issue)?,
    };
    if bridge.lock_door == bridge.issue_door {
        return Err("lockDoor and issueDoor must differ".into());
    }
    if (bridge.lock_issue == Issue::Xrp) != (bridge.issue_issue == Issue::Xrp) {
        return Err("cross-currency bridges are not allowed (XRP pairs only with XRP)".into());
    }
    Ok(bridge)
}

fn check_bridge(v: &str) -> Result<(), String> {
    parse_bridge(v).map(|_| ())
}

fn bridge(f: &Fields) -> color_eyre::Result<Bridge> {
    parse_bridge(required(f, "XChainBridge")?).map_err(|e| eyre!("XChainBridge: {e}"))
}

fn bridge_model(f: &Fields) -> color_eyre::Result<XChainBridge<'static>> {
    let b = bridge(f)?;
    Ok(XChainBridge {
        issuing_chain_door: b.issue_door.into(),
        issuing_chain_issue: (&b.issue_issue).into(),
        locking_chain_door: b.lock_door.into(),
        locking_chain_issue: (&b.lock_issue).into(),
    })
}

// ------------------------------------------------------------------ small validators

/// Signature rewards may be zero.
fn reward_drops(v: &str) -> Result<(), String> {
    parse_drops(v).map(|_| ())
}

fn positive_drops(v: &str) -> Result<(), String> {
    match parse_drops(v)? {
        0 => Err("XRP drops must be greater than zero".into()),
        _ => Ok(()),
    }
}

/// `XChainClaimID` is a UInt64 shown in hex (`13f`); never 0.
fn claim_id(v: &str) -> Result<(), String> {
    if v.is_empty() || v.len() > 16 || !v.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(
            "expected 1..=16 hex digits (the id from XChainCreateClaimID, e.g. 13f)".into(),
        );
    }
    match u64::from_str_radix(v, 16) {
        Ok(0) | Err(_) => Err("claim id must be non-zero".into()),
        Ok(_) => Ok(()),
    }
}

fn drops(f: &Fields, name: &str) -> color_eyre::Result<Option<XRPAmount<'static>>> {
    val(f, name)
        .map(|v| {
            v.parse::<u64>()
                .map(|n| XRPAmount::from(n.to_string()))
                .map_err(|e| eyre!("{name}: {e}"))
        })
        .transpose()
}

fn drops_required(f: &Fields, name: &str) -> color_eyre::Result<XRPAmount<'static>> {
    drops(f, name)?.ok_or_else(|| eyre!("{name}: required"))
}

fn xrp(a: XRPAmount<'static>) -> Amount<'static> {
    Amount::XRPAmount(a)
}

fn required<'a>(f: &'a Fields, name: &str) -> color_eyre::Result<&'a str> {
    val(f, name).ok_or_else(|| eyre!("{name}: required"))
}

fn address(f: &Fields, name: &str) -> color_eyre::Result<String> {
    required(f, name).map(str::to_string)
}

fn amount(f: &Fields) -> color_eyre::Result<Amount<'static>> {
    parse_offer_amount(required(f, "Amount")?)
}

/// Normalised claim id (upper-case hex, no leading zeros) as the codec expects.
fn claim_id_hex(f: &Fields) -> color_eyre::Result<String> {
    let n = u64::from_str_radix(required(f, "XChainClaimID")?, 16)?;
    Ok(format!("{n:X}"))
}

fn eyre_model<E: std::fmt::Debug>(e: E) -> color_eyre::Report {
    eyre!("{e:?}")
}

fn to_json<T: serde::Serialize>(tx: &T, what: &str) -> color_eyre::Result<Value> {
    serde_json::to_value(tx).map_err(|e| eyre!("{what} tx_json: {e}"))
}

/// Commit / Claim move `Amount` of one of the bridge's issues.
fn amount_matches_bridge(f: &Fields) -> Result<(), String> {
    let (Some(b), Some(a)) = (val(f, "XChainBridge"), val(f, "Amount")) else {
        return Ok(());
    };
    let bridge = parse_bridge(b)?;
    let mut parts = a.splitn(3, ':');
    let issue = match (parts.next(), parts.next()) {
        (Some("XRP"), _) => Issue::Xrp,
        (Some(currency), Some(issuer)) => Issue::Iou {
            currency: currency.into(),
            issuer: issuer.into(),
        },
        _ => return Ok(()),
    };
    if bridge.has_issue(&issue) {
        Ok(())
    } else {
        Err("Amount: must be the bridge's locking or issuing chain issue".into())
    }
}

fn no_check(_: &Fields) -> Result<(), String> {
    Ok(())
}

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

const BRIDGE_HELP: &str =
    "lockDoor,lockIssue,issueDoor,issueIssue — issue is XRP or CUR:issuer (XRP pairs with XRP)";

const BRIDGE_FIELD: FieldSpec = field(
    "XChainBridge",
    FieldKind::Custom(check_bridge),
    true,
    BRIDGE_HELP,
);

/// `<tx>_json` (simulate) and `<tx>_sign` (typed signer) from a `<tx>_tx` builder.
macro_rules! json_and_sign {
    ($tx:ident, $json:ident, $sign:ident, $what:literal) => {
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

// ------------------------------------------------------------ XChainCreateBridge

pub const CREATE_BRIDGE: TxSpec = TxSpec {
    tx_type: "XChainCreateBridge",
    family: FAMILY,
    summary: "create a cross-chain bridge (door account) ⚠ no delete tx",
    fields: &[
        BRIDGE_FIELD,
        field(
            "SignatureReward",
            FieldKind::Custom(reward_drops),
            true,
            "drops split among witnesses per attestation set (0 allowed)",
        ),
        field(
            "MinAccountCreateAmount",
            FieldKind::Custom(positive_drops),
            false,
            "drops; XRP-XRP bridges only; enables XChainAccountCreateCommit",
        ),
    ],
    confirm: Some(Confirm {
        phrase: "BRIDGE",
        hint: "No transaction deletes a bridge; the signer must be one of its door accounts",
    }),
    check: create_bridge_check,
    build_json: create_bridge_json,
    sign: create_bridge_sign,
};

fn create_bridge_check(f: &Fields) -> Result<(), String> {
    if let (Some(b), Some(_)) = (val(f, "XChainBridge"), val(f, "MinAccountCreateAmount"))
        && !parse_bridge(b)?.is_xrp()
    {
        return Err("MinAccountCreateAmount: only XRP-XRP bridges".into());
    }
    Ok(())
}

fn create_bridge_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<XChainCreateBridge<'static>> {
    Ok(XChainCreateBridge {
        common_fields: common_fields!(account, XChainCreateBridge, sequence, fee_lls),
        signature_reward: xrp(drops_required(f, "SignatureReward")?),
        xchain_bridge: bridge_model(f)?,
        min_account_create_amount: drops(f, "MinAccountCreateAmount")?,
    })
}

json_and_sign!(
    create_bridge_tx,
    create_bridge_json,
    create_bridge_sign,
    "XChainCreateBridge"
);

// ------------------------------------------------------------ XChainModifyBridge

pub const MODIFY_BRIDGE: TxSpec = TxSpec {
    tx_type: "XChainModifyBridge",
    family: FAMILY,
    summary: "change a bridge's reward / minimum account-create amount",
    fields: &[
        BRIDGE_FIELD,
        field(
            "SignatureReward",
            FieldKind::Custom(reward_drops),
            false,
            "new reward in drops",
        ),
        field(
            "MinAccountCreateAmount",
            FieldKind::Custom(positive_drops),
            false,
            "new minimum in drops; XRP-XRP bridges only",
        ),
        field(
            "Flags",
            FieldKind::Flags(MODIFY_FLAGS),
            false,
            "ClearAccountCreateAmount removes the minimum (not with MinAccountCreateAmount)",
        ),
    ],
    confirm: None,
    check: modify_bridge_check,
    build_json: modify_bridge_json,
    sign: modify_bridge_sign,
};

fn modify_clears(f: &Fields) -> bool {
    val(f, "Flags").is_some_and(|v| flag_bits(MODIFY_FLAGS, v) != 0)
}

fn modify_bridge_check(f: &Fields) -> Result<(), String> {
    let min = val(f, "MinAccountCreateAmount");
    if min.is_none() && val(f, "SignatureReward").is_none() && !modify_clears(f) {
        return Err(
            "set SignatureReward, MinAccountCreateAmount or Flags=ClearAccountCreateAmount".into(),
        );
    }
    if min.is_some() && modify_clears(f) {
        return Err("MinAccountCreateAmount: cannot combine with ClearAccountCreateAmount".into());
    }
    if let (Some(b), Some(_)) = (val(f, "XChainBridge"), min)
        && !parse_bridge(b)?.is_xrp()
    {
        return Err("MinAccountCreateAmount: only XRP-XRP bridges".into());
    }
    Ok(())
}

fn modify_bridge_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<XChainModifyBridge<'static>> {
    let mut common = common_fields!(account, XChainModifyBridge, sequence, fee_lls);
    if modify_clears(f) {
        common = common.with_flags(vec![XChainModifyBridgeFlags::TfClearAccountCreateAmount]);
    }
    Ok(XChainModifyBridge {
        common_fields: common,
        xchain_bridge: bridge_model(f)?,
        min_account_create_amount: drops(f, "MinAccountCreateAmount")?.map(xrp),
        signature_reward: drops(f, "SignatureReward")?.map(xrp),
    })
}

json_and_sign!(
    modify_bridge_tx,
    modify_bridge_json,
    modify_bridge_sign,
    "XChainModifyBridge"
);

// ---------------------------------------------------------- XChainCreateClaimID

pub const CREATE_CLAIM_ID: TxSpec = TxSpec {
    tx_type: "XChainCreateClaimID",
    family: FAMILY,
    summary: "reserve a claim id on the destination chain",
    fields: &[
        BRIDGE_FIELD,
        field(
            "OtherChainSource",
            FieldKind::Address,
            true,
            "account that will send XChainCommit on the other chain",
        ),
        field(
            "SignatureReward",
            FieldKind::Custom(reward_drops),
            true,
            "drops; must equal the bridge's SignatureReward",
        ),
    ],
    confirm: None,
    check: no_check,
    build_json: create_claim_id_json,
    sign: create_claim_id_sign,
};

fn create_claim_id_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<XChainCreateClaimID<'static>> {
    Ok(XChainCreateClaimID {
        common_fields: common_fields!(account, XChainCreateClaimID, sequence, fee_lls),
        other_chain_source: address(f, "OtherChainSource")?.into(),
        signature_reward: drops_required(f, "SignatureReward")?,
        xchain_bridge: bridge_model(f)?,
    })
}

json_and_sign!(
    create_claim_id_tx,
    create_claim_id_json,
    create_claim_id_sign,
    "XChainCreateClaimID"
);

// ------------------------------------------------------------------ XChainCommit

pub const COMMIT: TxSpec = TxSpec {
    tx_type: "XChainCommit",
    family: FAMILY,
    summary: "lock / burn funds on this chain for a claim ⚠ moves funds",
    fields: &[
        BRIDGE_FIELD,
        field(
            "XChainClaimID",
            FieldKind::Custom(claim_id),
            true,
            "hex id created on the destination chain by XChainCreateClaimID",
        ),
        field(
            "Amount",
            FieldKind::Amount,
            true,
            "XRP:drops or CUR:issuer:value; must be this chain's bridge issue",
        ),
        field(
            "OtherChainDestination",
            FieldKind::Address,
            false,
            "pay out directly on the other chain; blank = claimer must send XChainClaim",
        ),
    ],
    confirm: Some(Confirm {
        phrase: "COMMIT",
        hint: "Irreversible: funds leave your account until claimed on the other chain",
    }),
    check: amount_matches_bridge,
    build_json: commit_json,
    sign: commit_sign,
};

fn commit_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<XChainCommit<'static>> {
    Ok(XChainCommit {
        common_fields: common_fields!(account, XChainCommit, sequence, fee_lls),
        amount: amount(f)?,
        xchain_bridge: bridge_model(f)?,
        xchain_claim_id: claim_id_hex(f)?.into(),
        other_chain_destination: val(f, "OtherChainDestination").map(|a| a.to_string().into()),
    })
}

json_and_sign!(commit_tx, commit_json, commit_sign, "XChainCommit");

// ------------------------------------------------------------------ XChainClaim

pub const CLAIM: TxSpec = TxSpec {
    tx_type: "XChainClaim",
    family: FAMILY,
    summary: "claim attested funds on this chain ⚠ moves funds",
    fields: &[
        BRIDGE_FIELD,
        field(
            "XChainClaimID",
            FieldKind::Custom(claim_id),
            true,
            "hex id you created with XChainCreateClaimID",
        ),
        field(
            "Amount",
            FieldKind::Amount,
            true,
            "must equal the attested amount; XRP:drops or CUR:issuer:value",
        ),
        field(
            "Destination",
            FieldKind::Address,
            true,
            "receiving account on this chain (must exist)",
        ),
        field(
            "DestinationTag",
            FieldKind::Uint(u32::MAX),
            false,
            "optional destination tag",
        ),
    ],
    confirm: Some(Confirm {
        phrase: "CLAIM",
        hint: "Irreversible: the claim id is consumed and funds are paid to Destination",
    }),
    check: amount_matches_bridge,
    build_json: claim_json,
    sign: claim_sign,
};

fn claim_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<XChainClaim<'static>> {
    Ok(XChainClaim {
        common_fields: common_fields!(account, XChainClaim, sequence, fee_lls),
        amount: amount(f)?,
        destination: address(f, "Destination")?.into(),
        xchain_bridge: bridge_model(f)?,
        xchain_claim_id: claim_id_hex(f)?.into(),
        destination_tag: val(f, "DestinationTag")
            .map(|v| v.parse::<u32>().map_err(|e| eyre!("DestinationTag: {e}")))
            .transpose()?,
    })
}

json_and_sign!(claim_tx, claim_json, claim_sign, "XChainClaim");

// ------------------------------------------------- XChainAccountCreateCommit

pub const ACCOUNT_CREATE_COMMIT: TxSpec = TxSpec {
    tx_type: "XChainAccountCreateCommit",
    family: FAMILY,
    summary: "fund a new account on the other chain ⚠ moves funds",
    fields: &[
        BRIDGE_FIELD,
        field(
            "Destination",
            FieldKind::Address,
            true,
            "account to create on the other chain",
        ),
        field(
            "Amount",
            FieldKind::Custom(positive_drops),
            true,
            "drops; at least the bridge's MinAccountCreateAmount (XRP-XRP bridges only)",
        ),
        field(
            "SignatureReward",
            FieldKind::Custom(reward_drops),
            false,
            "drops; must equal the bridge's SignatureReward",
        ),
    ],
    confirm: Some(Confirm {
        phrase: "COMMIT",
        hint: "Irreversible: funds leave your account; the other-chain account is created by witnesses",
    }),
    check: account_create_commit_check,
    build_json: account_create_commit_json,
    sign: account_create_commit_sign,
};

fn account_create_commit_check(f: &Fields) -> Result<(), String> {
    match val(f, "XChainBridge").map(parse_bridge).transpose()? {
        Some(b) if !b.is_xrp() => {
            Err("XChainBridge: account creation needs an XRP-XRP bridge".into())
        }
        _ => Ok(()),
    }
}

fn account_create_commit_tx(
    account: &str,
    f: &Fields,
    sequence: u32,
    fee_lls: Option<(u32, u32)>,
) -> color_eyre::Result<XChainAccountCreateCommit<'static>> {
    Ok(XChainAccountCreateCommit {
        common_fields: common_fields!(account, XChainAccountCreateCommit, sequence, fee_lls),
        amount: xrp(drops_required(f, "Amount")?),
        destination: address(f, "Destination")?.into(),
        xchain_bridge: bridge_model(f)?,
        signature_reward: drops(f, "SignatureReward")?.map(xrp),
    })
}

json_and_sign!(
    account_create_commit_tx,
    account_create_commit_json,
    account_create_commit_sign,
    "XChainAccountCreateCommit"
);

#[cfg(test)]
mod tests {
    use serde_json::json;
    use xrpl::core::binarycodec::decode;

    use super::super::MAX_DROPS;
    use super::*;

    const DOOR_A: &str = "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh";
    const DOOR_B: &str = "rN7n7otQDd6FczFgLdSqtcsAUxDkw6fzRH";
    const ISSUER: &str = "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN";
    const GENESIS: &str = "rrrrrrrrrrrrrrrrrrrrrhoLvTp";

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

    fn xrp_bridge() -> String {
        format!("{DOOR_A},XRP,{DOOR_B},XRP")
    }

    fn iou_bridge() -> String {
        format!("{DOOR_A},USD:{ISSUER},{DOOR_B},USD:{DOOR_B}")
    }

    /// TC-320: the flat `XChainBridge` grammar — 4 parts, doors checksummed and distinct,
    /// issues `XRP` / `CUR:issuer`, no cross-currency pairing.
    #[test]
    fn bridge_grammar_table() {
        let cases: Vec<(String, Option<&str>)> = vec![
            (xrp_bridge(), None),
            (format!(" {DOOR_A} , XRP , {DOOR_B} , XRP "), None),
            (iou_bridge(), None),
            (format!("{DOOR_A},XRP,{GENESIS},XRP"), None),
            (format!("{DOOR_A},XRP,{DOOR_B}"), Some("got 3")),
            (format!("{DOOR_A},XRP,{DOOR_B},XRP,x"), Some("got 5")),
            (String::new(), Some("got 1")),
            (format!("{DOOR_A},XRP,{DOOR_A},XRP"), Some("must differ")),
            (format!("rBad,XRP,{DOOR_B},XRP"), Some("lockDoor")),
            (format!("{DOOR_A},XRP,rBad,XRP"), Some("issueDoor")),
            (
                format!("{DOOR_A},xrp,{DOOR_B},XRP"),
                Some("expected XRP or CUR:issuer"),
            ),
            (
                format!("{DOOR_A},USD,{DOOR_B},USD:{ISSUER}"),
                Some("CUR:issuer"),
            ),
            (
                format!("{DOOR_A},USD:rBad,{DOOR_B},USD:{ISSUER}"),
                Some("issue"),
            ),
            (
                // Symbols up to 20 bytes are encoded (TC-369); a longer one has no code.
                format!("{DOOR_A},ABCDEFGHIJKLMNOPQRSTU:{ISSUER},{DOOR_B},USD:{ISSUER}"),
                Some("issue"),
            ),
            (format!("{DOOR_A},XRP:{ISSUER},{DOOR_B},XRP"), Some("issue")),
            (
                format!("{DOOR_A},USD:{ISSUER}:1,{DOOR_B},USD:{ISSUER}"),
                Some("CUR:issuer"),
            ),
            (
                format!("{DOOR_A},XRP,{DOOR_B},USD:{ISSUER}"),
                Some("cross-currency"),
            ),
            (
                format!("{DOOR_A},USD:{ISSUER},{DOOR_B},XRP"),
                Some("cross-currency"),
            ),
        ];
        for (input, want) in cases {
            let got = check_bridge(&input);
            match want {
                None => assert!(got.is_ok(), "{input}: {got:?}"),
                Some(n) => {
                    let e = got.expect_err(&input);
                    assert!(e.contains(n), "{input}: {e} lacks {n:?}");
                }
            }
        }
    }

    /// TC-321: per-type field validation — drops boundaries, claim id hex, required fields,
    /// cross-field rules (XRP-only minimum, modify needs a change, amount ∈ bridge issues).
    #[test]
    fn field_validation_tables() {
        let (xb, ib) = (xrp_bridge(), iou_bridge());
        let max = MAX_DROPS.to_string();
        let over = (MAX_DROPS + 1).to_string();
        let cases: Vec<(&str, Fields, Option<&str>)> = vec![
            (
                "XChainCreateBridge",
                f(&[("XChainBridge", &xb), ("SignatureReward", "100")]),
                None,
            ),
            (
                "XChainCreateBridge",
                f(&[("XChainBridge", &xb), ("SignatureReward", "0")]),
                None,
            ),
            (
                "XChainCreateBridge",
                f(&[("XChainBridge", &xb), ("SignatureReward", &max)]),
                None,
            ),
            (
                "XChainCreateBridge",
                f(&[("XChainBridge", &xb), ("SignatureReward", &over)]),
                Some("SignatureReward"),
            ),
            (
                "XChainCreateBridge",
                f(&[("XChainBridge", &xb), ("SignatureReward", "-1")]),
                Some("SignatureReward"),
            ),
            (
                "XChainCreateBridge",
                f(&[("XChainBridge", &xb), ("SignatureReward", "1.5")]),
                Some("SignatureReward"),
            ),
            (
                "XChainCreateBridge",
                f(&[("XChainBridge", &xb)]),
                Some("SignatureReward: required"),
            ),
            (
                "XChainCreateBridge",
                f(&[("SignatureReward", "1")]),
                Some("XChainBridge: required"),
            ),
            (
                "XChainCreateBridge",
                f(&[
                    ("XChainBridge", &xb),
                    ("SignatureReward", "1"),
                    ("MinAccountCreateAmount", "1"),
                ]),
                None,
            ),
            (
                "XChainCreateBridge",
                f(&[
                    ("XChainBridge", &xb),
                    ("SignatureReward", "1"),
                    ("MinAccountCreateAmount", "0"),
                ]),
                Some("MinAccountCreateAmount"),
            ),
            (
                "XChainCreateBridge",
                f(&[("XChainBridge", &ib), ("SignatureReward", "1")]),
                None,
            ),
            (
                "XChainCreateBridge",
                f(&[
                    ("XChainBridge", &ib),
                    ("SignatureReward", "1"),
                    ("MinAccountCreateAmount", "5"),
                ]),
                Some("only XRP-XRP"),
            ),
            (
                "XChainCreateBridge",
                f(&[
                    ("XChainBridge", &xb),
                    ("SignatureReward", "1"),
                    ("Bogus", "1"),
                ]),
                Some("unknown field Bogus"),
            ),
            (
                "XChainModifyBridge",
                f(&[("XChainBridge", &xb)]),
                Some("set SignatureReward"),
            ),
            (
                "XChainModifyBridge",
                f(&[("XChainBridge", &xb), ("SignatureReward", "0")]),
                None,
            ),
            (
                "XChainModifyBridge",
                f(&[("XChainBridge", &xb), ("MinAccountCreateAmount", "7")]),
                None,
            ),
            (
                "XChainModifyBridge",
                f(&[("XChainBridge", &xb), ("Flags", "ClearAccountCreateAmount")]),
                None,
            ),
            (
                "XChainModifyBridge",
                f(&[
                    ("XChainBridge", &xb),
                    ("Flags", "tfClearAccountCreateAmount"),
                ]),
                None,
            ),
            (
                "XChainModifyBridge",
                f(&[("XChainBridge", &xb), ("Flags", "bogus")]),
                Some("unknown flag"),
            ),
            (
                "XChainModifyBridge",
                f(&[
                    ("XChainBridge", &xb),
                    ("Flags", "ClearAccountCreateAmount"),
                    ("MinAccountCreateAmount", "7"),
                ]),
                Some("cannot combine"),
            ),
            (
                "XChainModifyBridge",
                f(&[("XChainBridge", &ib), ("MinAccountCreateAmount", "7")]),
                Some("only XRP-XRP"),
            ),
            (
                "XChainCreateClaimID",
                f(&[
                    ("XChainBridge", &xb),
                    ("OtherChainSource", DOOR_A),
                    ("SignatureReward", "100"),
                ]),
                None,
            ),
            (
                "XChainCreateClaimID",
                f(&[
                    ("XChainBridge", &xb),
                    ("OtherChainSource", "rBad"),
                    ("SignatureReward", "100"),
                ]),
                Some("OtherChainSource"),
            ),
            (
                "XChainCreateClaimID",
                f(&[("XChainBridge", &xb), ("SignatureReward", "100")]),
                Some("OtherChainSource: required"),
            ),
            (
                "XChainCommit",
                f(&[
                    ("XChainBridge", &xb),
                    ("XChainClaimID", "13f"),
                    ("Amount", "XRP:1000"),
                ]),
                None,
            ),
            (
                "XChainCommit",
                f(&[
                    ("XChainBridge", &xb),
                    ("XChainClaimID", "FFFFFFFFFFFFFFFF"),
                    ("Amount", "XRP:1000"),
                ]),
                None,
            ),
            (
                "XChainCommit",
                f(&[
                    ("XChainBridge", &xb),
                    ("XChainClaimID", "10000000000000000"),
                    ("Amount", "XRP:1000"),
                ]),
                Some("XChainClaimID"),
            ),
            (
                "XChainCommit",
                f(&[
                    ("XChainBridge", &xb),
                    ("XChainClaimID", "0"),
                    ("Amount", "XRP:1000"),
                ]),
                Some("non-zero"),
            ),
            (
                "XChainCommit",
                f(&[
                    ("XChainBridge", &xb),
                    ("XChainClaimID", "0x1"),
                    ("Amount", "XRP:1000"),
                ]),
                Some("XChainClaimID"),
            ),
            (
                "XChainCommit",
                f(&[
                    ("XChainBridge", &xb),
                    ("XChainClaimID", "1"),
                    ("Amount", "XRP:0"),
                ]),
                Some("Amount"),
            ),
            (
                "XChainCommit",
                f(&[
                    ("XChainBridge", &xb),
                    ("XChainClaimID", "1"),
                    ("Amount", &format!("USD:{ISSUER}:5")),
                ]),
                Some("locking or issuing"),
            ),
            (
                "XChainCommit",
                f(&[
                    ("XChainBridge", &ib),
                    ("XChainClaimID", "1"),
                    ("Amount", &format!("USD:{ISSUER}:5")),
                ]),
                None,
            ),
            (
                "XChainCommit",
                f(&[
                    ("XChainBridge", &ib),
                    ("XChainClaimID", "1"),
                    ("Amount", &format!("USD:{DOOR_A}:5")),
                ]),
                Some("locking or issuing"),
            ),
            (
                "XChainCommit",
                f(&[
                    ("XChainBridge", &xb),
                    ("XChainClaimID", "1"),
                    ("Amount", "XRP:1"),
                    ("OtherChainDestination", DOOR_A),
                ]),
                None,
            ),
            (
                "XChainCommit",
                f(&[
                    ("XChainBridge", &xb),
                    ("XChainClaimID", "1"),
                    ("Amount", "XRP:1"),
                    ("OtherChainDestination", "rBad"),
                ]),
                Some("OtherChainDestination"),
            ),
            (
                "XChainClaim",
                f(&[
                    ("XChainBridge", &xb),
                    ("XChainClaimID", "1"),
                    ("Amount", "XRP:1"),
                    ("Destination", DOOR_A),
                ]),
                None,
            ),
            (
                "XChainClaim",
                f(&[
                    ("XChainBridge", &xb),
                    ("XChainClaimID", "1"),
                    ("Amount", "XRP:1"),
                    ("Destination", DOOR_A),
                    ("DestinationTag", "4294967295"),
                ]),
                None,
            ),
            (
                "XChainClaim",
                f(&[
                    ("XChainBridge", &xb),
                    ("XChainClaimID", "1"),
                    ("Amount", "XRP:1"),
                    ("Destination", DOOR_A),
                    ("DestinationTag", "4294967296"),
                ]),
                Some("DestinationTag"),
            ),
            (
                "XChainClaim",
                f(&[
                    ("XChainBridge", &xb),
                    ("XChainClaimID", "1"),
                    ("Amount", "XRP:1"),
                ]),
                Some("Destination: required"),
            ),
            (
                "XChainClaim",
                f(&[
                    ("XChainBridge", &ib),
                    ("XChainClaimID", "1"),
                    ("Amount", "XRP:1"),
                    ("Destination", DOOR_A),
                ]),
                Some("locking or issuing"),
            ),
            (
                "XChainAccountCreateCommit",
                f(&[
                    ("XChainBridge", &xb),
                    ("Destination", DOOR_A),
                    ("Amount", "10000000"),
                ]),
                None,
            ),
            (
                "XChainAccountCreateCommit",
                f(&[
                    ("XChainBridge", &xb),
                    ("Destination", DOOR_A),
                    ("Amount", "0"),
                ]),
                Some("Amount"),
            ),
            (
                "XChainAccountCreateCommit",
                f(&[
                    ("XChainBridge", &xb),
                    ("Destination", DOOR_A),
                    ("Amount", "XRP:5"),
                ]),
                Some("Amount"),
            ),
            (
                "XChainAccountCreateCommit",
                f(&[
                    ("XChainBridge", &xb),
                    ("Destination", DOOR_A),
                    ("Amount", "5"),
                    ("SignatureReward", "0"),
                ]),
                None,
            ),
            (
                "XChainAccountCreateCommit",
                f(&[
                    ("XChainBridge", &ib),
                    ("Destination", DOOR_A),
                    ("Amount", "5"),
                ]),
                Some("XRP-XRP"),
            ),
            (
                "XChainAccountCreateCommit",
                f(&[("XChainBridge", &xb), ("Amount", "5")]),
                Some("Destination: required"),
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

    /// Valid forms (as typed) per type. Claim-id is typed lower-case to prove normalisation.
    fn samples() -> Vec<(&'static str, Fields)> {
        let xb = xrp_bridge();
        vec![
            (
                "XChainCreateBridge",
                f(&[
                    ("XChainBridge", &xb),
                    ("SignatureReward", "200"),
                    ("MinAccountCreateAmount", "10000000"),
                ]),
            ),
            (
                "XChainModifyBridge",
                f(&[
                    ("XChainBridge", &xb),
                    ("SignatureReward", "300"),
                    ("Flags", "ClearAccountCreateAmount"),
                ]),
            ),
            (
                "XChainCreateClaimID",
                f(&[
                    ("XChainBridge", &xb),
                    ("OtherChainSource", DOOR_A),
                    ("SignatureReward", "200"),
                ]),
            ),
            (
                "XChainCommit",
                f(&[
                    ("XChainBridge", &xb),
                    ("XChainClaimID", "00013f"),
                    ("Amount", "XRP:5000000"),
                    ("OtherChainDestination", DOOR_A),
                ]),
            ),
            (
                "XChainClaim",
                f(&[
                    ("XChainBridge", &xb),
                    ("XChainClaimID", "1ab"),
                    ("Amount", "XRP:5000000"),
                    ("Destination", DOOR_A),
                    ("DestinationTag", "77"),
                ]),
            ),
            (
                "XChainAccountCreateCommit",
                f(&[
                    ("XChainBridge", &xb),
                    ("Destination", DOOR_A),
                    ("Amount", "20000000"),
                    ("SignatureReward", "200"),
                ]),
            ),
        ]
    }

    /// The bridge as the ledger JSON carries it.
    fn bridge_json(j: &Value) -> Value {
        json!({
            "LockingChainDoor": j["LockingChainDoor"], "LockingChainIssue": j["LockingChainIssue"],
            "IssuingChainDoor": j["IssuingChainDoor"], "IssuingChainIssue": j["IssuingChainIssue"],
        })
    }

    /// TC-322: unsigned `tx_json` per type — account (a door for create/modify), Sequence, bridge
    /// object, drops strings, flag bit, normalised claim id, no Fee / LastLedgerSequence.
    #[test]
    fn unsigned_tx_json_shape() {
        let wallet = wallet();
        let me = wallet.classic_address.clone();
        let xb = format!("{me},XRP,{DOOR_B},XRP");
        let want_bridge = json!({
            "LockingChainDoor": me, "LockingChainIssue": {"currency": "XRP"},
            "IssuingChainDoor": DOOR_B, "IssuingChainIssue": {"currency": "XRP"},
        });
        for (tx_type, mut fields) in samples() {
            // Door-signed types need the signer as a door.
            fields.insert("XChainBridge".into(), xb.clone());
            let s = spec(tx_type);
            let prepared = s.prepare(&fields, 0).expect("valid");
            let j = (s.build_json)(&me, &prepared, 41).expect(tx_type);
            assert_eq!(j["TransactionType"], tx_type);
            assert_eq!(j["Account"], me.as_str());
            assert_eq!(j["Sequence"], 41, "{tx_type}");
            assert!(j.get("Fee").is_none() && j.get("LastLedgerSequence").is_none());
            assert_eq!(j["XChainBridge"], want_bridge, "{tx_type}");
            match tx_type {
                "XChainCreateBridge" => {
                    assert_eq!(j["SignatureReward"], "200");
                    assert_eq!(j["MinAccountCreateAmount"], "10000000");
                }
                "XChainModifyBridge" => {
                    assert_eq!(j["SignatureReward"], "300");
                    assert_eq!(j["Flags"], 0x0001_0000);
                    assert!(j.get("MinAccountCreateAmount").is_none());
                }
                "XChainCreateClaimID" => {
                    assert_eq!(j["OtherChainSource"], DOOR_A);
                    assert_eq!(j["SignatureReward"], "200");
                }
                "XChainCommit" => {
                    assert_eq!(j["XChainClaimID"], "13F");
                    assert_eq!(j["Amount"], "5000000");
                    assert_eq!(j["OtherChainDestination"], DOOR_A);
                }
                "XChainClaim" => {
                    assert_eq!(j["XChainClaimID"], "1AB");
                    assert_eq!(j["Amount"], "5000000");
                    assert_eq!(j["Destination"], DOOR_A);
                    assert_eq!(j["DestinationTag"], 77);
                }
                "XChainAccountCreateCommit" => {
                    assert_eq!(j["Amount"], "20000000");
                    assert_eq!(j["Destination"], DOOR_A);
                    assert_eq!(j["SignatureReward"], "200");
                }
                other => panic!("sample for {other} is not asserted"),
            }
        }
        // IOU bridge: issues serialise as {currency, issuer}.
        let ib = format!("{me},USD:{ISSUER},{DOOR_B},USD:{DOOR_B}");
        let prepared = CREATE_BRIDGE
            .prepare(&f(&[("XChainBridge", &ib), ("SignatureReward", "1")]), 0)
            .unwrap();
        let j = (CREATE_BRIDGE.build_json)(&me, &prepared, 1).unwrap();
        assert_eq!(
            bridge_json(&j["XChainBridge"]),
            json!({
                "LockingChainDoor": me,
                "LockingChainIssue": {"currency": "USD", "issuer": ISSUER},
                "IssuingChainDoor": DOOR_B,
                "IssuingChainIssue": {"currency": "USD", "issuer": DOOR_B},
            })
        );
    }

    /// TC-323: signed blobs decode to the same fields with the simulate-chosen Sequence / Fee /
    /// LastLedgerSequence (distinct values, so swapped arguments fail) and carry a signature.
    #[test]
    fn signed_blobs_decode_per_type() {
        let wallet = wallet();
        let me = wallet.classic_address.clone();
        let xb = format!("{me},XRP,{DOOR_B},XRP");
        let (sequence, fee, lls) = (41_u32, 13_u32, 987_654_u32);
        for (tx_type, mut fields) in samples() {
            fields.insert("XChainBridge".into(), xb.clone());
            let s = spec(tx_type);
            let prepared = s.prepare(&fields, 0).expect("valid");
            let blob = (s.sign)(&wallet, &me, &prepared, sequence, fee, lls).expect(tx_type);
            let d = decode(&blob).expect("decode");
            assert_eq!(d["TransactionType"], tx_type);
            assert_eq!(d["Account"], me.as_str());
            assert_eq!(d["Sequence"], sequence, "{tx_type}");
            assert_eq!(d["Fee"], fee.to_string(), "{tx_type}");
            assert_eq!(d["LastLedgerSequence"], lls, "{tx_type}");
            assert!(d["TxnSignature"].as_str().is_some_and(|s| !s.is_empty()));
            assert_eq!(
                d["SigningPubKey"],
                wallet.public_key.to_uppercase().as_str()
            );
            assert_eq!(
                d["XChainBridge"]["LockingChainDoor"],
                me.as_str(),
                "{tx_type}"
            );
            assert_eq!(d["XChainBridge"]["IssuingChainDoor"], DOOR_B, "{tx_type}");
            assert_eq!(d["XChainBridge"]["LockingChainIssue"]["currency"], "XRP");
            match tx_type {
                "XChainCreateBridge" => {
                    assert_eq!(d["SignatureReward"], "200");
                    assert_eq!(d["MinAccountCreateAmount"], "10000000");
                }
                "XChainModifyBridge" => {
                    assert_eq!(d["SignatureReward"], "300");
                    assert_eq!(d["Flags"], 0x0001_0000);
                }
                "XChainCreateClaimID" => {
                    assert_eq!(d["OtherChainSource"], DOOR_A);
                    assert_eq!(d["SignatureReward"], "200");
                }
                "XChainCommit" => {
                    assert_eq!(d["XChainClaimID"], "000000000000013F"); // codec pads UInt64 to 16 digits
                    assert_eq!(d["Amount"], "5000000");
                    assert_eq!(d["OtherChainDestination"], DOOR_A);
                }
                "XChainClaim" => {
                    assert_eq!(d["XChainClaimID"], "00000000000001AB"); // codec pads UInt64 to 16 digits
                    assert_eq!(d["Amount"], "5000000");
                    assert_eq!(d["Destination"], DOOR_A);
                    assert_eq!(d["DestinationTag"], 77);
                }
                "XChainAccountCreateCommit" => {
                    assert_eq!(d["Amount"], "20000000");
                    assert_eq!(d["Destination"], DOOR_A);
                    assert_eq!(d["SignatureReward"], "200");
                }
                other => panic!("sample for {other} is not asserted"),
            }
        }
    }

    /// TC-328: an IOU bridge (issued-currency issues) encodes and decodes through the signer.
    #[test]
    fn iou_bridge_signs_and_decodes() {
        let wallet = wallet();
        let me = wallet.classic_address.clone();
        let bridge = format!("{me},USD:{ISSUER},{DOOR_B},USD:{DOOR_B}");
        let fields = f(&[
            ("XChainBridge", &bridge),
            ("XChainClaimID", "5"),
            ("Amount", &format!("USD:{ISSUER}:12.5")),
        ]);
        let p = COMMIT.prepare(&fields, 0).expect("valid");
        let blob = (COMMIT.sign)(&wallet, &me, &p, 3, 12, 99).expect("sign");
        let d = decode(&blob).expect("decode");
        assert_eq!(d["XChainBridge"]["LockingChainIssue"]["currency"], "USD");
        assert_eq!(d["XChainBridge"]["LockingChainIssue"]["issuer"], ISSUER);
        assert_eq!(d["XChainBridge"]["IssuingChainIssue"]["issuer"], DOOR_B);
        assert_eq!(d["Amount"]["value"], "12.5");
    }

    /// TC-324: the crate's model rules run when the unsigned JSON is built, before any RPC —
    /// signer must be a door (create / modify).
    #[test]
    fn builder_applies_model_rules() {
        let me = wallet().classic_address.clone();
        // `me` is neither door of this bridge.
        let not_door = f(&[("XChainBridge", &xrp_bridge()), ("SignatureReward", "1")]);
        let p = CREATE_BRIDGE.prepare(&not_door, 0).unwrap();
        let err = (CREATE_BRIDGE.build_json)(&me, &p, 1).unwrap_err();
        assert!(err.to_string().contains("AccountDoorMismatch"), "{err}");
        let modify = f(&[("XChainBridge", &xrp_bridge()), ("SignatureReward", "1")]);
        let p = MODIFY_BRIDGE.prepare(&modify, 0).unwrap();
        let err = (MODIFY_BRIDGE.build_json)(&me, &p, 1).unwrap_err();
        assert!(err.to_string().contains("AccountDoorMismatch"), "{err}");
        // As a door it builds.
        let p = CREATE_BRIDGE.prepare(&not_door, 0).unwrap();
        assert!((CREATE_BRIDGE.build_json)(DOOR_A, &p, 1).is_ok());
    }

    /// TC-325: fund-moving / irreversible types carry a typed confirm; the rest do not.
    #[test]
    fn confirm_phrases() {
        let got: Vec<(&str, Option<&str>)> = SPECS
            .iter()
            .map(|s| (s.tx_type, s.confirm.map(|c| c.phrase)))
            .collect();
        assert_eq!(
            got,
            [
                ("XChainCreateBridge", Some("BRIDGE")),
                ("XChainModifyBridge", None),
                ("XChainCreateClaimID", None),
                ("XChainCommit", Some("COMMIT")),
                ("XChainClaim", Some("CLAIM")),
                ("XChainAccountCreateCommit", Some("COMMIT")),
            ]
        );
    }

    /// TC-329: Tx History has a detail parser for every XChain type (coverage "read" column);
    /// the rendering is covered in `tx_detail/parsers.rs`.
    #[test]
    fn history_parser_registered_for_every_type() {
        let read: Vec<&str> = crate::components::shared::tx_detail::registered_tx_types().collect();
        for s in SPECS {
            assert!(read.contains(&s.tx_type), "{}", s.tx_type);
        }
    }

    /// TC-326: `-x tx` argument parsing resolves the flat bridge field case-insensitively.
    #[test]
    fn cli_fields_parse() {
        let raw = [
            format!("xchainbridge={}", xrp_bridge()),
            "xchainclaimid=13f".to_string(),
            "amount=XRP:1".to_string(),
        ];
        let fields = COMMIT.parse_cli_fields(&raw).expect("parse");
        assert_eq!(fields["XChainBridge"], xrp_bridge());
        assert!(COMMIT.validate(&fields).is_ok());
    }

    /// TC-327: generic submit against a local JSON-RPC mock — `XChainCommit` goes
    /// `server_info → account_info → simulate → submit`; the simulate request and the signed blob
    /// carry the bridge and normalised claim id; invalid bridges are refused before any RPC.
    #[tokio::test]
    async fn generic_submit_xchain_commit_against_local_rpc() {
        use tokio::net::TcpListener;

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
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let params = |bridge: &str| GenericTxSubmitParams {
            tx_type: "XChainCommit".into(),
            fields: f(&[
                ("XChainBridge", bridge),
                ("XChainClaimID", "13f"),
                ("Amount", "XRP:5000000"),
            ]),
            skip_mainnet_prompt: false,
            network: Network::Testnet,
        };

        // Refused before any RPC (the server expects exactly 4 requests).
        submit_generic_transaction(
            &rpc,
            &Network::Testnet,
            params(&format!("{DOOR_A},XRP,{DOOR_A},XRP")),
            &tx,
            Some(&credential),
        )
        .await;
        assert!(matches!(
            rx.try_recv(),
            Ok(Action::GenericTxSubmitErr(e)) if e.contains("must differ")
        ));

        submit_generic_transaction(
            &rpc,
            &Network::Testnet,
            params(&xrp_bridge()),
            &tx,
            Some(&credential),
        )
        .await;
        let Ok(Action::GenericTxSubmitOk(shown)) = rx.try_recv() else {
            panic!("expected GenericTxSubmitOk");
        };
        assert!(shown.starts_with("XChainCommit · "), "{shown}");

        let requests = server.await.expect("join").expect("serve");
        assert_eq!(
            rpc_mock::methods(&requests),
            ["server_info", "account_info", "simulate", "submit"]
        );
        let txj = &requests[2]["params"][0]["tx_json"];
        assert_eq!(txj["TransactionType"], "XChainCommit");
        assert_eq!(txj["XChainClaimID"], "13F");
        assert_eq!(txj["XChainBridge"]["LockingChainDoor"], DOOR_A);
        let blob = requests[3]["params"][0]["tx_blob"].as_str().expect("blob");
        let d = decode(blob).expect("decode");
        assert_eq!((&d["Sequence"], &d["Fee"]), (&json!(7), &json!("15")));
        assert_eq!(d["XChainBridge"]["IssuingChainDoor"], DOOR_B);
    }
}
