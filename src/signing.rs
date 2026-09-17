//! XRPL CLI signing helpers and payment helpers.

use std::io::{self, Write};

use kobe_primitives::Derive;
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;

use crate::network::Network;
use crate::xrpl::GeneratedWalletKeys;
pub const SEED_ENV: &str = "XRPL_SEED";
pub const MNEMONIC_ENV: &str = "XRPL_MNEMONIC";

pub fn credential_from_secrets(
    seed: Option<&SecretString>,
    mnemonic: Option<&SecretString>,
) -> color_eyre::Result<Option<SigningCredential>> {
    match (seed, mnemonic) {
        (Some(_), Some(_)) => {
            color_eyre::eyre::bail!("configure only one of XRPL_SEED or XRPL_MNEMONIC")
        }
        (Some(seed), None) => Ok(Some(SigningCredential::from_family_seed(
            seed.expose_secret(),
        ))),
        (None, Some(mnemonic)) => Ok(Some(SigningCredential::from_bip39_mnemonic(
            mnemonic.expose_secret(),
        )?)),
        (None, None) => Ok(None),
    }
}

/// Supported XRPL signing credentials.
///
/// BIP39 currently derives the XRPL secp256k1 account at index 0 only.
#[derive(Clone)]
pub enum SigningCredential {
    FamilySeed(SecretString),
    Bip39Mnemonic(SecretString),
}

impl SigningCredential {
    #[must_use]
    pub fn from_family_seed(seed: impl Into<String>) -> Self {
        Self::FamilySeed(SecretString::from(seed.into()))
    }

    pub fn from_bip39_mnemonic(mnemonic: impl Into<String>) -> color_eyre::Result<Self> {
        let mnemonic = mnemonic.into();
        kobe_primitives::Wallet::from_mnemonic(&mnemonic, None)
            .map_err(|e| color_eyre::eyre::eyre!("invalid BIP39 mnemonic: {e}"))?;
        Ok(Self::Bip39Mnemonic(SecretString::from(mnemonic)))
    }

    pub fn wallet(&self) -> color_eyre::Result<xrpl::wallet::Wallet> {
        match self {
            Self::FamilySeed(seed) => wallet_from_family_seed(seed.expose_secret(), 0),
            Self::Bip39Mnemonic(mnemonic) => {
                let wallet = kobe_primitives::Wallet::from_mnemonic(mnemonic.expose_secret(), None)
                    .map_err(|e| color_eyre::eyre::eyre!("BIP39 wallet: {e}"))?;
                let account = kobe_xrpl::Deriver::new(&wallet)
                    .derive(0)
                    .map_err(|e| color_eyre::eyre::eyre!("XRPL derivation: {e}"))?;
                let private_key = account.private_key_hex();

                Ok(xrpl::wallet::Wallet {
                    seed: String::new(),
                    public_key: account.public_key_hex().to_uppercase(),
                    private_key: format!("00{}", private_key.to_uppercase()),
                    classic_address: account.address().to_owned(),
                    sequence: 0,
                })
            }
        }
    }
    pub fn address(&self) -> color_eyre::Result<String> {
        Ok(self.wallet()?.classic_address.clone())
    }
}

impl std::fmt::Debug for SigningCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(match self {
            Self::FamilySeed(_) => "FamilySeed",
            Self::Bip39Mnemonic(_) => "Bip39Mnemonic",
        })
        .field(&"[REDACTED]")
        .finish()
    }
}

