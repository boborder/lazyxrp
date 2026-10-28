use super::*;

pub(super) fn default_flare_evm_key_env() -> String {
    "FLARE_EVM_KEY".to_string()
}

impl Default for FlareFassetsConfig {
    fn default() -> Self {
        Self {
            execute: false,
            evm_key_env: default_flare_evm_key_env(),
            expected_core_vault: None,
        }
    }
}

pub const FALLBACK_CURRENCY_CODE: &str = "USD";
pub const FALLBACK_ISSUER: &str = "rvYAfWj5gh67oV6fW32ZzP3Aw4Eubs59B";

pub(super) fn fallback_issuer() -> String {
    FALLBACK_ISSUER.to_string()
}

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
            account: "r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV".to_string(),
            issuer: "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De".to_string(),
            currency: "RLUSD".to_string(),
            currency_code: None,
            offer_limit: 5,
            poll_interval_ms: 5_000,
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
