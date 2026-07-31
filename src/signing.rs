//! XRPL CLI signing helpers and payment helpers.
//!
//! ## Threading vs. environment mutation
//!
//! [`SigningConfig::prime_seed_source`] clears `XRPL_SEED` with [`std::env::remove_var`], wrapped in
//! `unsafe` where required by the platform API. Only call during **single-threaded process
//! startup** before other threads observe the environment — concurrent mutation is undefined behaviour.

use std::{
    env,
    io::{self, Write},
};

use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;

use crate::network::Network;
use crate::xrpl::WalletProposeResult;

pub const SEED_ENV: &str = "XRPL_SEED";

/// Generate a new key pair locally (no `wallet_propose` RPC).
///
/// Public RPC / Clio endpoints often omit `master_seed` or reject the method;
/// local generation matches rippled `wallet_propose` semantics for TUI keygen.
pub fn propose_wallet_local(key_type: &str) -> color_eyre::Result<WalletProposeResult> {
    use xrpl::constants::CryptoAlgorithm;
    use xrpl::core::addresscodec::decode_seed;
    use xrpl::wallet::Wallet;

    let (algo, key_type_label) = match key_type.to_lowercase().as_str() {
        "ed25519" => (CryptoAlgorithm::ED25519, "ed25519"),
        "secp256k1" => (CryptoAlgorithm::SECP256K1, "secp256k1"),
        other => {
            return Err(color_eyre::eyre::eyre!(
                "unsupported key_type: {other} (expected ed25519 or secp256k1)"
            ));
        }
    };

    let wallet =
        Wallet::create(Some(algo)).map_err(|e| color_eyre::eyre::eyre!("keygen: {e:?}"))?;
    let (entropy, _) = decode_seed(&wallet.seed)
        .map_err(|e| color_eyre::eyre::eyre!("keygen: decode seed: {e:?}"))?;
    let master_seed_hex: String = entropy.iter().map(|b| format!("{b:02X}")).collect();

    Ok(WalletProposeResult {
        master_seed: wallet.seed.clone(),
        master_seed_hex,
        account_id: wallet.classic_address.clone(),
        public_key: wallet.public_key.clone(),
        public_key_hex: wallet.public_key.clone(),
        key_type: key_type_label.into(),
    })
}

/// Trim whitespace from a family seed (`s...` / `sEd...`); shells and dotenv often add `\n`.
#[must_use]
pub fn trim_family_seed(seed: &str) -> &str {
    seed.trim()
}

/// Prefix bytes (after base58-check decode) for Ed25519 family seeds (`sEd...`).
///
/// See [XRPL base58 encodings](https://xrpl.org/base58-encodings.html).
const XRPL_ED25519_SEED_PREFIX: &[u8] = &[0x01, 0xE1, 0x4B];

/// Build a [`xrpl::wallet::Wallet`] from a family seed (`s...` / `sEd...`).
///
/// `Wallet::new` uses `decode_seed`, which can fall through to the secp256k1 path when the
/// Ed25519 decode fails in unexpected ways, producing `InvalidSecretKey`. For strings that look
/// like Ed25519 family seeds (`sEd` prefix), we decode with the Ed25519 seed prefix only.
pub fn wallet_from_family_seed(
    seed: &str,
    sequence: u64,
) -> color_eyre::Result<xrpl::wallet::Wallet> {
    use xrpl::constants::CryptoAlgorithm;
    use xrpl::core::addresscodec::utils::{SEED_LENGTH, decode_base58};
    use xrpl::core::keypairs::generate_seed;
    use xrpl::wallet::Wallet;

    let seed = trim_family_seed(seed);
    if seed.starts_with("sEd") {
        let payload = decode_base58(seed, XRPL_ED25519_SEED_PREFIX).map_err(|e| {
            color_eyre::eyre::eyre!(
                "invalid Ed25519 family seed (check characters / checksum): {e:?}"
            )
        })?;
        let bytes: [u8; SEED_LENGTH] = payload.try_into().map_err(|v: Vec<u8>| {
            color_eyre::eyre::eyre!(
                "Ed25519 family seed payload length {} (expected {SEED_LENGTH})",
                v.len()
            )
        })?;
        let canonical = generate_seed(Some(bytes), Some(CryptoAlgorithm::ED25519))
            .map_err(|e| color_eyre::eyre::eyre!("Ed25519 seed re-encode: {e:?}"))?;
        Wallet::new(&canonical, sequence)
            .map_err(|e| color_eyre::eyre::eyre!("wallet error: {:?}", e))
    } else {
        Wallet::new(seed, sequence).map_err(|e| color_eyre::eyre::eyre!("wallet error: {:?}", e))
    }
}