/// Generate a new key pair locally (no `wallet_propose` RPC).
///
/// Public RPC / Clio endpoints often omit `master_seed` or reject the method;
/// local generation matches rippled `wallet_propose` semantics for TUI keygen.
pub fn generate_wallet_keys_local(key_type: &str) -> color_eyre::Result<GeneratedWalletKeys> {
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
    let master_seed_hex = hex::encode_upper(entropy);

    Ok(GeneratedWalletKeys {
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

/// Prompts for explicit confirmation before a write on a production chain
/// (`Network::is_production()`: XRPL mainnet or Xahau).
///
/// Returns `true` if the operation should proceed:
/// - Always `true` when `skip_prompt` is set (scripting / `--yes`).
/// - Always `true` when `network` is not production.
/// - `true` only if the user types `y` or `yes` (case-insensitive) otherwise.
///
/// Non-TUI `Send` on production calls this unless the caller passes `skip_prompt`.
pub fn prompt_production_confirmation(
    operation: &str,
    network: &Network,
    skip_prompt: bool,
) -> bool {
    if !network.is_production() || skip_prompt {
        return true;
    }
    eprint!(
        "⚠️  {}: about to execute {operation}. Continue? [y/N] ",
        network.display_name()
    );
    let _ = io::stderr().flush();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

/// Direct Mint memo prefix (`DIRECT_MINTING`) — 8 bytes as lowercase hex.
pub const DIRECT_MINT_MEMO_PREFIX: &str = "4642505266410018";

/// Normalize a Flare/EVM address to 40 lowercase hex chars (no `0x`).
pub fn normalize_eth_address_hex(addr: &str) -> color_eyre::Result<String> {
    let s = addr.trim();
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    if s.len() != 40 {
        color_eyre::eyre::bail!(
            "Flare recipient must be 20 bytes (40 hex chars), got {}",
            s.len()
        );
    }
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        color_eyre::eyre::bail!("Flare recipient must be hex");
    }
    Ok(s.to_ascii_lowercase())
}

/// Build 32-byte Direct Mint `MemoData` hex (no `0x`): prefix + 4 zero bytes + recipient.
pub fn build_direct_mint_memo_data(recipient_eth: &str) -> color_eyre::Result<String> {
    let recipient = normalize_eth_address_hex(recipient_eth)?;
    Ok(format!("{DIRECT_MINT_MEMO_PREFIX}00000000{recipient}"))
}

fn payment_memos_from_memo_data(
    memo_data: Option<&str>,
) -> Option<Vec<xrpl::models::transactions::Memo>> {
    memo_data.map(|data| {
        vec![xrpl::models::transactions::Memo {
            memo_data: Some(data.to_string()),
            memo_format: None,
            memo_type: None,
        }]
    })
}
/// Build the Payment `Amount` from composer input (IOU when currency+issuer, else XRP drops).
fn payment_amount(
    amount_spec: &str,
    iou_currency: Option<&str>,
    iou_issuer: Option<&str>,
) -> color_eyre::Result<xrpl::models::Amount<'static>> {
    use xrpl::models::{Amount, IssuedCurrencyAmount};

    match (iou_currency, iou_issuer) {
        (Some(cur), Some(iss)) => {
            validate_iou_fields(cur, iss, amount_spec)?;
            Ok(Amount::IssuedCurrencyAmount(IssuedCurrencyAmount {
                currency: cur.to_string().into(),
                issuer: iss.to_string().into(),
                value: amount_spec.to_string().into(),
            }))
        }
        (None, None) => Ok(crate::xrpl::xrp_to_drops(amount_spec)?.into()),
        _ => Err(color_eyre::eyre::eyre!(
            "IOU payment requires both currency and issuer"
        )),
    }
}

/// Create, sign, and encode a Payment transaction as a submit-ready blob.
///
/// `amount_spec`: XRP value (XRP mode) or IOU value (IOU mode).
/// `iou_currency`: If Some, triggers IOU mode (e.g. "USD").
/// `iou_issuer`: If Some, issuer address for IOU mode.
#[allow(clippy::too_many_arguments)]
pub fn create_and_sign_payment(
    wallet: &xrpl::wallet::Wallet,
    account: &str,
    destination: &str,
    amount_spec: &str,
    iou_currency: Option<&str>,
    iou_issuer: Option<&str>,
    destination_tag: Option<u32>,
    memo_data: Option<&str>,
    sequence: u32,
    fee_drops: u32,
    last_ledger_sequence: u32,
    _network: &Network,
) -> color_eyre::Result<String> {
    use xrpl::core::binarycodec::encode;
    use xrpl::models::XRPAmount;
    use xrpl::models::transactions::payment::Payment;
    use xrpl::models::transactions::{CommonFields, TransactionType};
    use xrpl::transaction::sign;

    let amount = payment_amount(amount_spec, iou_currency, iou_issuer)?;
    let mut common = CommonFields::from_account(account.to_string())
        .with_transaction_type(TransactionType::Payment)
        .with_sequence(sequence)
        .with_fee(XRPAmount::from(fee_drops.to_string()))
        .with_last_ledger_sequence(last_ledger_sequence);
    if let Some(memos) = payment_memos_from_memo_data(memo_data) {
        common = common.with_memos(memos);
    }
    let mut payment = Payment {
        common_fields: common,
        amount,
        destination: destination.to_string().into(),
        destination_tag,
        ..Default::default()
    };

    sign(&mut payment, wallet, false)
        .map_err(|e| color_eyre::eyre::eyre!("sign error: {:?}", e))?;

    encode(&payment).map_err(|e| color_eyre::eyre::eyre!("encode error: {:?}", e))
}

/// Unsigned Payment JSON for `simulate` (XRP or IOU).
#[allow(clippy::too_many_arguments)]
pub fn build_payment_tx_json_for_simulate(
    account: &str,
    destination: &str,
    amount_spec: &str,
    iou_currency: Option<&str>,
    iou_issuer: Option<&str>,
    destination_tag: Option<u32>,
    memo_data: Option<&str>,
    sequence: u32,
) -> color_eyre::Result<Value> {
    use xrpl::models::transactions::payment::Payment;
    use xrpl::models::transactions::{CommonFields, TransactionType};

    let amount = payment_amount(amount_spec, iou_currency, iou_issuer)?;
    let mut common = CommonFields::from_account(account.to_string())
        .with_transaction_type(TransactionType::Payment)
        .with_sequence(sequence);
    if let Some(memos) = payment_memos_from_memo_data(memo_data) {
        common = common.with_memos(memos);
    }
    let payment = Payment {
        common_fields: common,
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

/// Unsigned SetRegularKey JSON for `simulate`.
/// Pass `regular_key` as `None` (or empty) to clear the existing regular key.
pub fn build_set_regular_key_tx_json_for_simulate(
    account: &str,
    regular_key: Option<&str>,
    sequence: u32,
) -> color_eyre::Result<Value> {
    use xrpl::models::transactions::set_regular_key::SetRegularKey;
    use xrpl::models::transactions::{CommonFields, TransactionType};

    let key = regular_key.map(str::trim).filter(|s| !s.is_empty());
    if let Some(k) = key {
        require_classic_address_shape("regular_key", k)?;
    }

    let tx = SetRegularKey {
        common_fields: CommonFields::from_account(account.to_string())
            .with_transaction_type(TransactionType::SetRegularKey)
            .with_sequence(sequence),
        regular_key: key.map(|k| k.to_string().into()),
    };

    serde_json::to_value(&tx).map_err(|e| color_eyre::eyre::eyre!("set_regular_key tx_json: {e}"))
}

/// Create, sign, and encode a SetRegularKey transaction as a submit-ready blob.
///
/// Pass `regular_key` as `None` to clear (remove) the existing regular key.
pub fn create_and_sign_set_regular_key(
    wallet: &xrpl::wallet::Wallet,
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

    let mut tx = SetRegularKey {
        common_fields: CommonFields::from_account(account.to_string())
            .with_transaction_type(TransactionType::SetRegularKey)
            .with_sequence(sequence)
            .with_fee(XRPAmount::from(fee_drops.to_string()))
            .with_last_ledger_sequence(last_ledger_sequence),
        regular_key: regular_key.map(|k| k.to_string().into()),
    };

    sign(&mut tx, wallet, false).map_err(|e| color_eyre::eyre::eyre!("sign error: {:?}", e))?;

    encode(&tx).map_err(|e| color_eyre::eyre::eyre!("encode error: {:?}", e))
}

/// Shared field helper: trim and require non-empty (composer / TrustSet / Offer fields).
pub(crate) fn require_nonempty_field<'a>(
    label: &str,
    value: &'a str,
) -> color_eyre::Result<&'a str> {
    let t = value.trim();
    if t.is_empty() {
        color_eyre::eyre::bail!("{label}: required");
    }
    Ok(t)
}

/// Validate a classic address using XRPL base58 checksum rules.
pub(crate) fn require_classic_address_shape<'a>(
    label: &str,
    value: &'a str,
) -> color_eyre::Result<&'a str> {
    let t = require_nonempty_field(label, value)?;
    if !xrpl::core::addresscodec::is_valid_classic_address(t) {
        color_eyre::eyre::bail!("{label}: expected valid classic address");
    }
    Ok(t)
}

