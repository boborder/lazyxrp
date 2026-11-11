use super::*;

pub(crate) const DEFAULT_FLARE_EVM_KEY_ENV: &str = "FLARE_EVM_KEY";

pub(super) fn default_flare_evm_key_env() -> String {
    DEFAULT_FLARE_EVM_KEY_ENV.to_string()
}

/// Serde defaults for `[xrpl]`: a user file that sets only `network` (or only `currency`)
/// must merge with the built-in defaults instead of failing on a missing field.
pub(super) fn default_account() -> String {
    "r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV".to_string()
}

pub(super) fn default_currency() -> String {
    "RLUSD".to_string()
}

/// Default issuer for `[xrpl] currency` (RLUSD's issuer). Must match the no-table
/// `LedgerConfig::default()`; `FALLBACK_ISSUER` is only the blank-issuer book fallback.
pub(super) fn default_issuer() -> String {
    "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De".to_string()
}

pub(super) fn default_offer_limit() -> u16 {
    5
}

pub(super) fn default_poll_interval_ms() -> u64 {
    5_000
}

impl Default for FlareFassetsConfig {
    fn default() -> Self {
        Self {
            execute: false,
            evm_key_env: default_flare_evm_key_env(),
            expected_core_vault: None,
            expected_tag_manager: None,
        }
    }
}

pub const FALLBACK_CURRENCY_CODE: &str = "USD";
pub const FALLBACK_ISSUER: &str = "rvYAfWj5gh67oV6fW32ZzP3Aw4Eubs59B";

pub(super) fn default_oracle_pairs() -> Vec<crate::xrpl::OraclePricePair> {
    vec![
        crate::xrpl::OraclePricePair {
            base_asset: "XRP".into(),
            quote_asset: "USD".into(),
        },
        crate::xrpl::OraclePricePair {
            base_asset: "BTC".into(),
            quote_asset: "USD".into(),
        },
        crate::xrpl::OraclePricePair {
            base_asset: "ETH".into(),
            quote_asset: "USD".into(),
        },
        crate::xrpl::OraclePricePair {
            base_asset: "524C555344000000000000000000000000000000".into(),
            quote_asset: "USD".into(),
        },
        crate::xrpl::OraclePricePair {
            base_asset: "5553444300000000000000000000000000000000".into(),
            quote_asset: "USD".into(),
        },
        crate::xrpl::OraclePricePair {
            base_asset: "5553445400000000000000000000000000000000".into(),
            quote_asset: "USD".into(),
        },
    ]
}

pub(super) fn default_slippage_bps() -> u32 {
    100
}

impl Default for LedgerConfig {
    fn default() -> Self {
        Self {
            account: default_account(),
            issuer: default_issuer(),
            currency: default_currency(),
            currency_code: None,
            offer_limit: default_offer_limit(),
            poll_interval_ms: default_poll_interval_ms(),
            network: Network::default(),
            rpc_server: None,
            ws_server: None,
            signing: RawSigningConfig::default(),
            oracles: Vec::new(),
            oracle_pairs: default_oracle_pairs(),
            slippage_bps: default_slippage_bps(),
        }
    }
}