/// Derive classic address from a family seed (UI / poll seed-address lookup).
pub fn seed_to_address(seed: &str) -> Result<String, String> {
    wallet_from_family_seed(trim_family_seed(seed), 0)
        .map(|w| w.classic_address.clone())
        .map_err(|e| format!("{e}"))
}

/// Resolved signing credentials. Seed is memory-masked via `secrecy`.
///
/// Used for write paths that need a seed (e.g. CLI `Send` via `XRPL_SEED` or
/// `config.toml [xrpl.signing] seed`). `load` clears `XRPL_SEED` from the
/// process environment immediately after reading it.
pub struct SigningConfig {
    pub seed: Option<SecretString>,
}

impl SigningConfig {
    /// Resolves the signing seed.
    /// Priority: `XRPL_SEED` env var > `config.toml [xrpl.signing] seed`.
    /// The plain string is immediately wrapped in `SecretString` to minimise
    /// the window where the value is unprotected in memory.
    pub fn prime_seed_source(seed_from_config: Option<String>) -> Self {
        let env_seed = env::var(SEED_ENV).ok();
        // Security: remove seed from environment immediately after reading to prevent
        // exposure via /proc/self/environ or inheritance by child processes.
        if env_seed.is_some() {
            // SAFETY: no other threads access SEED_ENV concurrently at this point
            unsafe { env::remove_var(SEED_ENV) };
        }
        let seed = env_seed
            .or(seed_from_config)
            .map(|s| trim_family_seed(&s).to_string())
            .filter(|s| !s.is_empty())
            .map(SecretString::from);
        Self { seed }
    }

    #[cfg(test)]
    pub fn has_seed(&self) -> bool {
        self.seed.is_some()
    }
}