/// IOU currency codes: 3 ASCII alphanumerics or 40 hex chars; never `XRP`.
fn is_valid_iou_currency(currency: &str) -> bool {
    let shape_ok = (currency.len() == 3 && currency.bytes().all(|b| b.is_ascii_alphanumeric()))
        || (currency.len() == 40 && currency.bytes().all(|b| b.is_ascii_hexdigit()));
    shape_ok && !currency.eq_ignore_ascii_case("XRP")
}

fn validate_iou_fields(currency: &str, issuer: &str, value: &str) -> color_eyre::Result<()> {
    if !is_valid_iou_currency(currency) {
        color_eyre::eyre::bail!("invalid IOU currency code");
    }
    require_classic_address_shape("issuer", issuer)?;
    let value = value
        .parse::<f64>()
        .map_err(|_| color_eyre::eyre::eyre!("IOU value must be numeric"))?;
    if !value.is_finite() || value <= 0.0 {
        color_eyre::eyre::bail!("IOU value must be finite and greater than zero");
    }
    Ok(())
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
        let drops = parts[1]
            .parse::<u64>()
            .map_err(|_| color_eyre::eyre::eyre!("XRP drops must be an integer"))?;
        if drops == 0 {
            color_eyre::eyre::bail!("XRP drops must be greater than zero");
        }
        return Ok(OfferAmountSpec::Xrp(parts[1]));
    }
    if parts.len() < 3 {
        color_eyre::eyre::bail!("IOU amount needs 3 parts (CUR:issuer:value): {spec}");
    }
    let currency = parts[0];
    validate_iou_fields(currency, parts[1], parts[2])?;
    Ok(OfferAmountSpec::Iou {
        currency,
        issuer: parts[1],
        value: parts[2],
    })
}

