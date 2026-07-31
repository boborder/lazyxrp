use std::collections::HashSet;

use alloy::{
    primitives::{Address, FixedBytes, U256, address},
    providers::ProviderBuilder,
    sol,
};

use crate::xrpl::{FlareFeedPrice, FxrpDirectMintInfo};

pub const DEFAULT_FLARE_RPC: &str = "https://flare-api.flare.network/ext/C/rpc";
pub const DEFAULT_FLARE_FEED: &str = "FXRP/USD";
pub const DEFAULT_FLARE_FEEDS: &[&str] = &["FXRP/USD", "FLR/USD", "BTC/USD", "ETH/USD"];
const FLARE_CONTRACT_REGISTRY: Address = address!("aD67FE66660Fb8dFE9d6b1b4240d8650e30F6019");
const ASSET_MANAGER_FXRP_NAME: &str = "AssetManagerFXRP";

sol! {
    #[sol(rpc)]
    interface FlareContractRegistry {
        function getContractAddressByName(string memory _name) external view returns (address);
    }

    #[sol(rpc)]
    interface FtsoV2 {
        function getFeedById(bytes21 _feedId) external view returns (uint256 value, int8 decimals, uint64 timestamp);
    }

    /// Minimal IAssetManager surface for FXRP Direct Mint C1 reads.
    #[sol(rpc)]
    interface IAssetManager {
        function directMintingPaymentAddress() external view returns (string memory);
        function getDirectMintingMinimumFeeUBA() external view returns (uint256);
        function getDirectMintingFeeBIPS() external view returns (uint256);
        function getDirectMintingExecutorFeeUBA() external view returns (uint256);
    }
}

fn to_crypto_feed_id(symbol: &str) -> color_eyre::Result<FixedBytes<21>> {
    if symbol.is_empty() {
        color_eyre::eyre::bail!("feed name is empty");
    }
    let symbol_bytes = symbol.as_bytes();
    if symbol_bytes.len() > 20 {
        color_eyre::eyre::bail!("feed name too long for bytes21: {symbol}");
    }

    let mut feed_id = [0u8; 21];
    feed_id[0] = 0x01; // Crypto category
    feed_id[1..1 + symbol_bytes.len()].copy_from_slice(symbol_bytes);
    Ok(FixedBytes::<21>::from(feed_id))
}

fn format_price(value: U256, decimals: i8) -> String {
    let divisor = 10_f64.powi(decimals as i32);
    let human = value.to::<u128>() as f64 / divisor;
    format!("{human:.6}")
}

/// Stable UBA → XRP display (6 decimals like drops; trailing zeros trimmed).
#[must_use]
pub fn uba_to_xrp_display(uba: u128) -> String {
    let whole = uba / 1_000_000;
    let frac = uba % 1_000_000;
    if frac == 0 {
        format!("{whole}")
    } else {
        let s = format!("{whole}.{frac:06}");
        s.trim_end_matches('0').to_string()
    }
}

/// BIPS → percent string (`10` → `"0.10%"`, `100` → `"1.00%"`).
#[must_use]
pub fn bips_to_percent_display(bips: u64) -> String {
    let whole = bips / 100;
    let frac = bips % 100;
    format!("{whole}.{frac:02}%")
}

async fn fetch_from_rpc(
    rpc_url: &str,
    feeds: &[String],
) -> color_eyre::Result<Vec<FlareFeedPrice>> {
    let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
    let registry = FlareContractRegistry::new(FLARE_CONTRACT_REGISTRY, provider.clone());
    let ftso_address = registry
        .getContractAddressByName("FtsoV2".to_string())
        .call()
        .await?;
    let ftso = FtsoV2::new(ftso_address, provider);

    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for requested in feeds {
        let mut pair = requested.trim().to_string();
        if pair.is_empty() {
            continue;
        }
        let mut feed_id = match to_crypto_feed_id(&pair) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let result = match ftso.getFeedById(feed_id).call().await {
            Ok(v) => v,
            Err(err)
                if pair == DEFAULT_FLARE_FEED
                    && err.to_string().contains("feed does not exist") =>
            {
                pair = "XRP/USD".to_string();
                feed_id = to_crypto_feed_id(&pair)?;
                match ftso.getFeedById(feed_id).call().await {
                    Ok(v) => v,
                    Err(_) => continue,
                }
            }
            Err(_) => continue,
        };

        if !seen.insert(pair.clone()) {
            continue;
        }
        out.push(FlareFeedPrice {
            pair,
            price: format_price(result.value, result.decimals),
            timestamp: result.timestamp,
            source: "FLARE-FTSO".to_string(),
        });
    }

    if out.is_empty() {
        color_eyre::eyre::bail!("flare ftso: no feeds fetched from {rpc_url}");
    }

    Ok(out)
}