/// Prompts for explicit confirmation before executing a write operation on mainnet.
///
/// Returns `true` if the operation should proceed:
/// - Always `true` when `yes == true` (scripting / `--yes` flag).
/// - Always `true` when `network` is not mainnet.
/// - `true` only if the user types `y` or `yes` (case-insensitive) otherwise.
///
/// Non-TUI `Send` on mainnet calls this unless the caller passes `skip_prompt`
/// (e.g. scripting / `--yes`).
pub fn prompt_mainnet_confirmation(operation: &str, network: &Network, skip_prompt: bool) -> bool {
    if !network.is_mainnet() || skip_prompt {
        return true;
    }
    eprint!("⚠️  MAINNET: about to execute {operation}. Continue? [y/N] ");
    let _ = io::stderr().flush();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

/// Create, sign, and encode a Payment transaction as a submit-ready blob.
///
/// Phase 3: Transaction signing implementation for XRP transfers
#[allow(dead_code, clippy::too_many_arguments)]
/// `amount_spec`: XRP value (XRP mode) or IOU value (IOU mode).
/// `iou_currency`: If Some, triggers IOU mode (e.g. "USD").
/// `iou_issuer`: If Some, issuer address for IOU mode.
pub fn create_and_sign_payment(
    seed: &SecretString,
    account: &str,
    destination: &str,
    amount_spec: &str,
    iou_currency: Option<&str>,
    iou_issuer: Option<&str>,
    destination_tag: Option<u32>,
    sequence: u32,
    fee_drops: u32,
    last_ledger_sequence: u32,
    _network: &Network,
) -> color_eyre::Result<String> {
    use xrpl::core::binarycodec::encode;
    use xrpl::models::transactions::payment::Payment;
    use xrpl::models::transactions::{CommonFields, TransactionType};
    use xrpl::models::{Amount, XRPAmount};
    use xrpl::transaction::sign;

    let wallet =
        wallet_from_family_seed(seed.expose_secret(), 0).map_err(|e| color_eyre::eyre::eyre!(e))?;

    let amount: Amount = match (iou_currency, iou_issuer) {
        (Some(cur), Some(iss)) => {
            let ica = xrpl::models::IssuedCurrencyAmount {
                currency: cur.to_string().into(),
                issuer: iss.to_string().into(),
                value: amount_spec.to_string().into(),
            };
            Amount::IssuedCurrencyAmount(ica)
        }
        (None, None) => {
            let amount_drops = crate::xrpl::xrp_to_drops(amount_spec)?;
            amount_drops.into()
        }
        _ => {
            return Err(color_eyre::eyre::eyre!(
                "IOU payment requires both currency and issuer"
            ));
        }
    };

    let mut payment = Payment {
        common_fields: CommonFields::from_account(account.to_string())
            .with_transaction_type(TransactionType::Payment)
            .with_sequence(sequence)
            .with_fee(XRPAmount::from(fee_drops.to_string()))
            .with_last_ledger_sequence(last_ledger_sequence),
        amount,
        destination: destination.to_string().into(),
        destination_tag,
        ..Default::default()
    };

    sign(&mut payment, &wallet, false)
        .map_err(|e| color_eyre::eyre::eyre!("sign error: {:?}", e))?;

    encode(&payment).map_err(|e| color_eyre::eyre::eyre!("encode error: {:?}", e))
}

/// Unsigned Payment JSON for `simulate` (XRP or IOU).
pub fn build_payment_tx_json_for_simulate(
    account: &str,
    destination: &str,
    amount_spec: &str,
    iou_currency: Option<&str>,
    iou_issuer: Option<&str>,
    destination_tag: Option<u32>,
    sequence: u32,
) -> color_eyre::Result<Value> {
    use xrpl::models::transactions::payment::Payment;
    use xrpl::models::transactions::{CommonFields, TransactionType};
    use xrpl::models::{Amount, IssuedCurrencyAmount};

    let amount: Amount = match (iou_currency, iou_issuer) {
        (Some(cur), Some(iss)) => {
            let ica = IssuedCurrencyAmount {
                currency: cur.to_string().into(),
                issuer: iss.to_string().into(),
                value: amount_spec.to_string().into(),
            };
            Amount::IssuedCurrencyAmount(ica)
        }
        (None, None) => {
            let amount_drops = crate::xrpl::xrp_to_drops(amount_spec)?;
            amount_drops.into()
        }
        _ => {
            return Err(color_eyre::eyre::eyre!(
                "IOU payment requires both currency and issuer"
            ));
        }
    };

    let payment = Payment {
        common_fields: CommonFields::from_account(account.to_string())
            .with_transaction_type(TransactionType::Payment)
            .with_sequence(sequence),
        amount,
        destination: destination.to_string().into(),
        destination_tag,
        ..Default::default()
    };

    serde_json::to_value(&payment).map_err(|e| color_eyre::eyre::eyre!("payment tx_json: {e}"))
}

/// Unsigned AccountSet JSON for `simulate`.
#[allow(clippy::too_many_arguments)]
pub fn build_account_set_tx_json_for_simulate(
    account: &str,
    sequence: u32,
    set_flag: Option<xrpl::models::transactions::account_set::AccountSetFlag>,
    clear_flag: Option<xrpl::models::transactions::account_set::AccountSetFlag>,
    domain_hex: Option<&str>,
    tick_size: Option<u32>,
    transfer_rate: Option<u32>,
) -> color_eyre::Result<Value> {
    use std::borrow::Cow;
    use xrpl::models::transactions::account_set::AccountSet;
    use xrpl::models::transactions::{CommonFields, TransactionType};

    let tx = AccountSet {
        common_fields: CommonFields::from_account(account.to_string())
            .with_transaction_type(TransactionType::AccountSet)
            .with_sequence(sequence),
        set_flag,
        clear_flag,
        domain: domain_hex.map(|s| Cow::Owned(s.to_string())),
        tick_size,
        transfer_rate,
        ..Default::default()
    };

    serde_json::to_value(&tx).map_err(|e| color_eyre::eyre::eyre!("account_set tx_json: {e}"))
}

/// Extract `Sequence`, `Fee`, and `LastLedgerSequence` from a successful simulate response.
pub fn sequence_fee_ledger_from_simulate(tx_json: &Value) -> color_eyre::Result<(u32, u32, u32)> {
    fn field_u32(tx: &Value, key: &str) -> color_eyre::Result<u32> {
        let v = tx
            .get(key)
            .ok_or_else(|| color_eyre::eyre::eyre!("simulate tx_json missing {key}"))?;
        let n = v
            .as_u64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            .ok_or_else(|| color_eyre::eyre::eyre!("simulate tx_json invalid {key}"))?;
        u32::try_from(n).map_err(|_| color_eyre::eyre::eyre!("simulate tx_json {key} out of range"))
    }
    Ok((
        field_u32(tx_json, "Sequence")?,
        field_u32(tx_json, "Fee")?,
        field_u32(tx_json, "LastLedgerSequence")?,
    ))
}

/// Create unsigned Payment JSON for signing
#[allow(dead_code)]
pub fn create_unsigned_payment_json(
    account: &str,
    destination: &str,
    amount_xrp: &str,
    sequence: u32,
    fee: u32,
    last_ledger_sequence: u32,
) -> color_eyre::Result<Value> {
    let amount_drops = crate::xrpl::xrp_to_drops(amount_xrp)?;

    let tx_json = serde_json::json!({
        "Account": account,
        "Destination": destination,
        "Amount": amount_drops,
        "Sequence": sequence,
        "Fee": fee,
        "LastLedgerSequence": last_ledger_sequence,
        "TransactionType": "Payment"
    });

    Ok(tx_json)
}

/// Create, sign, and encode a SetRegularKey transaction as a submit-ready blob.
///
/// Pass `regular_key` as `None` to clear (remove) the existing regular key.
#[allow(dead_code)]
pub fn create_and_sign_set_regular_key(
    seed: &SecretString,
    account: &str,
    regular_key: Option<&str>,
    sequence: u32,
    fee_drops: u32,
    last_ledger_sequence: u32,
    _network: &Network,
) -> color_eyre::Result<String> {
    use xrpl::core::binarycodec::encode;
    use xrpl::models::XRPAmount;
    use xrpl::models::transactions::set_regular_key::SetRegularKey;
    use xrpl::models::transactions::{CommonFields, TransactionType};
    use xrpl::transaction::sign;

    let wallet =
        wallet_from_family_seed(seed.expose_secret(), 0).map_err(|e| color_eyre::eyre::eyre!(e))?;

    let mut tx = SetRegularKey {
        common_fields: CommonFields::from_account(account.to_string())
            .with_transaction_type(TransactionType::SetRegularKey)
            .with_sequence(sequence)
            .with_fee(XRPAmount::from(fee_drops.to_string()))
            .with_last_ledger_sequence(last_ledger_sequence),
        regular_key: regular_key.map(|k| k.to_string().into()),
    };

    sign(&mut tx, &wallet, false).map_err(|e| color_eyre::eyre::eyre!("sign error: {:?}", e))?;

    encode(&tx).map_err(|e| color_eyre::eyre::eyre!("encode error: {:?}", e))
}

/// Create and sign an `EscrowCreate` transaction, returning the tx_blob hex.
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub fn create_and_sign_escrow_create(
    seed: &SecretString,
    account: &str,
    destination: &str,
    amount_drops: &str,
    finish_after: u32,
    sequence: u32,
    fee_drops: u32,
    last_ledger_sequence: u32,
    _network: &Network,
) -> color_eyre::Result<String> {
    use xrpl::core::binarycodec::encode;
    use xrpl::models::transactions::escrow_create::EscrowCreate;
    use xrpl::models::transactions::{CommonFields, TransactionType};
    use xrpl::transaction::sign;

    let wallet =
        wallet_from_family_seed(seed.expose_secret(), 0).map_err(|e| color_eyre::eyre::eyre!(e))?;

    let mut tx = EscrowCreate {
        common_fields: CommonFields::from_account(account.to_string())
            .with_transaction_type(TransactionType::EscrowCreate)
            .with_sequence(sequence)
            .with_fee(xrpl::models::XRPAmount::from(fee_drops.to_string()))
            .with_last_ledger_sequence(last_ledger_sequence),
        amount: xrpl::models::XRPAmount::from(amount_drops.to_string()),
        destination: destination.into(),
        finish_after: if finish_after > 0 {
            Some(finish_after)
        } else {
            None
        },
        ..Default::default()
    };

    sign(&mut tx, &wallet, false).map_err(|e| color_eyre::eyre::eyre!("sign error: {:?}", e))?;

    encode(&tx).map_err(|e| color_eyre::eyre::eyre!("encode error: {:?}", e))
}

enum OfferAmountSpec<'a> {
    Xrp(&'a str),
    Iou {
        currency: &'a str,
        issuer: &'a str,
        value: &'a str,
    },
}

fn parse_offer_amount_spec(spec: &str) -> color_eyre::Result<OfferAmountSpec<'_>> {
    let parts: Vec<&str> = spec.splitn(3, ':').collect();
    if parts.len() < 2 {
        color_eyre::eyre::bail!("invalid amount spec (use XRP:drops or CUR:issuer:value): {spec}");
    }
    if parts[0] == "XRP" {
        Ok(OfferAmountSpec::Xrp(parts[1]))
    } else if parts.len() < 3 {
        color_eyre::eyre::bail!("IOU amount needs 3 parts (CUR:issuer:value): {spec}");
    } else {
        Ok(OfferAmountSpec::Iou {
            currency: parts[0],
            issuer: parts[1],
            value: parts[2],
        })
    }
}