/// Build an `Amount` from a compact spec string.
/// `"XRP:100000000"` → XRP amount in drops.
/// `"USD:rIssuer:100.5"` → issued currency amount.
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

/// Unsigned OfferCreate JSON for `simulate`.
/// `taker_gets` / `taker_pays` use compact specs: `XRP:drops` or `CUR:issuer:value`.
pub fn build_offer_create_tx_json_for_simulate(
    account: &str,
    taker_gets_spec: &str,
    taker_pays_spec: &str,
    sequence: u32,
) -> color_eyre::Result<Value> {
    use xrpl::models::transactions::offer_create::OfferCreate;
    use xrpl::models::transactions::{CommonFields, TransactionType};

    let taker_gets = parse_offer_amount(taker_gets_spec)?;
    let taker_pays = parse_offer_amount(taker_pays_spec)?;

    let tx = OfferCreate {
        common_fields: CommonFields::from_account(account.to_string())
            .with_transaction_type(TransactionType::OfferCreate)
            .with_sequence(sequence),
        taker_gets,
        taker_pays,
        ..Default::default()
    };

    serde_json::to_value(&tx).map_err(|e| color_eyre::eyre::eyre!("offer_create tx_json: {e}"))
}

/// Create and sign an `OfferCreate` transaction, returning the tx_blob hex.
#[allow(clippy::too_many_arguments)]
pub fn create_and_sign_offer_create(
    wallet: &xrpl::wallet::Wallet,
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

    sign(&mut tx, wallet, false).map_err(|e| color_eyre::eyre::eyre!("sign error: {:?}", e))?;

    encode(&tx).map_err(|e| color_eyre::eyre::eyre!("encode error: {:?}", e))
}

