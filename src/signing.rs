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

pub const SEED_ENV: &str = "XRPL_SEED";

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

/// Resolved signing credentials. Seed is memory-masked via `secrecy`.
///
/// Used for write paths that need a seed (e.g. CLI `Send` via `XRPL_SEED` or
/// `config.toml [xrpl.signing] seed`). `load` clears `XRPL_SEED` from the
/// process environment immediately after reading it.
#[allow(dead_code)]
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

    #[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
pub fn create_and_sign_payment(
    seed: &SecretString,
    account: &str,
    destination: &str,
    amount_xrp: &str,
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

    let amount_drops = crate::xrpl::xrp_to_drops(amount_xrp)?;
    let amount: Amount = amount_drops.into();

    let mut payment = Payment {
        common_fields: CommonFields::from_account(account.to_string())
            .with_transaction_type(TransactionType::Payment)
            .with_sequence(sequence)
            .with_fee(XRPAmount::from(fee_drops.to_string()))
            .with_last_ledger_sequence(last_ledger_sequence),
        amount,
        destination: destination.to_string().into(),
        ..Default::default()
    };

    sign(&mut payment, &wallet, false)
        .map_err(|e| color_eyre::eyre::eyre!("sign error: {:?}", e))?;

    encode(&payment).map_err(|e| color_eyre::eyre::eyre!("encode error: {:?}", e))
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
        let cfg = SigningConfig::prime_seed_source(None);
        assert!(!cfg.has_seed());
    }

    #[test]
    fn resolve_seed_from_config_raw() {
        let _g = env_lock();
        let _env = TestEnvGuard::new(&[SEED_ENV]);
        _env.remove(SEED_ENV);
        let cfg = SigningConfig::prime_seed_source(Some("sTest1234".to_string()));
        assert!(cfg.has_seed());
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
    fn ed25519_family_seed_wallet_new_ok() {
        let seed = "sEdSkooMk31MeTjbHVE7vLvgCpEMAdB";
        let w = wallet_from_family_seed(seed, 0).expect("ed25519 wallet");
        assert!(w.classic_address.starts_with('r'));
    }

    #[test]
    fn ed25519_seed_trims_whitespace() {
        let seed = "sEdSkooMk31MeTjbHVE7vLvgCpEMAdB  \n";
        let w = wallet_from_family_seed(seed, 0).expect("trimmed ed25519");
        assert!(w.classic_address.starts_with('r'));
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
        let cfg = SigningConfig::prime_seed_source(Some("sFromConfigIgnored".to_string()));
        assert!(cfg.has_seed());
        assert_eq!(
            cfg.seed.as_ref().unwrap().expose_secret(),
            "sFromEnvOverride"
        );
    }
}
