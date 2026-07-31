use std::collections::HashSet;

use alloy::{
    network::EthereumWallet,
    primitives::{Address, Bytes, FixedBytes, U256, address},
    providers::ProviderBuilder,
    signers::local::PrivateKeySigner,
    sol,
};
use serde_json::Value;

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

    /// Minimal IAssetManager surface for FXRP Direct Mint (C1 reads + C3 execute).
    #[sol(rpc)]
    interface IAssetManager {
        function directMintingPaymentAddress() external view returns (string memory);
        function getDirectMintingMinimumFeeUBA() external view returns (uint256);
        function getDirectMintingFeeBIPS() external view returns (uint256);
        function getDirectMintingExecutorFeeUBA() external view returns (uint256);

        struct DirectMintingProof {
            bytes32[] merkleProof;
            bytes data;
        }

        function executeDirectMinting(DirectMintingProof _proof) external returns (uint256);
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

/// Refuse Flare writes unless `[flare.fassets] execute = true`.
pub fn ensure_fassets_execute_enabled(execute: bool) -> color_eyre::Result<()> {
    if !execute {
        color_eyre::eyre::bail!(
            "flare fassets execute is disabled; set [flare.fassets] execute = true to allow executeDirectMinting"
        );
    }
    Ok(())
}

/// Parsed FDC Payment attestation proof for `executeDirectMinting`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FdcPaymentProof {
    pub merkle_proof: Vec<FixedBytes<32>>,
    pub data: Bytes,
}

fn parse_hex_bytes32(raw: &str) -> color_eyre::Result<FixedBytes<32>> {
    let parsed: FixedBytes<32> = raw
        .trim()
        .parse()
        .map_err(|e| color_eyre::eyre::eyre!("invalid bytes32 hex `{raw}`: {e}"))?;
    Ok(parsed)
}

fn parse_hex_bytes(raw: &str) -> color_eyre::Result<Bytes> {
    let parsed: Bytes = raw
        .trim()
        .parse()
        .map_err(|e| color_eyre::eyre::eyre!("invalid bytes hex: {e}"))?;
    Ok(parsed)
}

fn json_hex_array(value: &Value, field: &str) -> color_eyre::Result<Vec<FixedBytes<32>>> {
    let arr = value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| color_eyre::eyre::eyre!("proof JSON missing `{field}` array"))?;
    if arr.is_empty() {
        color_eyre::eyre::bail!("proof JSON `{field}` must be a non-empty array");
    }
    arr.iter()
        .map(|v| {
            let s = v
                .as_str()
                .ok_or_else(|| color_eyre::eyre::eyre!("`{field}` entries must be hex strings"))?;
            parse_hex_bytes32(s)
        })
        .collect()
}

fn json_hex_bytes(value: &Value, field: &str) -> color_eyre::Result<Bytes> {
    let s = value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| color_eyre::eyre::eyre!("proof JSON missing `{field}` hex string"))?;
    parse_hex_bytes(s)
}

/// Accept FDC DA `{proof,response}` or contract-shaped `{merkleProof,data}`.
pub fn parse_fdc_payment_proof_json(proof_json: &str) -> color_eyre::Result<FdcPaymentProof> {
    let value: Value = serde_json::from_str(proof_json.trim())
        .map_err(|e| color_eyre::eyre::eyre!("invalid FDC proof JSON: {e}"))?;
    if !value.is_object() {
        color_eyre::eyre::bail!("FDC proof JSON must be an object");
    }

    let (merkle_proof, data) = if value.get("merkleProof").is_some() || value.get("data").is_some()
    {
        (
            json_hex_array(&value, "merkleProof")?,
            json_hex_bytes(&value, "data")?,
        )
    } else if value.get("proof").is_some() || value.get("response").is_some() {
        (
            json_hex_array(&value, "proof")?,
            json_hex_bytes(&value, "response")?,
        )
    } else {
        color_eyre::eyre::bail!(
            "FDC proof JSON needs `merkleProof`+`data` or DA-layer `proof`+`response`"
        );
    };

    Ok(FdcPaymentProof { merkle_proof, data })
}

/// Submit `AssetManagerFXRP.executeDirectMinting` with an alloy wallet.
///
/// Returns the Flare transaction hash (`0x…`). Does **not** fetch FDC proofs over HTTP.
pub async fn execute_direct_minting(
    rpc_url: &str,
    private_key_hex: &str,
    proof_json: &str,
) -> color_eyre::Result<String> {
    let proof = parse_fdc_payment_proof_json(proof_json)?;
    let key = private_key_hex.trim();
    let key = if key.starts_with("0x") || key.starts_with("0X") {
        key.to_string()
    } else {
        format!("0x{key}")
    };
    let signer: PrivateKeySigner = key
        .parse()
        .map_err(|e| color_eyre::eyre::eyre!("invalid Flare EVM private key: {e}"))?;
    let wallet = EthereumWallet::from(signer);
    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(rpc_url.parse()?);

    let registry = FlareContractRegistry::new(FLARE_CONTRACT_REGISTRY, provider.clone());
    let asset_manager_addr = registry
        .getContractAddressByName(ASSET_MANAGER_FXRP_NAME.to_string())
        .call()
        .await?;
    let am = IAssetManager::new(asset_manager_addr, provider);

    let arg = IAssetManager::DirectMintingProof {
        merkleProof: proof.merkle_proof,
        data: proof.data,
    };
    let pending = am
        .executeDirectMinting(arg)
        .send()
        .await
        .map_err(|e| color_eyre::eyre::eyre!("executeDirectMinting send failed: {e}"))?;
    let tx_hash = *pending.tx_hash();
    let _receipt = pending
        .get_receipt()
        .await
        .map_err(|e| color_eyre::eyre::eyre!("executeDirectMinting receipt failed: {e}"))?;
    Ok(format!("{tx_hash:#x}"))
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

    #[test]
    fn ensure_fassets_execute_enabled_gate() {
        assert!(ensure_fassets_execute_enabled(false).is_err());
        assert!(ensure_fassets_execute_enabled(true).is_ok());
    }

    #[test]
    fn parse_fdc_payment_proof_json_da_shape() {
        let leaf = format!("0x{}", "ab".repeat(32));
        let data = format!("0x{}", "cd".repeat(8));
        let json = format!(r#"{{"proof":["{leaf}"],"response":"{data}"}}"#);
        let proof = parse_fdc_payment_proof_json(&json).expect("parse da");
        assert_eq!(proof.merkle_proof.len(), 1);
        assert_eq!(proof.data.len(), 8);
    }

    #[test]
    fn parse_fdc_payment_proof_json_contract_shape() {
        let leaf = format!("0x{}", "11".repeat(32));
        let data = format!("0x{}", "22".repeat(4));
        let json = format!(r#"{{"merkleProof":["{leaf}"],"data":"{data}"}}"#);
        let proof = parse_fdc_payment_proof_json(&json).expect("parse contract");
        assert_eq!(proof.merkle_proof.len(), 1);
        assert_eq!(proof.data.len(), 4);
    }

    #[test]
    fn parse_fdc_payment_proof_json_rejects_empty() {
        assert!(parse_fdc_payment_proof_json("{}").is_err());
        assert!(parse_fdc_payment_proof_json(r#"{"proof":[],"response":"0x"}"#).is_err());
        assert!(parse_fdc_payment_proof_json("not-json").is_err());
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