/// Build an `Amount` from a compact spec string.
/// `"XRP:100000000"` → XRP amount in drops.
/// `"USD:rIssuer:100.5"` → issued currency amount.
#[allow(dead_code)]
fn parse_offer_amount(spec: &str) -> color_eyre::Result<xrpl::models::Amount<'static>> {
    use xrpl::models::{Amount, IssuedCurrencyAmount, XRPAmount};

    match parse_offer_amount_spec(spec)? {
        OfferAmountSpec::Xrp(drops) => Ok(Amount::XRPAmount(XRPAmount::from(drops.to_string()))),
        OfferAmountSpec::Iou {
            currency,
            issuer,
            value,
        } => {
            let ica = IssuedCurrencyAmount {
                currency: currency.to_string().into(),
                issuer: issuer.to_string().into(),
                value: value.to_string().into(),
            };
            Ok(Amount::IssuedCurrencyAmount(ica))
        }
    }
}

/// Convert an OfferCreate compact amount spec to a [`serde_json::Value`] for
/// use in simulate tx_json.
#[allow(dead_code)]
pub(crate) fn offer_spec_to_json_value(spec: &str) -> color_eyre::Result<serde_json::Value> {
    match parse_offer_amount_spec(spec)? {
        OfferAmountSpec::Xrp(drops) => Ok(serde_json::Value::String(drops.to_string())),
        OfferAmountSpec::Iou {
            currency,
            issuer,
            value,
        } => Ok(serde_json::json!({
            "currency": currency,
            "issuer": issuer,
            "value": value
        })),
    }
}

