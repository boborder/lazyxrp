use std::collections::HashSet;

use alloy::{
    primitives::{Address, FixedBytes, U256, address},
    providers::ProviderBuilder,
    sol,
};

use crate::xrpl::FlareFeedPrice;

pub const DEFAULT_FLARE_RPC: &str = "https://flare-api.flare.network/ext/C/rpc";
pub const DEFAULT_FLARE_FEED: &str = "FXRP/USD";
pub const DEFAULT_FLARE_FEEDS: &[&str] = &["FXRP/USD", "FLR/USD", "BTC/USD", "ETH/USD"];
const FLARE_CONTRACT_REGISTRY: Address = address!("aD67FE66660Fb8dFE9d6b1b4240d8650e30F6019");

sol! {
    #[sol(rpc)]
    interface FlareContractRegistry {
        function getContractAddressByName(string memory _name) external view returns (address);
    }

    #[sol(rpc)]
    interface FtsoV2 {
        function getFeedById(bytes21 _feedId) external view returns (uint256 value, int8 decimals, uint64 timestamp);
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