pub async fn fetch_ftso_prices(
    rpc_url: &str,
    feeds: &[String],
) -> color_eyre::Result<Vec<FlareFeedPrice>> {
    fetch_from_rpc(rpc_url, feeds).await
}

/// Resolve AssetManagerFXRP and read Core Vault + direct-mint fee views (read-only).
pub async fn fetch_fxrp_direct_mint_info(
    rpc_url: &str,
) -> color_eyre::Result<FxrpDirectMintInfo> {
    let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
    let registry = FlareContractRegistry::new(FLARE_CONTRACT_REGISTRY, provider.clone());
    let asset_manager_addr = registry
        .getContractAddressByName(ASSET_MANAGER_FXRP_NAME.to_string())
        .call()
        .await?;
    let am = IAssetManager::new(asset_manager_addr, provider);

    let core_vault_xrpl = am.directMintingPaymentAddress().call().await?;
    let min_fee = am.getDirectMintingMinimumFeeUBA().call().await?;
    let fee_bips = am.getDirectMintingFeeBIPS().call().await?;
    let executor_fee = am.getDirectMintingExecutorFeeUBA().call().await?;

    Ok(FxrpDirectMintInfo {
        core_vault_xrpl,
        asset_manager: format!("{asset_manager_addr:#x}"),
        min_fee_uba: min_fee.to::<u128>(),
        fee_bips: fee_bips.to::<u64>(),
        executor_fee_uba: executor_fee.to::<u128>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uba_to_xrp_display_table() {
        assert_eq!(uba_to_xrp_display(0), "0");
        assert_eq!(uba_to_xrp_display(100_000), "0.1");
        assert_eq!(uba_to_xrp_display(200_000), "0.2");
        assert_eq!(uba_to_xrp_display(1_000_000), "1");
        assert_eq!(uba_to_xrp_display(10_500_000), "10.5");
        assert_eq!(uba_to_xrp_display(1), "0.000001");
    }

    #[test]
    fn bips_to_percent_display_table() {
        assert_eq!(bips_to_percent_display(0), "0.00%");
        assert_eq!(bips_to_percent_display(10), "0.10%");
        assert_eq!(bips_to_percent_display(100), "1.00%");
        assert_eq!(bips_to_percent_display(12), "0.12%");
    }

    #[tokio::test]
    #[ignore = "live network dependency"]
    async fn flare_default_feeds_fetch_live() -> color_eyre::Result<()> {
        let feeds: Vec<String> = DEFAULT_FLARE_FEEDS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let prices = fetch_ftso_prices(DEFAULT_FLARE_RPC, &feeds).await?;

        assert!(!prices.is_empty());
        assert!(prices.iter().any(|p| p.pair == "FLR/USD"));
        assert!(prices.iter().any(|p| p.pair == "BTC/USD"));
        assert!(prices.iter().any(|p| p.pair == "ETH/USD"));

        Ok(())
    }

    #[tokio::test]
    #[ignore = "live network dependency"]
    async fn fxrp_direct_mint_info_fetch_live() -> color_eyre::Result<()> {
        let info = fetch_fxrp_direct_mint_info(DEFAULT_FLARE_RPC).await?;
        assert!(
            info.core_vault_xrpl.starts_with('r'),
            "core vault should be classic XRPL addr: {}",
            info.core_vault_xrpl
        );
        assert!(info.asset_manager.starts_with("0x"));
        assert!(info.min_fee_uba > 0);
        Ok(())
    }
}