/// Create and sign a TrustSet transaction, returning the tx_blob hex (v1: limit only).
#[allow(clippy::too_many_arguments)]
pub fn create_and_sign_trust_set(
    wallet: &xrpl::wallet::Wallet,
    account: &str,
    currency: &str,
    issuer: &str,
    limit: &str,
    sequence: u32,
    fee_drops: u32,
    last_ledger_sequence: u32,
    _network: &Network,
) -> color_eyre::Result<String> {
    use xrpl::core::binarycodec::encode;
    use xrpl::models::IssuedCurrencyAmount;
    use xrpl::models::XRPAmount;
    use xrpl::models::transactions::trust_set::TrustSet;
    use xrpl::models::transactions::{CommonFields, TransactionType};
    use xrpl::transaction::sign;

    let currency = require_nonempty_field("currency", currency)?;

    let issuer = require_classic_address_shape("issuer", issuer)?;
    let limit = require_nonempty_field("limit", limit)?;

    let mut tx = TrustSet {
        common_fields: CommonFields::from_account(account.to_string())
            .with_transaction_type(TransactionType::TrustSet)
            .with_sequence(sequence)
            .with_fee(XRPAmount::from(fee_drops.to_string()))
            .with_last_ledger_sequence(last_ledger_sequence),
        limit_amount: IssuedCurrencyAmount {
            currency: currency.to_string().into(),
            issuer: issuer.to_string().into(),
            value: limit.to_string().into(),
        },
        ..Default::default()
    };

    sign(&mut tx, wallet, false).map_err(|e| color_eyre::eyre::eyre!("sign error: {:?}", e))?;
    encode(&tx).map_err(|e| color_eyre::eyre::eyre!("encode error: {:?}", e))
}

/// Unsigned TrustSet JSON for `simulate` (v1: Limit + Currency + Issuer only).
pub fn build_trust_set_tx_json_for_simulate(
    account: &str,
    currency: &str,
    issuer: &str,
    limit: &str,
    sequence: u32,
) -> color_eyre::Result<Value> {
    use xrpl::models::IssuedCurrencyAmount;
    use xrpl::models::transactions::trust_set::TrustSet;
    use xrpl::models::transactions::{CommonFields, TransactionType};

    let currency = require_nonempty_field("currency", currency)?;
    let issuer = require_classic_address_shape("issuer", issuer)?;
    let limit = require_nonempty_field("limit", limit)?;

    let tx = TrustSet {
        common_fields: CommonFields::from_account(account.to_string())
            .with_transaction_type(TransactionType::TrustSet)
            .with_sequence(sequence),
        limit_amount: IssuedCurrencyAmount {
            currency: currency.to_string().into(),
            issuer: issuer.to_string().into(),
            value: limit.to_string().into(),
        },
        ..Default::default()
    };

    serde_json::to_value(&tx).map_err(|e| color_eyre::eyre::eyre!("trust_set tx_json: {e}"))
}