/// Create and sign an `OfferCreate` transaction, returning the tx_blob hex.
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub fn create_and_sign_offer_create(
    seed: &SecretString,
    account: &str,
    taker_gets_spec: &str,
    taker_pays_spec: &str,
    sequence: u32,
    fee_drops: u32,
    last_ledger_sequence: u32,
    _network: &Network,
) -> color_eyre::Result<String> {
    use xrpl::core::binarycodec::encode;
    use xrpl::models::XRPAmount;
    use xrpl::models::transactions::offer_create::OfferCreate;
    use xrpl::models::transactions::{CommonFields, TransactionType};
    use xrpl::transaction::sign;

    let wallet =
        wallet_from_family_seed(seed.expose_secret(), 0).map_err(|e| color_eyre::eyre::eyre!(e))?;

    let taker_gets = parse_offer_amount(taker_gets_spec)?;
    let taker_pays = parse_offer_amount(taker_pays_spec)?;

    let mut tx = OfferCreate {
        common_fields: CommonFields::from_account(account.to_string())
            .with_transaction_type(TransactionType::OfferCreate)
            .with_sequence(sequence)
            .with_fee(XRPAmount::from(fee_drops.to_string()))
            .with_last_ledger_sequence(last_ledger_sequence),
        taker_gets,
        taker_pays,
        ..Default::default()
    };

    sign(&mut tx, &wallet, false).map_err(|e| color_eyre::eyre::eyre!("sign error: {:?}", e))?;

    encode(&tx).map_err(|e| color_eyre::eyre::eyre!("encode error: {:?}", e))
}

