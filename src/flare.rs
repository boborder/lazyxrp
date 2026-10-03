use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use alloy::{
    network::EthereumWallet,
    primitives::{Address, Bytes, FixedBytes, U256, address},
    providers::{Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    sol,
};
use serde_json::Value;

use crate::xrpl::{FlareFeedPrice, FlareWalletSummary, FxrpDirectMintInfo};

pub const DEFAULT_FLARE_RPC: &str = "https://flare-api.flare.network/ext/C/rpc";
pub const DEFAULT_FLARE_FEED: &str = "FXRP/USD";
pub const DEFAULT_FLARE_FEEDS: &[&str] = &["FXRP/USD", "FLR/USD", "BTC/USD", "ETH/USD"];
const FLARE_CONTRACT_REGISTRY: Address = address!("aD67FE66660Fb8dFE9d6b1b4240d8650e30F6019");
const ASSET_MANAGER_FXRP_NAME: &str = "AssetManagerFXRP";
const FTSO_V2_REGISTRY_NAME: &str = "FtsoV2";

#[derive(Default)]
struct FlareRegistryAddresses {
    ftso_v2: Option<Address>,
    asset_manager_fxrp: Option<Address>,
}

fn flare_registry_cache() -> &'static Mutex<HashMap<String, FlareRegistryAddresses>> {
    static CACHE: OnceLock<Mutex<HashMap<String, FlareRegistryAddresses>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn resolve_registry_contract<P: Provider + Clone>(
    provider: P,
    rpc_url: &str,
    contract_name: &str,
) -> color_eyre::Result<Address> {
    if let Ok(guard) = flare_registry_cache().lock()
        && let Some(entry) = guard.get(rpc_url)
    {
        let cached = match contract_name {
            FTSO_V2_REGISTRY_NAME => entry.ftso_v2,
            ASSET_MANAGER_FXRP_NAME => entry.asset_manager_fxrp,
            _ => None,
        };
        if let Some(addr) = cached {
            return Ok(addr);
        }
    }

    let registry = FlareContractRegistry::new(FLARE_CONTRACT_REGISTRY, provider);
    let addr = registry
        .getContractAddressByName(contract_name.to_string())
        .call()
        .await?;

    if let Ok(mut guard) = flare_registry_cache().lock() {
        let entry = guard.entry(rpc_url.to_string()).or_default();
        match contract_name {
            FTSO_V2_REGISTRY_NAME => entry.ftso_v2 = Some(addr),
            ASSET_MANAGER_FXRP_NAME => entry.asset_manager_fxrp = Some(addr),
            _ => {}
        }
    }

    Ok(addr)
}

async fn resolve_ftso_v2_address<P: Provider + Clone>(
    provider: P,
    rpc_url: &str,
) -> color_eyre::Result<Address> {
    resolve_registry_contract(provider, rpc_url, FTSO_V2_REGISTRY_NAME).await
}

async fn resolve_asset_manager_fxrp_address<P: Provider + Clone>(
    provider: P,
    rpc_url: &str,
) -> color_eyre::Result<Address> {
    resolve_registry_contract(provider, rpc_url, ASSET_MANAGER_FXRP_NAME).await
}

sol! {
    #[sol(rpc)]
    interface FlareContractRegistry {
        function getContractAddressByName(string memory _name) external view returns (address);
    }

    #[sol(rpc)]
    interface FtsoV2 {
        function getFeedById(bytes21 _feedId) external view returns (uint256 value, int8 decimals, uint64 timestamp);
        function getFeedsById(bytes21[] _feedIds) external view returns (uint256[] values, int8[] decimals, uint64 timestamp);
        function calculateFeeByIds(bytes21[] _feedIds) external view returns (uint256 fee);
    }

    /// Minimal IAssetManager surface for FXRP Direct Mint (C1 reads + C3 execute).
    #[sol(rpc)]
    interface IAssetManager {
        function directMintingPaymentAddress() external view returns (string memory);
        function getDirectMintingMinimumFeeUBA() external view returns (uint256);
        function getDirectMintingFeeBIPS() external view returns (uint256);
        function getDirectMintingExecutorFeeUBA() external view returns (uint256);
        function fAsset() external view returns (address);

        struct DirectMintingProof {
            bytes32[] merkleProof;
            bytes data;
        }

        function executeDirectMinting(DirectMintingProof _proof) external returns (uint256);
    }

    #[sol(rpc)]
    interface IERC20 {
        function balanceOf(address account) external view returns (uint256);
        function decimals() external view returns (uint8);
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

/// Normalize configured feed names: trim, drop empties, map `FXRP/USD` → `XRP/USD`
/// (FTSOv2 indexes the underlying XRP), dedup preserving order.
fn to_valid_flare_pairs(feeds: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for feed in feeds {
        let pair = feed.trim();
        if pair.is_empty() {
            continue;
        }
        let pair = if pair == DEFAULT_FLARE_FEED {
            "XRP/USD"
        } else {
            pair
        };
        if seen.insert(pair.to_string()) {
            out.push(pair.to_string());
        }
    }
    out
}

fn format_price(value: U256, decimals: i8) -> String {
    let divisor = 10_f64.powi(decimals as i32);
    let human = value.to::<u128>() as f64 / divisor;
    format!("{human:.6}")
}

fn format_token_balance(value: U256, decimals: u8) -> String {
    let divisor = 10_f64.powi(decimals as i32);
    let human = value.to::<u128>() as f64 / divisor;
    if decimals <= 6 {
        format!("{human:.6}")
    } else {
        format!("{human:.4}")
    }
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
    let ftso_address = resolve_ftso_v2_address(provider.clone(), rpc_url).await?;
    let ftso = FtsoV2::new(ftso_address, provider);

    let feed_ids: Vec<(String, FixedBytes<21>)> = to_valid_flare_pairs(feeds)
        .into_iter()
        .filter_map(|pair| to_crypto_feed_id(&pair).ok().map(|id| (pair, id)))
        .collect();

    // One batched round trip for all feeds. On revert, retry once with the
    // FtsoV2-calculated fee (eth_call simulation only — no funds move), then
    // fall back to per-feed reads (e.g. a bad feed id in the config).
    // ponytail: fee re-queried on every revert; add a TTL cache if Flare
    // activates fees and this per-poll path grows round trips.
    let ids: Vec<FixedBytes<21>> = feed_ids.iter().map(|(_, id)| *id).collect();
    let batch = match ftso.getFeedsById(ids.clone()).call().await {
        Ok(result) => Some(result),
        Err(_) => {
            let fee = ftso.calculateFeeByIds(ids.clone()).call().await.ok();
            match fee {
                Some(fee) if !fee.is_zero() => ftso.getFeedsById(ids).value(fee).call().await.ok(),
                _ => None,
            }
        }
    };
    if let Some(result) = batch {
        let out = feed_ids
            .iter()
            .zip(result.values)
            .zip(result.decimals)
            .map(|(((pair, _), value), decimals)| FlareFeedPrice {
                pair: pair.clone(),
                price: format_price(value, decimals),
                timestamp: result.timestamp,
                source: "FLARE-FTSO".to_string(),
            })
            .collect::<Vec<_>>();
        if !out.is_empty() {
            return Ok(out);
        }
    }

    let mut out = Vec::new();
    for (pair, feed_id) in &feed_ids {
        let Ok(result) = ftso.getFeedById(*feed_id).call().await else {
            continue;
        };
        out.push(FlareFeedPrice {
            pair: pair.clone(),
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

/// Read native FLR + FXRP ERC20 balances for the Overview wallet panel.
pub async fn fetch_flare_wallet_balance(
    rpc_url: &str,
    address: &str,
    execute_enabled: bool,
    evm_key_env: &str,
) -> color_eyre::Result<FlareWalletSummary> {
    let wallet: Address = address
        .parse()
        .map_err(|e| color_eyre::eyre::eyre!("invalid flare wallet address `{address}`: {e}"))?;
    let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
    let native = provider.get_balance(wallet).await?;
    let native_balance_display = format_token_balance(native, 18);

    let asset_manager_addr = resolve_asset_manager_fxrp_address(provider.clone(), rpc_url).await?;
    let am = IAssetManager::new(asset_manager_addr, provider.clone());
    let fxrp_token = am.fAsset().call().await?;
    let token = IERC20::new(fxrp_token, provider);
    let (fxrp_balance, decimals) =
        tokio::try_join!(async { token.balanceOf(wallet).call().await }, async {
            token.decimals().call().await
        },)?;

    let executor_key_configured = std::env::var(evm_key_env)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);

    Ok(FlareWalletSummary {
        address: format!("{wallet:#x}"),
        native_balance_display,
        fxrp_balance_display: format_token_balance(fxrp_balance, decimals),
        execute_enabled,
        executor_key_configured,
    })
}

/// Resolve AssetManagerFXRP and read Core Vault + direct-mint fee views (read-only).
pub async fn fetch_fxrp_direct_mint_info(rpc_url: &str) -> color_eyre::Result<FxrpDirectMintInfo> {
    let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
    let asset_manager_addr = resolve_asset_manager_fxrp_address(provider.clone(), rpc_url).await?;
    let am = IAssetManager::new(asset_manager_addr, provider);

    let (core_vault_xrpl, min_fee, fee_bips, executor_fee) = tokio::try_join!(
        async { am.directMintingPaymentAddress().call().await },
        async { am.getDirectMintingMinimumFeeUBA().call().await },
        async { am.getDirectMintingFeeBIPS().call().await },
        async { am.getDirectMintingExecutorFeeUBA().call().await },
    )?;

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

    let asset_manager_addr = resolve_asset_manager_fxrp_address(provider.clone(), rpc_url).await?;
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
    fn to_valid_flare_pairs_normalizes_and_dedups() {
        let feeds = vec![
            " FXRP/USD ".to_string(),
            String::new(),
            "XRP/USD".to_string(),
            "FLR/USD".to_string(),
            "  ".to_string(),
            "FLR/USD".to_string(),
        ];
        assert_eq!(
            to_valid_flare_pairs(&feeds),
            vec!["XRP/USD".to_string(), "FLR/USD".to_string()]
        );
    }

    #[test]
    /// TC: FDC proof JSON accepts both DA `{proof,response}` and contract `{merkleProof,data}` field aliases.
    fn parse_fdc_payment_proof_json_accepts_da_and_contract_shapes() {
        let cases = [
            // (json, merkle_proof_len, data_len)
            (r#"{"proof":["0x{leaf}"],"response":"0x{data}"}"#, 1, 8),
            (r#"{"merkleProof":["0x{leaf}"],"data":"0x{data}"}"#, 1, 4),
        ];
        let leaves = ["ab".repeat(32), "11".repeat(32)];
        let datas = ["cd".repeat(8), "22".repeat(4)];
        for ((tpl, proof_len, data_len), (leaf, data)) in
            cases.iter().zip(leaves.iter().zip(datas.iter()))
        {
            let json = tpl.replace("{leaf}", leaf).replace("{data}", data);
            let proof = parse_fdc_payment_proof_json(&json).expect("parse FDC payment proof");
            assert_eq!(proof.merkle_proof.len(), *proof_len);
            assert_eq!(proof.data.len(), *data_len);
        }
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
        assert!(prices.iter().any(|p| p.pair == "XRP/USD"));

        Ok(())
    }

    /// Unknown feed id: batch reverts, fee query is attempted, per-feed
    /// fallback skips it, and the empty result is a clean error (no panic).
    #[tokio::test]
    #[ignore = "live network dependency"]
    async fn flare_unknown_feed_fails_cleanly_live() {
        let feeds = vec!["NOPE/USD".to_string()];
        assert!(fetch_ftso_prices(DEFAULT_FLARE_RPC, &feeds).await.is_err());
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
