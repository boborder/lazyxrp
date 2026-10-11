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

        // `IXRPPayment` structs from the Flare periphery contracts (field order = ABI order).
        struct XrpPaymentRequestBody {
            bytes32 transactionId;
            address proofOwner;
        }
        struct XrpPaymentResponseBody {
            uint64 blockNumber;
            uint64 blockTimestamp;
            string sourceAddress;
            bytes32 sourceAddressHash;
            bytes32 receivingAddressHash;
            bytes32 intendedReceivingAddressHash;
            int256 spentAmount;
            int256 intendedSpentAmount;
            int256 receivedAmount;
            int256 intendedReceivedAmount;
            bool hasMemoData;
            bytes firstMemoData;
            bool hasDestinationTag;
            uint256 destinationTag;
            uint8 status;
        }
        struct XrpPaymentResponse {
            bytes32 attestationType;
            bytes32 sourceId;
            uint64 votingRound;
            uint64 lowestUsedTimestamp;
            XrpPaymentRequestBody requestBody;
            XrpPaymentResponseBody responseBody;
        }
        struct XrpPaymentProof {
            bytes32[] merkleProof;
            XrpPaymentResponse data;
        }

        function executeDirectMinting(XrpPaymentProof _payment) external payable;
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
    let human = value.saturating_to::<u128>() as f64 / divisor;
    format!("{human:.6}")
}

fn format_token_balance(value: U256, decimals: u8) -> String {
    let divisor = 10_f64.powi(decimals as i32);
    let human = value.saturating_to::<u128>() as f64 / divisor;
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
    executor_key_configured: bool,
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

    // Remote values: a checked conversion, never a panicking one (the panic hook exits the TUI).
    let too_big = |what: &str| color_eyre::eyre::eyre!("Flare RPC returned out-of-range {what}");
    Ok(FxrpDirectMintInfo {
        core_vault_xrpl,
        asset_manager: format!("{asset_manager_addr:#x}"),
        min_fee_uba: min_fee.try_into().map_err(|_| too_big("minimum fee"))?,
        fee_bips: fee_bips.try_into().map_err(|_| too_big("fee bips"))?,
        executor_fee_uba: executor_fee
            .try_into()
            .map_err(|_| too_big("executor fee"))?,
    })
}

/// The Core Vault comes from one Flare RPC answer; sign a Payment to it only when it equals the
/// operator-pinned `[flare.fassets] expected_core_vault` (verified out of band).
pub fn check_core_vault(fetched: &str, expected: Option<&str>) -> Result<(), String> {
    match expected.map(str::trim).filter(|e| !e.is_empty()) {
        Some(e) if e == fetched.trim() => Ok(()),
        Some(e) => Err(format!(
            "refusing to sign: Flare RPC Core Vault {fetched} differs from expected_core_vault {e}"
        )),
        None => Err(format!(
            "refusing to sign: verify Core Vault {fetched} out of band, then set [flare.fassets] expected_core_vault"
        )),
    }
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

/// Typed field reader for the FDC proof JSON (numbers may arrive as JSON numbers or strings).
struct ProofJson<'a>(&'a Value);

impl<'a> ProofJson<'a> {
    fn get(&self, key: &str) -> color_eyre::Result<&'a Value> {
        self.0
            .get(key)
            .ok_or_else(|| color_eyre::eyre::eyre!("proof JSON missing `{key}`"))
    }
    fn obj(&self, key: &str) -> color_eyre::Result<ProofJson<'a>> {
        let v = self.get(key)?;
        if !v.is_object() {
            color_eyre::eyre::bail!("proof JSON `{key}` must be an object");
        }
        Ok(ProofJson(v))
    }
    fn str(&self, key: &str) -> color_eyre::Result<&'a str> {
        self.get(key)?
            .as_str()
            .ok_or_else(|| color_eyre::eyre::eyre!("proof JSON `{key}` must be a string"))
    }
    fn hex<T: std::str::FromStr>(&self, key: &str) -> color_eyre::Result<T>
    where
        T::Err: std::fmt::Display,
    {
        self.str(key)?
            .parse()
            .map_err(|e| color_eyre::eyre::eyre!("proof JSON `{key}`: {e}"))
    }
    fn num<T: std::str::FromStr>(&self, key: &str) -> color_eyre::Result<T>
    where
        T::Err: std::fmt::Display,
    {
        let v = self.get(key)?;
        let text = match v {
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            _ => color_eyre::eyre::bail!("proof JSON `{key}` must be a number"),
        };
        text.parse()
            .map_err(|e| color_eyre::eyre::eyre!("proof JSON `{key}`: {e}"))
    }
    fn bool(&self, key: &str) -> color_eyre::Result<bool> {
        self.get(key)?
            .as_bool()
            .ok_or_else(|| color_eyre::eyre::eyre!("proof JSON `{key}` must be a bool"))
    }
}