/// Lowercase ASCII domain → hex string for `AccountSet.domain`.
pub fn domain_ascii_to_hex(domain: &str) -> String {
    domain
        .to_ascii_lowercase()
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[must_use]
pub fn is_placeholder_account_set_flag(label: &str) -> bool {
    matches!(label.trim(), "" | "(none)" | "none" | "-")
}

#[must_use]
pub fn resolved_account_set_flag(flag: &Option<String>) -> bool {
    flag.as_ref()
        .is_some_and(|s| !is_placeholder_account_set_flag(s))
}

/// Map Wallet UI flag labels to XRPL enum.
pub fn parse_account_set_flag_choice(
    label: Option<&str>,
) -> Option<xrpl::models::transactions::account_set::AccountSetFlag> {
    use xrpl::models::transactions::account_set::AccountSetFlag;
    let label = label?.trim();
    if is_placeholder_account_set_flag(label) {
        return None;
    }
    match label {
        "RequireDest" => Some(AccountSetFlag::AsfRequireDest),
        "RequireAuth" => Some(AccountSetFlag::AsfRequireAuth),
        "DisallowXRP" => Some(AccountSetFlag::AsfDisallowXRP),
        "DisableMaster" => Some(AccountSetFlag::AsfDisableMaster),
        "AccountTxnID" => Some(AccountSetFlag::AsfAccountTxnID),
        "NoFreeze" => Some(AccountSetFlag::AsfNoFreeze),
        "GlobalFreeze" => Some(AccountSetFlag::AsfGlobalFreeze),
        "DefaultRipple" => Some(AccountSetFlag::AsfDefaultRipple),
        "DepositAuth" => Some(AccountSetFlag::AsfDepositAuth),
        "NFTokenMinter" => Some(AccountSetFlag::AsfAuthorizedNFTokenMinter),
        "DisallowInCheck" => Some(AccountSetFlag::AsfDisallowIncomingCheck),
        "DisallowInPayChan" => Some(AccountSetFlag::AsfDisallowIncomingPayChan),
        "DisallowInTrustline" => Some(AccountSetFlag::AsfDisallowIncomingTrustline),
        "DisallowInNFTOffer" => Some(AccountSetFlag::AsfDisallowIncomingNFTokenOffer),
        "AllowTrustClawback" => Some(AccountSetFlag::AsfAllowTrustLineClawback),
        _ => None,
    }
}

/// Serialize, sign, and encode an AccountSet transaction.
#[allow(clippy::too_many_arguments)]
pub fn create_and_sign_account_set(
    seed: &SecretString,
    account: &str,
    sequence: u32,
    fee_drops: u32,
    last_ledger_sequence: u32,
    set_flag: Option<xrpl::models::transactions::account_set::AccountSetFlag>,
    clear_flag: Option<xrpl::models::transactions::account_set::AccountSetFlag>,
    domain_hex: Option<&str>,
    tick_size: Option<u32>,
    transfer_rate: Option<u32>,
) -> color_eyre::Result<String> {
    use std::borrow::Cow;
    use xrpl::core::binarycodec::encode;
    use xrpl::models::transactions::account_set::AccountSet;
    use xrpl::models::transactions::{CommonFields, TransactionType};
    use xrpl::models::{Model, XRPAmount};
    use xrpl::transaction::sign;

    let wallet =
        wallet_from_family_seed(seed.expose_secret(), 0).map_err(|e| color_eyre::eyre::eyre!(e))?;

    let mut tx = AccountSet {
        common_fields: CommonFields::from_account(account.to_string())
            .with_transaction_type(TransactionType::AccountSet)
            .with_sequence(sequence)
            .with_fee(XRPAmount::from(fee_drops.to_string()))
            .with_last_ledger_sequence(last_ledger_sequence),
        set_flag,
        clear_flag,
        domain: domain_hex.map(|s| Cow::Owned(s.to_string())),
        tick_size,
        transfer_rate,
        ..Default::default()
    };

    tx.validate()
        .map_err(|e| color_eyre::eyre::eyre!("account_set validation: {e}"))?;

    sign(&mut tx, &wallet, false).map_err(|e| color_eyre::eyre::eyre!("sign error: {:?}", e))?;

    encode(&tx).map_err(|e| color_eyre::eyre::eyre!("encode error: {:?}", e))
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::*;
    use crate::config::{TestEnvGuard, env_lock};

    #[test]
    fn resolve_seed_none_when_no_source() {
        let _g = env_lock();
        let _env = TestEnvGuard::new(&[SEED_ENV]);
        _env.remove(SEED_ENV);
        let signing_config = SigningConfig::prime_seed_source(None);
        assert!(!signing_config.has_seed());
    }

    #[test]
    fn sequence_fee_ledger_from_simulate_extracts_fields() {
        let tx = serde_json::json!({
            "Sequence": 42,
            "Fee": "12",
            "LastLedgerSequence": 9_999_999
        });
        let (seq, fee, lls) = sequence_fee_ledger_from_simulate(&tx).expect("fields should parse");
        assert_eq!(seq, 42);
        assert_eq!(fee, 12);
        assert_eq!(lls, 9_999_999);
    }

    #[test]
    fn build_payment_tx_json_for_simulate_xrp() {
        let v = build_payment_tx_json_for_simulate("rSender", "rDest", "1", None, None, None, 7)
            .expect("payment json");
        assert_eq!(v["TransactionType"], "Payment");
        assert_eq!(v["Sequence"], 7);
        assert_eq!(v["Account"], "rSender");
        assert_eq!(v["Destination"], "rDest");
        assert_eq!(v["Amount"], "1000000");
    }

    #[test]
    fn build_payment_tx_json_for_simulate_iou() {
        let v = build_payment_tx_json_for_simulate(
            "rSender",
            "rDest",
            "12.5",
            Some("USD"),
            Some("rIssuer"),
            None,
            9,
        )
        .expect("iou payment json");
        assert_eq!(v["TransactionType"], "Payment");
        assert_eq!(v["Sequence"], 9);
        assert_eq!(v["Account"], "rSender");
        assert_eq!(v["Destination"], "rDest");
        assert_eq!(v["Amount"]["currency"], "USD");
        assert_eq!(v["Amount"]["issuer"], "rIssuer");
        assert_eq!(v["Amount"]["value"], "12.5");
    }

    #[test]
    fn build_payment_tx_json_for_simulate_includes_destination_tag() {
        let v =
            build_payment_tx_json_for_simulate("rSender", "rDest", "1", None, None, Some(12345), 3)
                .expect("tagged payment json");
        assert_eq!(v["DestinationTag"], 12345);
    }

    #[test]
    fn build_payment_tx_json_rejects_partial_iou() {
        let err =
            build_payment_tx_json_for_simulate("rSender", "rDest", "1", Some("USD"), None, None, 1)
                .expect_err("partial iou");
        assert!(format!("{err}").contains("both currency and issuer"));
    }

    #[test]
    fn resolve_seed_from_config_raw() {
        let _g = env_lock();
        let _env = TestEnvGuard::new(&[SEED_ENV]);
        _env.remove(SEED_ENV);
        let signing_config = SigningConfig::prime_seed_source(Some("sTest1234".to_string()));
        assert!(signing_config.has_seed());
    }

    #[test]
    fn confirm_non_mainnet_skips_prompt() {
        let _g = env_lock();
        assert!(prompt_mainnet_confirmation(
            "Payment",
            &Network::Testnet,
            false
        ));
        assert!(prompt_mainnet_confirmation(
            "Payment",
            &Network::Devnet,
            false
        ));
    }

    #[test]
    fn confirm_mainnet_with_yes_flag_skips_prompt() {
        let _g = env_lock();
        assert!(prompt_mainnet_confirmation(
            "Payment",
            &Network::Mainnet,
            true
        ));
    }

    #[test]
    fn domain_ascii_to_hex_lowercase() {
        assert_eq!(domain_ascii_to_hex("Example.COM"), "6578616d706c652e636f6d");
    }

    /// TC: AccountSet flag label parsing
    #[test]
    fn parse_require_dest() {
        use xrpl::models::transactions::account_set::AccountSetFlag;
        assert_eq!(
            parse_account_set_flag_choice(Some("RequireDest")),
            Some(AccountSetFlag::AsfRequireDest)
        );
        assert_eq!(parse_account_set_flag_choice(Some("(none)")), None);
    }

    #[test]
    fn propose_wallet_local_ed25519() {
        let r = propose_wallet_local("ed25519").expect("local keygen");
        assert!(r.master_seed.starts_with("sEd") || r.master_seed.starts_with('s'));
        assert!(r.account_id.starts_with('r'));
        assert_eq!(r.key_type, "ed25519");
        assert_eq!(r.master_seed_hex.len(), 32);
    }

    #[test]
    fn ed25519_family_seed_wallet_new_ok() {
        let seed = "sEdSkooMk31MeTjbHVE7vLvgCpEMAdB";
        let w = wallet_from_family_seed(seed, 0).expect("ed25519 wallet");
        assert_eq!(w.classic_address, "rU3Cw9Vezt3m3E7EonCnfGN1raFdudq4QQ");
    }

    #[test]
    fn ed25519_seed_trims_whitespace() {
        let seed = "sEdSkooMk31MeTjbHVE7vLvgCpEMAdB  \n";
        let w = wallet_from_family_seed(seed, 0).expect("trimmed ed25519");
        assert_eq!(w.classic_address, "rU3Cw9Vezt3m3E7EonCnfGN1raFdudq4QQ");
    }

    #[test]
    fn secp256k1_family_seed_wallet_unchanged() {
        use xrpl::wallet::Wallet;
        let seed = "sn259rEFXrQrWyx3Q7XneWcwV6dfL";
        let a = wallet_from_family_seed(seed, 0).expect("wallet");
        let b = Wallet::new(seed, 0).expect("direct");
        assert_eq!(a.classic_address, b.classic_address);
    }

    /// TC-049
    #[test]
    fn env_seed_overrides_config_seed() {
        let _g = env_lock();
        let _env = TestEnvGuard::new(&[SEED_ENV]);
        _env.set(SEED_ENV, "sFromEnvOverride");
        let signing_config =
            SigningConfig::prime_seed_source(Some("sFromConfigIgnored".to_string()));
        assert!(signing_config.has_seed());
        assert_eq!(
            signing_config.seed.as_ref().unwrap().expose_secret(),
            "sFromEnvOverride"
        );
    }
}