/// Lowercase ASCII domain → hex string for `AccountSet.domain`.
#[must_use]
pub fn domain_ascii_to_hex(domain: &str) -> String {
    hex::encode(domain.to_ascii_lowercase())
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
    wallet: &xrpl::wallet::Wallet,
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

    sign(&mut tx, wallet, false).map_err(|e| color_eyre::eyre::eyre!("sign error: {:?}", e))?;

    encode(&tx).map_err(|e| color_eyre::eyre::eyre!("encode error: {:?}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_from_secrets_returns_none_when_no_source() {
        let result = credential_from_secrets(None, None).expect("no source is ok");
        assert!(result.is_none());
    }

    #[test]
    fn credential_from_secrets_wraps_family_seed() {
        let seed = SecretString::from("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB".to_string());
        let cred = credential_from_secrets(Some(&seed), None)
            .expect("seed only")
            .expect("credential");
        match cred {
            SigningCredential::FamilySeed(s) => {
                assert_eq!(s.expose_secret(), "sEdSkooMk31MeTjbHVE7vLvgCpEMAdB");
            }
            other => panic!("expected FamilySeed, got {other:?}"),
        }
    }

    #[test]
    fn credential_from_secrets_wraps_bip39_mnemonic() {
        let mnemonic = SecretString::from(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
                .to_string(),
        );
        let cred = credential_from_secrets(None, Some(&mnemonic))
            .expect("mnemonic only")
            .expect("credential");
        match cred {
            SigningCredential::Bip39Mnemonic(s) => {
                assert!(s.expose_secret().starts_with("abandon"));
            }
            other => panic!("expected Bip39Mnemonic, got {other:?}"),
        }
    }

    #[test]
    fn credential_from_secrets_rejects_seed_and_mnemonic_together() {
        let seed = SecretString::from("sEdSkooMk31MeTjbHVE7vLvgCpEMAdB".to_string());
        let mnemonic = SecretString::from(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
                .to_string(),
        );
        let err =
            credential_from_secrets(Some(&seed), Some(&mnemonic)).expect_err("both configured");
        assert!(format!("{err}").contains("only one"));
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
        let v =
            build_payment_tx_json_for_simulate("rSender", "rDest", "1", None, None, None, None, 7)
                .expect("payment json");
        assert_eq!(v["TransactionType"], "Payment");
        assert_eq!(v["Sequence"], 7);
        assert_eq!(v["Account"], "rSender");
        assert_eq!(v["Destination"], "rDest");
        assert_eq!(v["Amount"], "1000000");
    }

    #[test]
    fn build_payment_tx_json_for_simulate_iou() {
        let issuer = "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN";
        let v = build_payment_tx_json_for_simulate(
            "rSender",
            "rDest",
            "12.5",
            Some("USD"),
            Some(issuer),
            None,
            None,
            9,
        )
        .expect("iou payment json");
        assert_eq!(v["TransactionType"], "Payment");
        assert_eq!(v["Sequence"], 9);
        assert_eq!(v["Account"], "rSender");
        assert_eq!(v["Destination"], "rDest");
        assert_eq!(v["Amount"]["currency"], "USD");
        assert_eq!(v["Amount"]["issuer"], issuer);
        assert_eq!(v["Amount"]["value"], "12.5");
    }

    #[test]
    fn build_payment_tx_json_for_simulate_includes_destination_tag() {
        let v = build_payment_tx_json_for_simulate(
            "rSender",
            "rDest",
            "1",
            None,
            None,
            Some(12345),
            None,
            3,
        )
        .expect("tagged payment json");
        assert_eq!(v["DestinationTag"], 12345);
    }

    #[test]
    fn build_payment_tx_json_rejects_partial_iou() {
        let err = build_payment_tx_json_for_simulate(
            "rSender",
            "rDest",
            "1",
            Some("USD"),
            None,
            None,
            None,
            1,
        )
        .expect_err("partial iou");
        assert!(format!("{err}").contains("both currency and issuer"));
    }

    #[test]
    fn build_direct_mint_memo_data_recipient_only() {
        let memo =
            build_direct_mint_memo_data("0xAbCDEF0123456789AbCDEF0123456789aBcDEF01").unwrap();
        assert_eq!(memo.len(), 64);
        assert!(memo.starts_with(DIRECT_MINT_MEMO_PREFIX));
        assert_eq!(&memo[16..24], "00000000");
        assert_eq!(&memo[24..], "abcdef0123456789abcdef0123456789abcdef01");
    }

    #[test]
    fn build_direct_mint_memo_data_rejects_bad_addr() {
        assert!(build_direct_mint_memo_data("0xabc").is_err());
        assert!(build_direct_mint_memo_data("not-hex-zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_err());
    }

    #[test]
    fn build_payment_tx_json_for_simulate_includes_memo() {
        let memo =
            build_direct_mint_memo_data("0xabcdef0123456789abcdef0123456789abcdef01").unwrap();
        let v = build_payment_tx_json_for_simulate(
            "rSender",
            "rDest",
            "1",
            None,
            None,
            None,
            Some(memo.as_str()),
            9,
        )
        .unwrap();
        let data = v["Memos"][0]["Memo"]["MemoData"].as_str().unwrap();
        assert_eq!(data, memo);
    }

    #[test]
    fn confirm_non_mainnet_skips_prompt() {
        assert!(prompt_production_confirmation(
            "Payment",
            &Network::Testnet,
            false
        ));
        assert!(prompt_production_confirmation(
            "Payment",
            &Network::Devnet,
            false
        ));
    }

    #[test]
    fn confirm_mainnet_with_yes_flag_skips_prompt() {
        assert!(prompt_production_confirmation(
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
    fn generate_wallet_keys_local_ed25519() {
        let r = generate_wallet_keys_local("ed25519").expect("local keygen");
        assert!(r.master_seed.starts_with("sEd"));
        assert!(r.account_id.starts_with('r'));
        assert_eq!(r.key_type, "ed25519");
        assert_eq!(r.master_seed_hex.len(), 32);
        // Roundtrip: the generated seed must decode back to the same account.
        let wallet =
            wallet_from_family_seed(&r.master_seed, 0).expect("wallet from generated seed");
        assert_eq!(wallet.classic_address, r.account_id);
    }
    #[test]
    fn bip39_secp256k1_derives_known_xrpl_wallet() {
        let credential = SigningCredential::from_bip39_mnemonic(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
        )
        .expect("valid BIP39 mnemonic");
        let wallet = credential.wallet().expect("XRPL wallet");

        assert_eq!(
            wallet.private_key,
            "0090802A50AA84EFB6CDB225F17C27616EA94048C179142FECF03F4712A07EA7A4"
        );
        assert_eq!(
            xrpl::core::keypairs::derive_classic_address(&wallet.public_key).unwrap(),
            wallet.classic_address
        );
        // Actual deterministic derivation for this mnemonic (index 0). The
        // often-quoted rG1QQv2... address belongs to a different test vector.
        assert_eq!(wallet.classic_address, "rHsMGQEkVNJmpGWs8XUBoTBiAAbwxZN5v3");
        assert!(xrpl::core::keypairs::sign(b"test", &wallet.private_key).is_ok());
    }
    #[test]
    fn bip39_secp256k1_can_sign_payment() {
        use xrpl::core::binarycodec::decode;

        let credential = SigningCredential::from_bip39_mnemonic(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
        )
        .expect("valid BIP39 mnemonic");
        let wallet = credential.wallet().expect("XRPL wallet");
        let account = wallet.classic_address.clone();
        let blob = create_and_sign_payment(
            &wallet,
            &account,
            "rG1QQv2nh2gr7RCZ1P8YYcBUKCCN633jCn",
            "1",
            None,
            None,
            None,
            None,
            1,
            12,
            100,
            &Network::Mainnet,
        )
        .expect("signed payment");

        // The blob must round-trip through the XRPL binary codec as a Payment
        // carrying the exact request arguments.
        let decoded = decode(&blob).expect("blob decodes via XRPL binary codec");
        assert_eq!(decoded["TransactionType"], serde_json::json!("Payment"));
        assert_eq!(
            decoded["Destination"],
            serde_json::json!("rG1QQv2nh2gr7RCZ1P8YYcBUKCCN633jCn")
        );
        assert_eq!(decoded["Amount"], serde_json::json!("1000000"));
        assert_eq!(decoded["Sequence"], serde_json::json!(1));
        assert_eq!(decoded["Fee"], serde_json::json!("12"));
        assert_eq!(decoded["Account"], serde_json::json!(account));
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
    fn secp256k1_family_seed_known_address() {
        let seed = "sn259rEFXrQrWyx3Q7XneWcwV6dfL";
        let wallet = wallet_from_family_seed(seed, 0).expect("wallet");
        // Known classic address for this fixture seed (not a Wallet::new mirror).
        assert_eq!(wallet.classic_address, "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN");
    }

    #[test]
    fn build_set_regular_key_tx_json_for_simulate_sets_key() {
        let account = "rU3Cw9Vezt3m3E7EonCnfGN1raFdudq4QQ";
        let regular_key = "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN";
        let v = build_set_regular_key_tx_json_for_simulate(account, Some(regular_key), 11)
            .expect("set regular key json");
        assert_eq!(v["TransactionType"], "SetRegularKey");
        assert_eq!(v["Sequence"], 11);
        assert_eq!(v["Account"], account);
        assert_eq!(v["RegularKey"], regular_key);
    }

    #[test]
    fn build_set_regular_key_tx_json_for_simulate_clear_omits_regular_key() {
        let v =
            build_set_regular_key_tx_json_for_simulate("rSenderxxxxxxxxxxxxxxxxxxxxxxxXX", None, 2)
                .expect("clear regular key");
        assert_eq!(v["TransactionType"], "SetRegularKey");
        assert!(v.get("RegularKey").is_none() || v["RegularKey"].is_null());
    }

    #[test]
    fn build_set_regular_key_rejects_bad_regular_key_shape() {
        let err = build_set_regular_key_tx_json_for_simulate(
            "rSenderxxxxxxxxxxxxxxxxxxxxxxxXX",
            Some("not-an-address"),
            1,
        )
        .expect_err("bad key");
        assert!(format!("{err}").contains("classic address"));
    }

    #[test]
    fn build_offer_create_tx_json_for_simulate_xrp_iou() {
        let issuer = "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN";
        let v = build_offer_create_tx_json_for_simulate(
            "rSenderxxxxxxxxxxxxxxxxxxxxxxxXX",
            "XRP:1000000",
            &format!("USD:{issuer}:10"),
            5,
        )
        .expect("offer create json");
        assert_eq!(v["TransactionType"], "OfferCreate");
        assert_eq!(v["Sequence"], 5);
        assert_eq!(v["TakerGets"], "1000000");
        assert_eq!(v["TakerPays"]["currency"], "USD");
        assert_eq!(v["TakerPays"]["issuer"], issuer);
        assert_eq!(v["TakerPays"]["value"], "10");
    }

    #[test]
    fn build_trust_set_tx_json_for_simulate_limit_only() {
        let issuer = "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN";
        let v = build_trust_set_tx_json_for_simulate(
            "rSenderxxxxxxxxxxxxxxxxxxxxxxxXX",
            "USD",
            issuer,
            "1000",
            8,
        )
        .expect("trust set json");
        assert_eq!(v["TransactionType"], "TrustSet");
        assert_eq!(v["Sequence"], 8);
        assert_eq!(v["LimitAmount"]["currency"], "USD");
        assert_eq!(v["LimitAmount"]["issuer"], issuer);
        assert_eq!(v["LimitAmount"]["value"], "1000");
        assert!(v.get("Flags").is_none() || v["Flags"] == 0);
    }

    #[test]
    fn build_trust_set_rejects_empty_currency() {
        let err = build_trust_set_tx_json_for_simulate(
            "rSenderxxxxxxxxxxxxxxxxxxxxxxxXX",
            "  ",
            "rIssuerxxxxxxxxxxxxxxxxxxxxxxxXX",
            "1",
            1,
        )
        .expect_err("empty currency");
        assert!(format!("{err}").contains("currency"));
    }

    #[test]
    fn require_classic_address_shape_accepts_valid_classic_address() {
        let a = require_classic_address_shape("acct", "rN7n7otQDd6FczFgLdSqtcsAUxDkw6fzRH")
            .expect("ok");
        assert!(a.starts_with('r'));
    }

    /// TC-098: IOU issuer checksum and finite positive value validation
    #[test]
    fn iou_validation_rejects_invalid_issuer_and_nonfinite_value() {
        assert!(validate_iou_fields("USD", "rInvalid", "1").is_err());
        assert!(validate_iou_fields("USD", "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN", "NaN").is_err());
        assert!(validate_iou_fields("USD", "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN", "0").is_err());
    }

    /// TC-099: IOU currency code validation
    #[test]
    fn iou_validation_rejects_invalid_currency_code() {
        let issuer = "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN";
        assert!(validate_iou_fields("XRP", issuer, "1").is_err());
        assert!(validate_iou_fields("TOOLONG", issuer, "1").is_err());
        assert!(validate_iou_fields("USD", issuer, "1").is_ok());
    }
}