/// FDC `XRPPayment` proof for `executeDirectMinting`: the DA layer answer `{response, proof}`
/// (or contract-shaped `{data, merkleProof}`), `response` being the decoded attestation.
pub fn parse_fdc_payment_proof_json(
    proof_json: &str,
) -> color_eyre::Result<IAssetManager::XrpPaymentProof> {
    use alloy::primitives::I256;
    let value: Value = serde_json::from_str(proof_json.trim())
        .map_err(|e| color_eyre::eyre::eyre!("invalid FDC proof JSON: {e}"))?;
    let merkle = value
        .get("proof")
        .or_else(|| value.get("merkleProof"))
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| color_eyre::eyre::eyre!("proof JSON needs a non-empty `proof` array"))?
        .iter()
        .map(|leaf| {
            leaf.as_str()
                .and_then(|s| s.parse::<FixedBytes<32>>().ok())
                .ok_or_else(|| color_eyre::eyre::eyre!("`proof` entries must be bytes32 hex"))
        })
        .collect::<color_eyre::Result<Vec<_>>>()?;
    let root = ProofJson(&value);
    let response = root
        .obj("response")
        .or_else(|_| root.obj("data"))
        .map_err(|_| color_eyre::eyre::eyre!("proof JSON needs the decoded `response` object"))?;
    let req = response.obj("requestBody")?;
    let body = response.obj("responseBody")?;
    let first_memo: Bytes = body.hex("firstMemoData")?;
    Ok(IAssetManager::XrpPaymentProof {
        merkleProof: merkle,
        data: IAssetManager::XrpPaymentResponse {
            attestationType: response.hex("attestationType")?,
            sourceId: response.hex("sourceId")?,
            votingRound: response.num("votingRound")?,
            lowestUsedTimestamp: response.num("lowestUsedTimestamp")?,
            requestBody: IAssetManager::XrpPaymentRequestBody {
                transactionId: req.hex("transactionId")?,
                proofOwner: req.hex::<Address>("proofOwner")?,
            },
            responseBody: IAssetManager::XrpPaymentResponseBody {
                blockNumber: body.num("blockNumber")?,
                blockTimestamp: body.num("blockTimestamp")?,
                sourceAddress: body.str("sourceAddress")?.to_string(),
                sourceAddressHash: body.hex("sourceAddressHash")?,
                receivingAddressHash: body.hex("receivingAddressHash")?,
                intendedReceivingAddressHash: body.hex("intendedReceivingAddressHash")?,
                spentAmount: body.num::<I256>("spentAmount")?,
                intendedSpentAmount: body.num::<I256>("intendedSpentAmount")?,
                receivedAmount: body.num::<I256>("receivedAmount")?,
                intendedReceivedAmount: body.num::<I256>("intendedReceivedAmount")?,
                hasMemoData: body.bool("hasMemoData")?,
                firstMemoData: first_memo,
                hasDestinationTag: body.bool("hasDestinationTag")?,
                destinationTag: body.num::<U256>("destinationTag")?,
                status: body.num("status")?,
            },
        },
    })
}

/// Submit `AssetManagerFXRP.executeDirectMinting` with an alloy wallet.
///
/// Returns the Flare transaction hash (`0x…`). Does **not** fetch FDC proofs over HTTP.
/// Gas estimation runs the call first, so a proof the contract rejects fails before sending.
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

    let pending = am
        .executeDirectMinting(proof)
        .send()
        .await
        .map_err(|e| color_eyre::eyre::eyre!("executeDirectMinting send failed: {e}"))?;
    let tx_hash = *pending.tx_hash();
    let receipt = pending
        .get_receipt()
        .await
        .map_err(|e| color_eyre::eyre::eyre!("executeDirectMinting receipt failed: {e}"))?;
    if !receipt.status() {
        color_eyre::eyre::bail!("executeDirectMinting reverted: {tx_hash:#x}");
    }
    Ok(format!("{tx_hash:#x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lead 1: the RPC-supplied Core Vault is signed only when it equals the pinned value.
    #[test]
    fn check_core_vault_requires_pinned_match() {
        assert!(check_core_vault("rVault", Some(" rVault ")).is_ok());
        assert!(
            check_core_vault("rEvil", Some("rVault"))
                .unwrap_err()
                .contains("differs")
        );
        assert!(
            check_core_vault("rVault", None)
                .unwrap_err()
                .contains("expected_core_vault")
        );
        assert!(check_core_vault("rVault", Some("")).is_err());
    }

    /// An oversized uint256 from the RPC must not panic the display formatters.
    #[test]
    fn oversized_uint256_does_not_panic_formatters() {
        assert!(format_price(U256::MAX, 6).parse::<f64>().is_ok());
        assert!(format_token_balance(U256::MAX, 18).parse::<f64>().is_ok());
        assert_eq!(format_price(U256::from(1_500_000_u64), 6), "1.500000");
    }

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

    /// Real Coston2 DA layer `proof-by-request-round` answer for an XRPPayment (testnet C2
    /// payment `FB8556…1A78`, 2026-10-11). Public on-chain data.
    const REAL_DA_PROOF: &str = r#"{"response":{"attestationType":"0x5852505061796d656e7400000000000000000000000000000000000000000000","sourceId":"0x7465737458525000000000000000000000000000000000000000000000000000","votingRound":1480723,"lowestUsedTimestamp":1791695071,"requestBody":{"transactionId":"0xfb8556096c0bf6ae25f858db9af65cc0381e51ff100505672229eaacce5a1a78","proofOwner":"0xed50519368f2a2e0f33b1c94711eb11c82b22c98"},"responseBody":{"blockNumber":21446349,"blockTimestamp":1791695071,"sourceAddress":"rLo2GiHSRivGN17CZhJGFpBbYGXEACpdnJ","sourceAddressHash":"0x952bcd5a19f6a45b7faf59942c00007eeed70d873746f276ca741666045d543f","receivingAddressHash":"0xcf68c86c10983c93617102d84e2f94ea63edf51eaab2c158d8168d0a1f10fb38","intendedReceivingAddressHash":"0xcf68c86c10983c93617102d84e2f94ea63edf51eaab2c158d8168d0a1f10fb38","spentAmount":10000010,"intendedSpentAmount":10000010,"receivedAmount":10000000,"intendedReceivedAmount":10000000,"hasMemoData":true,"firstMemoData":"0x464250526641001800000000ed50519368f2a2e0f33b1c94711eb11c82b22c98","hasDestinationTag":false,"destinationTag":0,"status":0}},"proof":["0xf712dd044a81a3665267c2b93f59e569716ed9cd65d8e9d0be06517f2f9462be","0x7be94cfb7571ab7883f053b2e65671a61018c52efc3d865adf9faea4da7f8e9c","0xd49c694378e96b2dce2ed12996866dc33d0fcf24b21aab64d10fbf141fe4566f"]}"#;

    /// TC-231: C3 targets the deployed `executeDirectMinting(IXRPPayment.Proof)` (selector
    /// 0x78d0299e from the Flare periphery `IDirectMinting`) and decodes the real DA answer.
    #[test]
    fn execute_direct_minting_matches_deployed_abi_and_real_da_proof() {
        use alloy::sol_types::SolCall;
        assert_eq!(
            IAssetManager::executeDirectMintingCall::SELECTOR,
            [0x78, 0xd0, 0x29, 0x9e]
        );
        let proof = parse_fdc_payment_proof_json(REAL_DA_PROOF).expect("real DA proof");
        assert_eq!(proof.merkleProof.len(), 3);
        let (data, body) = (&proof.data, &proof.data.responseBody);
        assert_eq!(data.votingRound, 1_480_723);
        assert_eq!(
            data.requestBody.proofOwner,
            address!("ed50519368f2a2e0f33b1c94711eb11c82b22c98")
        );
        assert_eq!(body.receivedAmount.to_string(), "10000000");
        assert_eq!(
            body.firstMemoData.to_string(),
            "0x464250526641001800000000ed50519368f2a2e0f33b1c94711eb11c82b22c98"
        );
        // Contract-shaped alias carries the same proof.
        let aliased = REAL_DA_PROOF
            .replace(r#""response":"#, r#""data":"#)
            .replace(r#""proof":"#, r#""merkleProof":"#);
        use alloy::sol_types::SolValue;
        assert_eq!(
            parse_fdc_payment_proof_json(&aliased)
                .expect("alias")
                .abi_encode(),
            proof.abi_encode()
        );
    }

    #[test]
    fn parse_fdc_payment_proof_json_rejects_malformed() {
        assert!(parse_fdc_payment_proof_json("{}").is_err());
        assert!(parse_fdc_payment_proof_json("not-json").is_err());
        let empty_proof = REAL_DA_PROOF.replace(r#""proof":["#, r#""proof":[],"x":["#);
        assert!(parse_fdc_payment_proof_json(&empty_proof).is_err());
        // The response is a decoded struct, not ABI bytes (the old pasted shape).
        assert!(parse_fdc_payment_proof_json(r#"{"proof":["0x00"],"response":"0xbb"}"#).is_err());
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
