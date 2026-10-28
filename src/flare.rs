use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use alloy::{
    contract::{CallBuilder, CallDecoder},
    network::{Ethereum, EthereumWallet},
    primitives::{Address, Bytes, FixedBytes, U256, address},
    providers::{PendingTransactionBuilder, Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    sol,
};
use serde_json::Value;

use crate::config::FlareNetwork;
use crate::network::Network;
use crate::xrpl::{
    FlareFeedPrice, FlareWalletSummary, FxrpDirectMintInfo, MintLimits, SmartAccountInfo,
    SmartAccountVault,
};

pub const DEFAULT_FLARE_RPC: &str = "https://flare-api.flare.network/ext/C/rpc";
pub const DEFAULT_FLARE_FEED: &str = "FXRP/USD";
pub const DEFAULT_FLARE_FEEDS: &[&str] = &["FXRP/USD", "FLR/USD", "BTC/USD", "ETH/USD"];
const FLARE_CONTRACT_REGISTRY: Address = address!("aD67FE66660Fb8dFE9d6b1b4240d8650e30F6019");
const ASSET_MANAGER_FXRP_NAME: &str = "AssetManagerFXRP";
const FTSO_V2_REGISTRY_NAME: &str = "FtsoV2";
pub(crate) const MASTER_ACCOUNT_CONTROLLER_NAME: &str = "MasterAccountController";
/// Cap on operator wallets kept from `getXrplProviderWallets()` (display only; `[0]` is used).
const OPERATOR_WALLET_DISPLAY_MAX: usize = 16;

fn flare_registry_cache() -> &'static Mutex<HashMap<(String, String), Address>> {
    static CACHE: OnceLock<Mutex<HashMap<(String, String), Address>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registry lookup, cached per `(rpc_url, contract name)`.
pub(crate) async fn resolve_registry_contract<P: Provider + Clone>(
    provider: P,
    rpc_url: &str,
    contract_name: &str,
) -> color_eyre::Result<Address> {
    let key = (rpc_url.to_string(), contract_name.to_string());
    if let Ok(guard) = flare_registry_cache().lock()
        && let Some(addr) = guard.get(&key)
    {
        return Ok(*addr);
    }

    let registry = FlareContractRegistry::new(FLARE_CONTRACT_REGISTRY, provider);
    let addr = registry
        .getContractAddressByName(contract_name.to_string())
        .call()
        .await?;
    if addr == Address::ZERO {
        color_eyre::eyre::bail!("{contract_name} is not registered in the Flare contract registry");
    }

    if let Ok(mut guard) = flare_registry_cache().lock() {
        guard.insert(key, addr);
    }

    Ok(addr)
}

async fn resolve_ftso_v2_address<P: Provider + Clone>(
    provider: P,
    rpc_url: &str,
) -> color_eyre::Result<Address> {
    resolve_registry_contract(provider, rpc_url, FTSO_V2_REGISTRY_NAME).await
}

pub(crate) async fn resolve_asset_manager_fxrp_address<P: Provider + Clone>(
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

    /// ERC-721 tag manager behind `IAssetManager.getMintingTagManager()` (reads only).
    #[sol(rpc)]
    interface IMintingTagManager {
        function reservationFee() external view returns (uint256);
        function ownerOf(uint256 tagId) external view returns (address);
        function mintingRecipient(uint256 tagId) external view returns (address);
        function allowedExecutor(uint256 tagId) external view returns (address);
        function nextAvailableTag() external view returns (uint256);
        function executorChangeAfterSeconds() external view returns (uint256);
        function reserve() external payable returns (uint256);
        function setMintingRecipient(uint256 tagId, address recipient) external;
        function setAllowedExecutor(uint256 tagId, address executor) external;
        function transfer(address to, uint256 tagId) external;
        event MintingTagReserved(uint256 tag, address owner);
        function pendingAllowedExecutorChange(uint256 tagId) external view returns (bool pending, address newExecutor, uint256 activeAfterTs);
    }

    #[sol(rpc)]
    interface FtsoV2 {
        function getFeedById(bytes21 _feedId) external view returns (uint256 value, int8 decimals, uint64 timestamp);
        function getFeedsById(bytes21[] _feedIds) external view returns (uint256[] values, int8[] decimals, uint64 timestamp);
        function calculateFeeByIds(bytes21[] _feedIds) external view returns (uint256 fee);
    }

    /// Minimal IAssetManager surface: Direct Mint reads / execute, redeem, minting tags, queue.
    #[sol(rpc)]
    interface IAssetManager {
        function directMintingPaymentAddress() external view returns (string memory);
        function getDirectMintingMinimumFeeUBA() external view returns (uint256);
        function getDirectMintingFeeBIPS() external view returns (uint256);
        function getDirectMintingExecutorFeeUBA() external view returns (uint256);
        function fAsset() external view returns (address);
        function mintingPaused() external view returns (bool);
        function emergencyPauseLevel() external view returns (uint8);
        function getDirectMintingHourlyLimitUBA() external view returns (uint256);
        function getDirectMintingDailyLimitUBA() external view returns (uint256);
        function getDirectMintingHourlyLimiterState() external view returns (uint64 windowStart, uint64 mintedAmg);
        function getDirectMintingDailyLimiterState() external view returns (uint64 windowStart, uint64 mintedAmg);
        function getDirectMintingLargeMintingThresholdUBA() external view returns (uint256);
        function getDirectMintingLargeMintingDelaySeconds() external view returns (uint256);

        function assetMintingGranularityUBA() external view returns (uint256);
        function getMintingTagManager() external view returns (address);
        function coreVaultAvailableAmount() external view returns (uint256 immediatelyAvailableUBA, uint256 totalAvailableUBA);
        function getAllAgents(uint256 _start, uint256 _end) external view returns (address[] memory _agents, uint256 _totalLength);

        struct RedemptionTicketInfo {
            uint256 redemptionTicketId;
            address agentVault;
            uint256 ticketValueUBA;
        }
        function redemptionQueue(uint256 _firstRedemptionTicketId, uint256 _pageSize) external view returns (RedemptionTicketInfo[] memory _queue, uint256 _nextRedemptionTicketId);

        event DirectMintingExecuted(bytes32 transactionId, address targetAddress, address executor, uint256 mintedAmountUBA, uint256 mintingFeeUBA, uint256 executorFeeUBA);
        event DirectMintingExecutedToSmartAccount(bytes32 transactionId, string sourceAddress, address executor, uint256 mintedAmountUBA, uint256 mintingFeeUBA, bytes memoData);
        event DirectMintingPaymentTooSmallForFee(bytes32 transactionId, uint256 receivedAmountUBA, uint256 minimumMintingFeeUBA);
        event DirectMintingDelayed(bytes32 transactionId, uint256 amount, uint256 executionAllowedAt);
        event LargeDirectMintingDelayed(bytes32 transactionId, uint256 amount, uint256 executionAllowedAt);

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
        function executeDirectMintingWithData(XrpPaymentProof _payment, bytes _data) external payable;
        function minimumRedeemAmountUBA() external view returns (uint256);
        function redeemAmount(uint256 _amountUBA, string _redeemerUnderlyingAddressString, address _executor) external payable;
        event RedemptionRequestIncomplete(address indexed redeemer, uint256 remainingLots);
        event RedemptionAmountIncomplete(address indexed redeemer, uint256 remainingAmountUBA);
        function redeemWithTag(uint256 _amountUBA, string _redeemerUnderlyingAddressString, address _executor, uint256 _destinationTag) external payable;
    }

    /// Minimal `IMasterAccountController` surface for Smart Account reads (Flare Smart Accounts).
    #[sol(rpc)]
    interface IMasterAccountController {
        function getPersonalAccount(string _xrplOwner) external view returns (address);
        function getNonce(address _personalAccount) external view returns (uint256);
        function getExecutor(address _personalAccount) external view returns (address);
        function isTransactionIdUsed(bytes32 _transactionId) external view returns (bool);
        function getXrplProviderWallets() external view returns (string[] memory);
        function getInstructionFee(uint256 _instructionId) external view returns (uint256);
        function getVaults() external view returns (uint256[] memory vaultIds, address[] memory vaultAddresses, uint8[] memory vaultTypes);
    }

    #[sol(rpc)]
    interface IERC20 {
        function balanceOf(address account) external view returns (uint256);
        function decimals() external view returns (uint8);
        function totalSupply() external view returns (uint256);
    }

    /// ERC-4626 view of a Smart Account vault (Firelight / Upshift share tokens are ERC-20 too).
    #[sol(rpc)]
    interface IVault {
        function convertToAssets(uint256 shares) external view returns (uint256);
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

/// Exact base units → decimal string (`decimals` places, trailing zeros trimmed; no floats).
#[must_use]
pub fn format_token_units(units: u128, decimals: u8) -> String {
    if decimals == 0 {
        return units.to_string();
    }
    let Some(scale) = 10_u128.checked_pow(u32::from(decimals)) else {
        return units.to_string();
    };
    let (whole, frac) = (units / scale, units % scale);
    if frac == 0 {
        return whole.to_string();
    }
    let frac = format!("{frac:0width$}", width = usize::from(decimals));
    format!("{whole}.{}", frac.trim_end_matches('0'))
}

/// BIPS → percent string (`10` → `"0.10%"`, `100` → `"1.00%"`).
#[must_use]
pub fn bips_to_percent_display(bips: u64) -> String {
    let whole = bips / 100;
    let frac = bips % 100;
    format!("{whole}.{frac:02}%")
}

pub async fn fetch_ftso_prices(
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
    let am = IAssetManager::new(asset_manager_addr, provider.clone());

    // Pause views are advisory (a contract without them still serves the rest): `None` = unknown.
    let (required, minting_paused, emergency_pause_level, limits) = tokio::join!(
        async {
            tokio::try_join!(
                async { am.directMintingPaymentAddress().call().await },
                async { am.getDirectMintingMinimumFeeUBA().call().await },
                async { am.getDirectMintingFeeBIPS().call().await },
                async { am.getDirectMintingExecutorFeeUBA().call().await },
            )
        },
        async { am.mintingPaused().call().await.ok() },
        async { am.emergencyPauseLevel().call().await.ok() },
        async { fetch_mint_limits(&am, &provider).await.ok() },
    );
    let (core_vault_xrpl, min_fee, fee_bips, executor_fee) = required?;

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
        minting_paused,
        emergency_pause_level,
        limits,
    })
}

/// Redemption queue pages read at most (`maxRedeemedTickets` tickets each): bounds RPC work.
const REDEMPTION_QUEUE_MAX_PAGES: usize = 100;

/// Aggregate redemption tickets `(agent vault, value UBA)` into totals and the 10 largest agents.
fn summarize_queue(tickets: &[(Address, u128)], lot_size_uba: u128) -> serde_json::Value {
    let total = tickets.iter().fold(0u128, |s, t| s.saturating_add(t.1));
    let mut by_agent: HashMap<Address, (u64, u128)> = HashMap::new();
    for (agent, value) in tickets {
        let e = by_agent.entry(*agent).or_default();
        e.0 += 1;
        e.1 = e.1.saturating_add(*value);
    }
    let mut agents: Vec<_> = by_agent.into_iter().collect();
    agents.sort_by(|a, b| b.1.1.cmp(&a.1.1).then(a.0.cmp(&b.0)));
    let xrp = |uba: u128| crate::xrpl::drops_to_xrp(&uba.to_string());
    serde_json::json!({
        "tickets": tickets.len(),
        "total_uba": total.to_string(),
        "total_xrp": xrp(total),
        "lots": (lot_size_uba != 0).then(|| (total / lot_size_uba).to_string()),
        "agents": agents.len(),
        "top_agents": agents.iter().take(10).map(|(a, (n, v))| serde_json::json!({
            "agent_vault": format!("{a:#x}"),
            "tickets": n,
            "value_xrp": xrp(*v),
        })).collect::<Vec<_>>(),
    })
}

/// Read-only FXRP report for `lazyxrp -x fxrp`: Direct Mint info, MintingTagManager (and one
/// tag when given) and the global redemption queue (unpinned: pages are separate calls).
pub async fn fetch_fxrp_report(
    rpc_url: &str,
    tag: Option<u64>,
) -> color_eyre::Result<serde_json::Value> {
    let info = fetch_fxrp_direct_mint_info(rpc_url).await?;
    let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
    let am = IAssetManager::new(info.asset_manager.parse()?, provider.clone());
    let too_big = |what: &str| color_eyre::eyre::eyre!("Flare RPC returned out-of-range {what}");

    let (manager_addr, settings, granularity) = tokio::try_join!(
        async { Ok(am.getMintingTagManager().call().await?) },
        fetch_settings_raw(&provider, *am.address()),
        async { Ok(am.assetMintingGranularityUBA().call().await?) },
    )?;
    let lot_amg = u64::try_from(settings_word(&settings, SETTINGS_LOT_SIZE_AMG_WORD)?)
        .map_err(|_| too_big("lot size"))?;
    let page_size = settings_word(&settings, SETTINGS_MAX_REDEEMED_TICKETS_WORD)?;
    let lot_size_uba = u128::from(lot_amg)
        .saturating_mul(u128::try_from(granularity).map_err(|_| too_big("granularity"))?);

    let manager = IMintingTagManager::new(manager_addr, provider.clone());
    let fee = manager.reservationFee().call().await?;
    let tag_info = match tag {
        None => None,
        Some(t) => {
            let id = U256::from(t);
            // A tag that is not reserved reverts on `ownerOf`.
            Some(match tag_owner(&manager, id).await? {
                None => {
                    serde_json::json!({ "tag": t, "reserved": false })
                }
                Some(owner) => {
                    let (recipient, executor, pending) = tokio::try_join!(
                        async {
                            Ok::<_, color_eyre::Report>(manager.mintingRecipient(id).call().await?)
                        },
                        async { Ok(manager.allowedExecutor(id).call().await?) },
                        async { Ok(manager.pendingAllowedExecutorChange(id).call().await?) },
                    )?;
                    serde_json::json!({
                        "tag": t,
                        "reserved": true,
                        "owner": format!("{owner:#x}"),
                        "minting_recipient": format!("{recipient:#x}"),
                        "allowed_executor": format!("{executor:#x}"),
                        "pending_executor_change": pending.pending.then(|| serde_json::json!({
                            "new_executor": format!("{:#x}", pending.newExecutor),
                            "active_after": pending.activeAfterTs.to_string(),
                        })),
                    })
                }
            })
        }
    };

    let mut tickets: Vec<(Address, u128)> = Vec::new();
    let mut next = U256::ZERO;
    let mut complete = false;
    for _ in 0..REDEMPTION_QUEUE_MAX_PAGES {
        let page = am.redemptionQueue(next, page_size).call().await?;
        for t in &page._queue {
            tickets.push((
                t.agentVault,
                u128::try_from(t.ticketValueUBA).map_err(|_| too_big("ticket value"))?,
            ));
        }
        next = page._nextRedemptionTicketId;
        if next.is_zero() {
            complete = true;
            break;
        }
    }
    let mut queue = summarize_queue(&tickets, lot_size_uba);
    queue["complete"] = complete.into();
    queue["lot_size_uba"] = lot_size_uba.to_string().into();

    // Core Vault holdings back the FXRP that has no redemption ticket (direct mints create none;
    // agent transfers to the Core Vault consume their tickets), so queue + Core Vault ≈ supply.
    let (core_vault, agents, cooldown, next_tag) = tokio::try_join!(
        async { Ok::<_, color_eyre::Report>(am.coreVaultAvailableAmount().call().await?) },
        async {
            Ok(am
                .getAllAgents(U256::ZERO, U256::from(REDEMPTION_QUEUE_MAX_PAGES))
                .call()
                .await?)
        },
        async { Ok(manager.executorChangeAfterSeconds().call().await?) },
        async { Ok(manager.nextAvailableTag().call().await?) },
    )?;
    let xrp = |v: U256| crate::xrpl::drops_to_xrp(&v.to_string());
    queue["agent_vaults"] = serde_json::json!({
        "total": agents._totalLength.to_string(),
        "listed": agents._agents.iter().map(|a| format!("{a:#x}")).collect::<Vec<_>>(),
    });

    Ok(serde_json::json!({
        "direct_mint": info,
        "minting_tag_manager": {
            "address": format!("{manager_addr:#x}"),
            "reservation_fee_wei": fee.to_string(),
            "next_available_tag": next_tag.to_string(),
            "executor_change_after_secs": cooldown.to_string(),
            "tag": tag_info,
        },
        "core_vault": {
            "immediately_available_xrp": xrp(core_vault.immediatelyAvailableUBA),
            "total_available_xrp": xrp(core_vault.totalAvailableUBA),
        },
        "redemption_queue": queue,
    }))
}

/// Memo-instruction state of one XRPL owner's personal account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoState {
    pub personal: alloy::primitives::Address,
    pub nonce: u128,
    /// Pinned executor (`Address::ZERO` = none).
    pub executor: alloy::primitives::Address,
    /// `isTransactionIdUsed(target)` when a target was given.
    pub target_used: Option<bool>,
}

/// Read `getNonce`, `getExecutor` and (optionally) `isTransactionIdUsed(target)` for the
/// personal account of `xrpl_address` (read-only).
pub async fn read_memo_state(
    rpc_url: &str,
    xrpl_address: &str,
    target: Option<[u8; 32]>,
) -> color_eyre::Result<MemoState> {
    let xrpl_owner = crate::signing::require_classic_address_shape("XRPL owner", xrpl_address)?;
    let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
    let controller_addr =
        resolve_registry_contract(provider.clone(), rpc_url, MASTER_ACCOUNT_CONTROLLER_NAME)
            .await?;
    let controller = IMasterAccountController::new(controller_addr, provider);
    let personal = controller
        .getPersonalAccount(xrpl_owner.to_string())
        .call()
        .await?;
    let (nonce, executor, target_used) = tokio::try_join!(
        async { Ok::<_, color_eyre::Report>(controller.getNonce(personal).call().await?) },
        async { Ok(controller.getExecutor(personal).call().await?) },
        async {
            match target {
                Some(t) => Ok(Some(controller.isTransactionIdUsed(t.into()).call().await?)),
                None => Ok(None),
            }
        },
    )?;
    Ok(MemoState {
        nonce: u128::try_from(nonce)
            .map_err(|_| color_eyre::eyre::eyre!("Flare RPC returned an out-of-range nonce"))?,
        personal,
        executor,
        target_used,
    })
}

/// `MasterAccountController.isTransactionIdUsed(tx)`: whether a Smart Account direct-mint payment
/// (memo instruction) was already finalized on Flare. Plain recipient-memo mints are not tracked
/// by this view.
pub async fn smart_account_tx_used(rpc_url: &str, tx: [u8; 32]) -> color_eyre::Result<bool> {
    let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
    let controller_addr =
        resolve_registry_contract(provider.clone(), rpc_url, MASTER_ACCOUNT_CONTROLLER_NAME)
            .await?;
    Ok(IMasterAccountController::new(controller_addr, provider)
        .isTransactionIdUsed(tx.into())
        .call()
        .await?)
}

/// Resolve `MasterAccountController` and read one XRPL owner's Smart Account state (read-only):
/// personal account (+ deployed?), nonce, operator wallets, FXRP transfer / redeem instruction
/// fees and the personal account's FXRP balance.
pub async fn fetch_smart_account_info(
    rpc_url: &str,
    xrpl_address: &str,
) -> color_eyre::Result<SmartAccountInfo> {
    let xrpl_owner = crate::signing::require_classic_address_shape("XRPL owner", xrpl_address)?;
    let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
    let (controller_addr, asset_manager_addr) = tokio::try_join!(
        resolve_registry_contract(provider.clone(), rpc_url, MASTER_ACCOUNT_CONTROLLER_NAME),
        resolve_asset_manager_fxrp_address(provider.clone(), rpc_url),
    )?;
    let controller = IMasterAccountController::new(controller_addr, provider.clone());
    let personal = controller
        .getPersonalAccount(xrpl_owner.to_string())
        .call()
        .await?;
    let fxrp = IERC20::new(
        IAssetManager::new(asset_manager_addr, provider.clone())
            .fAsset()
            .call()
            .await?,
        provider.clone(),
    );

    let (nonce, wallets, transfer_fee, redeem_fee, balance, decimals, code, vault_list) = tokio::try_join!(
        async { controller.getNonce(personal).call().await },
        async { controller.getXrplProviderWallets().call().await },
        async {
            controller
                .getInstructionFee(U256::from(crate::signing::SMART_ACCOUNT_FXRP_TRANSFER))
                .call()
                .await
        },
        async {
            controller
                .getInstructionFee(U256::from(crate::signing::SMART_ACCOUNT_FXRP_REDEEM))
                .call()
                .await
        },
        async { fxrp.balanceOf(personal).call().await },
        async { fxrp.decimals().call().await },
        async { Ok(provider.get_code_at(personal).await?) },
        async { Ok(controller.getVaults().call().await.ok()) },
    )?;
    if decimals > crate::signing::MAX_TOKEN_DECIMALS {
        color_eyre::eyre::bail!("Flare RPC returned out-of-range FXRP decimals {decimals}");
    }
    let vaults = match vault_list {
        Some(list) => fetch_vault_positions(&provider, personal, &list).await,
        None => Vec::new(),
    };

    // Remote values: checked conversions, never panicking (the panic hook exits the TUI).
    let too_big = |what: &str| color_eyre::eyre::eyre!("Flare RPC returned out-of-range {what}");
    Ok(SmartAccountInfo {
        xrpl_owner: xrpl_owner.to_string(),
        controller: format!("{controller_addr:#x}"),
        personal_account: format!("{personal:#x}"),
        deployed: !code.is_empty(),
        nonce: nonce.try_into().map_err(|_| too_big("nonce"))?,
        operator_wallets: wallets
            .into_iter()
            .take(OPERATOR_WALLET_DISPLAY_MAX)
            .collect(),
        transfer_fee_drops: transfer_fee
            .try_into()
            .map_err(|_| too_big("transfer instruction fee"))?,
        redeem_fee_drops: redeem_fee
            .try_into()
            .map_err(|_| too_big("redeem instruction fee"))?,
        fxrp_decimals: decimals,
        fxrp_balance_units: balance.try_into().map_err(|_| too_big("FXRP balance"))?,
        vaults,
    })
}

/// Vaults shown at most (display bound; a hostile list must not trigger unbounded reads).
const VAULT_READ_MAX: usize = 8;

/// Share balance (+ ERC-4626 asset value) of `personal` in each registered vault. Advisory: a vault
/// whose reads fail is skipped. Sequential over at most `VAULT_READ_MAX` vaults.
async fn fetch_vault_positions<P: Provider + Clone>(
    provider: &P,
    personal: Address,
    list: &IMasterAccountController::getVaultsReturn,
) -> Vec<SmartAccountVault> {
    let mut out = Vec::new();
    let entries = list
        .vaultIds
        .iter()
        .zip(&list.vaultAddresses)
        .zip(&list.vaultTypes)
        .take(VAULT_READ_MAX);
    for ((id, addr), kind) in entries {
        let Ok(shares) = IERC20::new(*addr, provider.clone())
            .balanceOf(personal)
            .call()
            .await
        else {
            continue;
        };
        let (Ok(id), Ok(shares_u)) = (u64::try_from(*id), u128::try_from(shares)) else {
            continue;
        };
        if shares_u == 0 {
            continue;
        }
        let assets = IVault::new(*addr, provider.clone())
            .convertToAssets(shares)
            .call()
            .await
            .ok()
            .and_then(|a| u128::try_from(a).ok());
        out.push(SmartAccountVault {
            id,
            address: format!("{addr:#x}"),
            kind: *kind,
            shares: shares_u,
            assets,
        });
    }
    out
}

/// A value read from one Flare RPC answer is signed against only when it equals the
/// operator-pinned config value (verified out of band). `section` / `key` name the pin.
fn check_pinned(
    label: &str,
    section: &str,
    key: &str,
    fetched: &str,
    expected: Option<&str>,
) -> Result<(), String> {
    match expected.map(str::trim).filter(|e| !e.is_empty()) {
        Some(e) if e == fetched.trim() => Ok(()),
        Some(e) => Err(format!(
            "refusing to sign: Flare RPC {label} {fetched} differs from {key} {e}"
        )),
        None => Err(format!(
            "refusing to sign: verify {label} {fetched} out of band, then set [flare.{section}] {key}"
        )),
    }
}

/// The Core Vault comes from one Flare RPC answer; sign a Payment to it only when it equals the
/// operator-pinned `[flare.fassets] expected_core_vault` (verified out of band).
pub fn check_core_vault(fetched: &str, expected: Option<&str>) -> Result<(), String> {
    check_pinned(
        "Core Vault",
        "fassets",
        "expected_core_vault",
        fetched,
        expected,
    )
}

/// The operator comes from one Flare RPC answer; sign an instruction Payment to it only when it
/// equals the operator-pinned `[flare.smart_accounts] expected_operator` (verified out of band).
pub fn check_operator(fetched: &str, expected: Option<&str>) -> Result<(), String> {
    check_pinned(
        "operator",
        "smart_accounts",
        "expected_operator",
        fetched,
        expected,
    )
}

/// XRPL Mainnet ↔ Flare, XRPL Testnet ↔ Coston2; anything else is refused before any RPC.
pub fn check_network_pair(xrpl: &Network, flare: FlareNetwork) -> Result<(), String> {
    match (xrpl, flare) {
        (Network::Mainnet, FlareNetwork::Flare) | (Network::Testnet, FlareNetwork::Coston2) => {
            Ok(())
        }
        (x, f) => Err(format!(
            "XRPL {x:?} does not pair with [flare] network {f:?} (Mainnet ↔ flare, Testnet ↔ coston2)"
        )),
    }
}

/// EVM chain id that `eth_chainId` must report for each configured Flare network.
#[must_use]
pub const fn expected_chain_id(network: FlareNetwork) -> u64 {
    match network {
        FlareNetwork::Flare => 14,
        FlareNetwork::Songbird => 19,
        FlareNetwork::Coston2 => 114,
    }
}

/// The RPC must report the chain id of the configured network (a mis-pointed or hostile RPC
/// would otherwise make the `[flare] network` label, and every gate built on it, a lie).
pub fn check_chain_id(configured: FlareNetwork, reported: u64) -> Result<(), String> {
    let expected = expected_chain_id(configured);
    if reported == expected {
        return Ok(());
    }
    Err(format!(
        "Flare RPC reports chain id {reported}, but [flare] network {configured:?} is chain id {expected}; refusing"
    ))
}

/// Write gate: right chain, and on Flare mainnet (decided by the *reported* chain id, not the
/// config label) only with `--yes`.
pub fn check_write_chain(configured: FlareNetwork, reported: u64, yes: bool) -> Result<(), String> {
    check_chain_id(configured, reported)?;
    if reported == expected_chain_id(FlareNetwork::Flare) && !yes {
        return Err("Flare mainnet write: re-run with --yes to allow it".into());
    }
    Ok(())
}

/// Read-only chain-id check for CLI paths whose Flare reads decide where XRPL funds go.
pub async fn verify_rpc_chain(rpc_url: &str, configured: FlareNetwork) -> color_eyre::Result<()> {
    let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
    check_chain_id(configured, provider.get_chain_id().await?)
        .map_err(|e| color_eyre::eyre::eyre!(e))
}

/// What every Flare EVM write must prove before it signs anything: which network the config
/// names and whether `--yes` was given.
#[derive(Debug, Clone, Copy)]
pub struct EvmGate {
    pub network: FlareNetwork,
    pub yes: bool,
}

impl EvmGate {
    /// `eth_chainId` of `provider` against [`check_write_chain`].
    pub async fn check<P: Provider>(self, provider: &P) -> color_eyre::Result<()> {
        check_write_chain(self.network, provider.get_chain_id().await?, self.yes)
            .map_err(|e| color_eyre::eyre::eyre!(e))
    }
}

/// Most one Flare EVM write may pay for gas (`gas_limit × max_fee_per_gas`): 10 FLR. A routine
/// write costs well under 1 FLR (≈ 2 M gas at the 25 gwei floor is 0.05 FLR), so only a hostile
/// or broken fee / gas answer can reach this. The attestation fee (`value`) is capped separately.
pub const MAX_TX_FEE_WEI: u128 = 10_000_000_000_000_000_000;

pub fn check_tx_fee(gas_limit: u64, max_fee_per_gas: u128) -> Result<(), String> {
    let cost = u128::from(gas_limit).saturating_mul(max_fee_per_gas);
    if cost > MAX_TX_FEE_WEI {
        return Err(format!(
            "refusing to send: gas {gas_limit} × max fee {max_fee_per_gas} wei = {cost} wei exceeds the {MAX_TX_FEE_WEI} wei cap"
        ));
    }
    Ok(())
}

/// Simulate (`eth_estimateGas` runs the call, so a revert fails here), cap the fee, then send
/// with the exact gas / fee fields that were checked.
pub(crate) async fn send_capped<P: Provider, D: CallDecoder>(
    call: CallBuilder<P, D>,
    say: impl Fn(&str),
) -> color_eyre::Result<PendingTransactionBuilder<Ethereum>> {
    let gas = call.estimate_gas().await?;
    let fees = call.provider.estimate_eip1559_fees().await?;
    check_tx_fee(gas, fees.max_fee_per_gas).map_err(|e| color_eyre::eyre::eyre!(e))?;
    say(&format!("simulated ok, gas {gas}"));
    Ok(call
        .gas(gas)
        .max_fee_per_gas(fees.max_fee_per_gas)
        .max_priority_fee_per_gas(fees.max_priority_fee_per_gas)
        .send()
        .await?)
}

/// A contract `revert` (a node answers `execution reverted`), as opposed to a transport failure.
fn is_revert(e: &alloy::contract::Error) -> bool {
    e.as_revert_data().is_some() || e.to_string().to_lowercase().contains("revert")
}

/// Owner of `id`, or `None` when the tag is not reserved (`ownerOf` reverts). Any other error
/// (transport, rate limit, malformed answer) is propagated, never read as "not reserved".
pub(crate) async fn tag_owner<P: Provider>(
    manager: &IMintingTagManager::IMintingTagManagerInstance<P>,
    id: U256,
) -> color_eyre::Result<Option<Address>> {
    match manager.ownerOf(id).call().await {
        Ok(owner) => Ok(Some(owner)),
        Err(e) if is_revert(&e) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Refuse Flare writes unless `[flare.fassets] execute = true`.
pub fn ensure_fassets_execute_enabled(execute: bool) -> color_eyre::Result<()> {
    if !execute {
        color_eyre::eyre::bail!(
            "flare fassets execute is disabled; set [flare.fassets] execute = true to allow Flare writes"
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
    gate: EvmGate,
    proof_json: &str,
) -> color_eyre::Result<(String, DirectMintOutcome)> {
    execute_direct_minting_with(rpc_url, private_key_hex, gate, proof_json, None).await
}

/// Flare EVM private key (`0x` optional) → signer, without copying the key text.
pub(crate) fn parse_evm_key(private_key_hex: &str) -> color_eyre::Result<PrivateKeySigner> {
    let key = private_key_hex.trim();
    let key = key
        .strip_prefix("0x")
        .or_else(|| key.strip_prefix("0X"))
        .unwrap_or(key);
    key.parse()
        .map_err(|e| color_eyre::eyre::eyre!("invalid Flare EVM private key: {e}"))
}

/// `executeDirectMinting`, or `executeDirectMintingWithData` when `data` is given (Smart Account
/// custom instructions whose memo commits to the user operation by hash).
pub async fn execute_direct_minting_with(
    rpc_url: &str,
    private_key_hex: &str,
    gate: EvmGate,
    proof_json: &str,
    data: Option<Bytes>,
) -> color_eyre::Result<(String, DirectMintOutcome)> {
    let wallet = EthereumWallet::from(parse_evm_key(private_key_hex)?);
    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(rpc_url.parse()?);
    // The chain / `--yes` gate comes before anything else is read or signed.
    gate.check(&provider).await?;
    let proof = parse_fdc_payment_proof_json(proof_json)?;

    let asset_manager_addr = resolve_asset_manager_fxrp_address(provider.clone(), rpc_url).await?;
    let am = IAssetManager::new(asset_manager_addr, &provider);

    let pending = match data {
        None => send_capped(am.executeDirectMinting(proof), |_| {}).await,
        Some(d) => send_capped(am.executeDirectMintingWithData(proof, d), |_| {}).await,
    }
    .map_err(|e| {
        let hint = if e.to_string().contains("0x18dce79f") {
            " (PaymentAlreadyConfirmed: another executor already finalized this payment; check the recipient balance)"
        } else {
            ""
        };
        color_eyre::eyre::eyre!("executeDirectMinting send failed: {e}{hint}")
    })?;
    let tx_hash = *pending.tx_hash();
    let receipt = pending
        .get_receipt()
        .await
        .map_err(|e| color_eyre::eyre::eyre!("executeDirectMinting receipt failed: {e}"))?;
    if !receipt.status() {
        color_eyre::eyre::bail!("executeDirectMinting reverted: {tx_hash:#x}");
    }
    let outcome = decode_direct_minting_outcome(receipt.inner.logs(), asset_manager_addr);
    Ok((format!("{tx_hash:#x}"), outcome))
}

/// What a non-reverting `executeDirectMinting` actually did, read from the receipt's events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectMintOutcome {
    Minted {
        minted_uba: u128,
    },
    MintedToSmartAccount {
        minted_uba: u128,
    },
    /// Throttled (hourly/daily window or large mint): nothing minted; re-run with the same proof
    /// at or after `execution_allowed_at` (unix seconds).
    Delayed {
        execution_allowed_at: u64,
    },
    /// Payment below the minimum fee: the whole payment went to the fee receiver.
    PaymentTooSmall {
        received_uba: u128,
        minimum_uba: u128,
    },
    /// The call succeeded but no AssetManager DirectMinting event was found.
    Unknown,
}

impl std::fmt::Display for DirectMintOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Minted { minted_uba } => {
                write!(f, "minted {} FXRP", uba_to_xrp_display(*minted_uba))
            }
            Self::MintedToSmartAccount { minted_uba } => write!(
                f,
                "minted {} FXRP to the Smart Account manager (routed to the personal account)",
                uba_to_xrp_display(*minted_uba)
            ),
            Self::Delayed {
                execution_allowed_at,
            } => write!(
                f,
                "DELAYED by mint rate limit, nothing minted yet: re-run with the same proof at or after unix time {execution_allowed_at}"
            ),
            Self::PaymentTooSmall {
                received_uba,
                minimum_uba,
            } => write!(
                f,
                "payment {} XRP is below the minimum fee {} XRP: it was minted entirely to the fee receiver",
                uba_to_xrp_display(*received_uba),
                uba_to_xrp_display(*minimum_uba)
            ),
            Self::Unknown => {
                write!(
                    f,
                    "no DirectMinting event in the receipt; verify on the explorer"
                )
            }
        }
    }
}

fn u128_sat(v: U256) -> u128 {
    u128::try_from(v).unwrap_or(u128::MAX)
}

/// Classify an `executeDirectMinting` receipt. Only logs emitted by the AssetManager count.
pub fn decode_direct_minting_outcome(
    logs: &[alloy::rpc::types::Log],
    asset_manager: Address,
) -> DirectMintOutcome {
    use alloy::sol_types::SolEvent;
    let at = |v: U256| u64::try_from(v).unwrap_or(u64::MAX);
    for log in logs.iter().filter(|l| l.address() == asset_manager) {
        let Some(topic) = log.topic0() else { continue };
        if *topic == IAssetManager::DirectMintingExecuted::SIGNATURE_HASH
            && let Ok(e) = log.log_decode::<IAssetManager::DirectMintingExecuted>()
        {
            return DirectMintOutcome::Minted {
                minted_uba: u128_sat(e.inner.data.mintedAmountUBA),
            };
        }
        if *topic == IAssetManager::DirectMintingExecutedToSmartAccount::SIGNATURE_HASH
            && let Ok(e) = log.log_decode::<IAssetManager::DirectMintingExecutedToSmartAccount>()
        {
            return DirectMintOutcome::MintedToSmartAccount {
                minted_uba: u128_sat(e.inner.data.mintedAmountUBA),
            };
        }
        if *topic == IAssetManager::DirectMintingDelayed::SIGNATURE_HASH
            && let Ok(e) = log.log_decode::<IAssetManager::DirectMintingDelayed>()
        {
            return DirectMintOutcome::Delayed {
                execution_allowed_at: at(e.inner.data.executionAllowedAt),
            };
        }
        if *topic == IAssetManager::LargeDirectMintingDelayed::SIGNATURE_HASH
            && let Ok(e) = log.log_decode::<IAssetManager::LargeDirectMintingDelayed>()
        {
            return DirectMintOutcome::Delayed {
                execution_allowed_at: at(e.inner.data.executionAllowedAt),
            };
        }
        if *topic == IAssetManager::DirectMintingPaymentTooSmallForFee::SIGNATURE_HASH
            && let Ok(e) = log.log_decode::<IAssetManager::DirectMintingPaymentTooSmallForFee>()
        {
            return DirectMintOutcome::PaymentTooSmall {
                received_uba: u128_sat(e.inner.data.receivedAmountUBA),
                minimum_uba: u128_sat(e.inner.data.minimumMintingFeeUBA),
            };
        }
    }
    DirectMintOutcome::Unknown
}

/// Refuse a Direct Mint Payment that would not leave anything after the fees. A payment below
/// `getDirectMintingMinimumFeeUBA()` is minted **entirely** to the fee receiver (irreversible).
/// Upstream `DirectMintingFacet._computeFees`: `fee = min(max(received * bips, minimum), received)`,
/// `executor = min(executorFee, received - fee)`, recipient gets `received - fee - executor`, so the
/// recipient receives something iff `received > fee + executorFee`. That is the bound enforced here
/// (the percentage share is floored like `mulBips`).
pub fn check_direct_mint_amount(
    amount_drops: u64,
    min_fee_uba: u128,
    fee_bips: u64,
    executor_fee_uba: u128,
) -> Result<(), String> {
    let amount = u128::from(amount_drops);
    let pct_fee = amount * u128::from(fee_bips) / 10_000;
    let required = min_fee_uba.max(pct_fee).saturating_add(executor_fee_uba);
    if amount > required {
        return Ok(());
    }
    Err(format!(
        "amount {} XRP must exceed the Direct Mint fees ({} XRP: max(min fee {} XRP, {}) + executor {} XRP); a payment below the minimum fee is minted entirely to the fee receiver",
        uba_to_xrp_display(amount),
        uba_to_xrp_display(required),
        uba_to_xrp_display(min_fee_uba),
        bips_to_percent_display(fee_bips),
        uba_to_xrp_display(executor_fee_uba),
    ))
}

/// `-x sa-memo` / `sa-call` / the TUI Direct Mint send XRP to the Core Vault: refuse while FXRP
/// minting is paused and when the amount is over the minting cap (the XRP would sit in the Core
/// Vault until it has room).
pub fn check_direct_mint_open(info: &FxrpDirectMintInfo, amount_xrp: &str) -> Result<(), String> {
    if info.minting_paused == Some(true) || info.emergency_pause_level.is_some_and(|l| l > 0) {
        return Err("FXRP minting is paused on Flare; not sending XRP to the Core Vault".into());
    }
    let drops = crate::xrpl::xrp_to_drops(amount_xrp.trim()).map_err(|e| format!("amount: {e}"))?;
    if let Some(limits) = &info.limits
        && classify_mint(u128::from(drops), limits) == MintClass::CapExceeded
    {
        return Err(format!(
            "{}; the XRP would sit in the Core Vault until the cap has room",
            MintClass::CapExceeded
        ));
    }
    Ok(())
}

/// Refuse when the live Flare views differ from the composer's snapshot (a fee bump in between
/// would mint the whole Direct Mint Payment to the fee receiver) or minting is paused / capped.
fn compare_direct_mint_terms(
    info: &FxrpDirectMintInfo,
    p: &crate::xrpl::FxrpDirectMintPaymentParams,
) -> Result<(), String> {
    if (info.min_fee_uba, info.fee_bips, info.executor_fee_uba)
        != (p.min_fee_uba, p.fee_bips, p.executor_fee_uba)
    {
        return Err("Direct Mint fees changed on Flare since the form was filled; refresh (Overview) and retry".into());
    }
    if info.core_vault_xrpl.trim() != p.core_vault_xrpl.trim() {
        return Err(
            "Core Vault address changed on Flare since the form was filled; refresh and retry"
                .into(),
        );
    }
    check_direct_mint_open(info, &p.amount_xrp)
}

/// Re-read the Direct Mint views from the Flare RPC right before the TUI signs a payment.
pub async fn verify_direct_mint_terms(
    rpc_url: &str,
    p: &crate::xrpl::FxrpDirectMintPaymentParams,
) -> Result<(), String> {
    let info = fetch_fxrp_direct_mint_info(rpc_url)
        .await
        .map_err(|e| format!("Flare re-check before signing failed: {e}"))?;
    compare_direct_mint_terms(&info, p)
}

/// Operator / `getInstructionFee` drift check. Vault instructions (`0x11`+) use other instruction
/// ids and are not re-read here (the operator pin still applies).
fn compare_smart_account_terms(
    info: &SmartAccountInfo,
    p: &crate::xrpl::SmartAccountInstructionParams,
) -> Result<(), String> {
    if info.operator_wallets.first().map(|s| s.trim()) != Some(p.operator_xrpl.trim()) {
        return Err(
            "Smart Account operator changed on Flare since the form was filled; refresh and retry"
                .into(),
        );
    }
    if p.vault.is_none() {
        let live = if p.redeem {
            info.redeem_fee_drops
        } else {
            info.transfer_fee_drops
        };
        if live != p.fee_drops {
            return Err("Smart Account instruction fee changed on Flare since the form was filled; refresh and retry".into());
        }
    }
    Ok(())
}

/// Re-read the Smart Account operator and fee from the Flare RPC right before signing.
pub async fn verify_smart_account_terms(
    rpc_url: &str,
    xrpl_owner: &str,
    p: &crate::xrpl::SmartAccountInstructionParams,
) -> Result<(), String> {
    let info = fetch_smart_account_info(rpc_url, xrpl_owner)
        .await
        .map_err(|e| format!("Flare re-check before signing failed: {e}"))?;
    compare_smart_account_terms(&info, p)
}

/// Largest mint a tumbling window still lets through right now. The limiter views are stale
/// between writes, so the window is advanced to `now` exactly like `MintingRateLimiter`; the
/// on-chain test is `minted < limit`, so the usable headroom is one granule lower.
#[must_use]
pub fn window_max_now(
    now: u64,
    start: u64,
    minted_amg: u64,
    granularity_uba: u128,
    limit_uba: u128,
    size_secs: u64,
) -> u128 {
    let mut used = u128::from(minted_amg).saturating_mul(granularity_uba);
    if start > 0 && now >= start.saturating_add(size_secs) {
        let windows = u128::from((now - start) / size_secs);
        used = used.saturating_sub(windows.saturating_mul(limit_uba));
    }
    limit_uba
        .saturating_sub(used)
        .saturating_sub(granularity_uba)
}

/// What `executeDirectMinting` will do with a mint of `amount_uba` (floored to whole granules).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MintClass {
    Immediate,
    /// Hourly / daily window overflow: delayed, retry with the same proof.
    DelayedWindow,
    /// At or above the large-mint threshold: fixed delay, windows not consulted.
    DelayedLarge {
        secs: u64,
    },
    /// Over the global cap: `executeDirectMinting` reverts (the XRP waits in the Core Vault).
    CapExceeded,
}

#[must_use]
pub fn classify_mint(amount_uba: u128, limits: &MintLimits) -> MintClass {
    let granule = limits.granularity_uba.max(1);
    let amount = amount_uba / granule * granule;
    if limits.cap_headroom_uba.is_some_and(|h| amount > h) {
        MintClass::CapExceeded
    } else if amount >= limits.large_threshold_uba {
        MintClass::DelayedLarge {
            secs: limits.large_delay_secs,
        }
    } else if amount > limits.window_max_now_uba {
        MintClass::DelayedWindow
    } else {
        MintClass::Immediate
    }
}

impl std::fmt::Display for MintClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Immediate => write!(f, "executes immediately"),
            Self::DelayedWindow => write!(f, "delayed by the hourly/daily mint limit"),
            Self::DelayedLarge { secs } => {
                write!(
                    f,
                    "large mint: delayed {}h {}m",
                    secs / 3600,
                    secs % 3600 / 60
                )
            }
            Self::CapExceeded => write!(f, "over the FXRP minting cap: would revert"),
        }
    }
}

/// Read the Direct Mint throttles in one parallel batch, pinned to the latest block time.
async fn fetch_mint_limits<P: Provider + Clone>(
    am: &IAssetManager::IAssetManagerInstance<P>,
    provider: &P,
) -> color_eyre::Result<MintLimits> {
    use color_eyre::Report;
    let (
        settings_raw,
        granularity,
        hourly,
        daily,
        hourly_state,
        daily_state,
        large_threshold,
        large_delay,
        fasset,
    ) = tokio::try_join!(
        async { fetch_settings_raw(provider, *am.address()).await },
        async {
            am.assetMintingGranularityUBA()
                .call()
                .await
                .map_err(Report::from)
        },
        async {
            am.getDirectMintingHourlyLimitUBA()
                .call()
                .await
                .map_err(Report::from)
        },
        async {
            am.getDirectMintingDailyLimitUBA()
                .call()
                .await
                .map_err(Report::from)
        },
        async {
            am.getDirectMintingHourlyLimiterState()
                .call()
                .await
                .map_err(Report::from)
        },
        async {
            am.getDirectMintingDailyLimiterState()
                .call()
                .await
                .map_err(Report::from)
        },
        async {
            am.getDirectMintingLargeMintingThresholdUBA()
                .call()
                .await
                .map_err(Report::from)
        },
        async {
            am.getDirectMintingLargeMintingDelaySeconds()
                .call()
                .await
                .map_err(Report::from)
        },
        async { am.fAsset().call().await.map_err(Report::from) },
    )?;
    let (supply, block) = tokio::try_join!(
        async {
            IERC20::new(fasset, provider.clone())
                .totalSupply()
                .call()
                .await
                .map_err(Report::from)
        },
        async {
            provider
                .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
                .await
                .map_err(Report::from)
        },
    )?;
    let now = block
        .ok_or_else(|| color_eyre::eyre::eyre!("no latest block"))?
        .header
        .timestamp;
    let chk = |v: U256| -> color_eyre::Result<u128> {
        u128::try_from(v)
            .map_err(|_| color_eyre::eyre::eyre!("Flare RPC returned out-of-range limit"))
    };
    let granule = chk(granularity)?;
    let cap_amg = settings_minting_cap_amg(&settings_raw)?;
    let supply_uba = chk(supply)?;
    let (hourly, daily) = (chk(hourly)?, chk(daily)?);
    // Window sizes: upstream `MintingRateLimiter.State.windowSizeSeconds` is governance-settable
    // (`setWindowSizeSeconds`) and has no getter, so 1 h / 1 d (the documented limiter windows)
    // is assumed; a governance change makes `Max now` wrong until updated here.
    let hourly_max = window_max_now(
        now,
        hourly_state.windowStart,
        hourly_state.mintedAmg,
        granule,
        hourly,
        3600,
    );
    let daily_max = window_max_now(
        now,
        daily_state.windowStart,
        daily_state.mintedAmg,
        granule,
        daily,
        86_400,
    );
    Ok(MintLimits {
        cap_uba: u128::from(cap_amg).saturating_mul(granule),
        supply_uba,
        // Reserved capacity has no getter, so `cap - supply` is an upper bound.
        cap_headroom_uba: (cap_amg != 0).then(|| {
            u128::from(cap_amg)
                .saturating_sub(supply_uba / granule.max(1))
                .saturating_mul(granule)
        }),
        window_max_now_uba: hourly_max.min(daily_max),
        large_threshold_uba: chk(large_threshold)?,
        large_delay_secs: u64::try_from(chk(large_delay)?).unwrap_or(u64::MAX),
        granularity_uba: granule,
    })
}

async fn fetch_settings_raw<P: Provider + Clone>(
    provider: &P,
    asset_manager: Address,
) -> color_eyre::Result<alloy::primitives::Bytes> {
    // `AssetManagerSettings.Data` has 58 fields: more than alloy's tuple ABI types support,
    // so read the raw head and pick the words needed.
    let selector = alloy::primitives::keccak256("getSettings()");
    let req = alloy::rpc::types::TransactionRequest::default()
        .to(asset_manager)
        .input(selector[..4].to_vec().into());
    Ok(provider.call(req).await?)
}

/// Field indexes in `AssetManagerSettings.Data` (upstream `AssetManagerSettings.sol` order);
/// the ABI answer is `[offset = 0x20][struct head: one 32-byte word per field]`.
const SETTINGS_LOT_SIZE_AMG_WORD: usize = 19;
const SETTINGS_MINTING_CAP_AMG_WORD: usize = 22;
const SETTINGS_MAX_REDEEMED_TICKETS_WORD: usize = 30;

fn settings_word(raw: &[u8], field: usize) -> color_eyre::Result<U256> {
    let word = |i: usize| raw.get(32 * i..32 * (i + 1));
    let (Some(offset), Some(value)) = (word(0), word(1 + field)) else {
        color_eyre::eyre::bail!("getSettings() answer too short");
    };
    if U256::from_be_slice(offset) != U256::from(32) {
        color_eyre::eyre::bail!("getSettings() answer has an unexpected layout");
    }
    Ok(U256::from_be_slice(value))
}

fn settings_minting_cap_amg(raw: &[u8]) -> color_eyre::Result<u64> {
    u64::try_from(settings_word(raw, SETTINGS_MINTING_CAP_AMG_WORD)?)
        .map_err(|_| color_eyre::eyre::eyre!("Flare RPC returned out-of-range minting cap"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC-205: the RPC-supplied Core Vault is signed only when it equals the pinned value.
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

    fn mint_info(min_fee: u128) -> FxrpDirectMintInfo {
        FxrpDirectMintInfo {
            core_vault_xrpl: "rVault".into(),
            asset_manager: String::new(),
            min_fee_uba: min_fee,
            fee_bips: 10,
            executor_fee_uba: 100,
            minting_paused: Some(false),
            emergency_pause_level: Some(0),
            limits: None,
        }
    }

    fn mint_params(min_fee: u128) -> crate::xrpl::FxrpDirectMintPaymentParams {
        crate::xrpl::FxrpDirectMintPaymentParams {
            core_vault_xrpl: "rVault".into(),
            min_fee_uba: min_fee,
            fee_bips: 10,
            executor_fee_uba: 100,
            flare_recipient: String::new(),
            amount_xrp: "5".into(),
            custom_memo: None,
            skip_mainnet_prompt: false,
            network: crate::network::Network::Testnet,
        }
    }

    /// A fee bump / vault change / pause between compose and sign is refused (the composer's
    /// snapshot alone would have let the payment through and minted it all to the fee receiver).
    #[test]
    fn direct_mint_terms_drift_is_refused() {
        assert!(compare_direct_mint_terms(&mint_info(1), &mint_params(1)).is_ok());
        let e = compare_direct_mint_terms(&mint_info(9_000_000), &mint_params(1)).unwrap_err();
        assert!(e.contains("fees changed"), "{e}");
        let mut moved = mint_info(1);
        moved.core_vault_xrpl = "rOther".into();
        assert!(
            compare_direct_mint_terms(&moved, &mint_params(1))
                .unwrap_err()
                .contains("Core Vault")
        );
        let mut paused = mint_info(1);
        paused.minting_paused = Some(true);
        assert!(
            compare_direct_mint_terms(&paused, &mint_params(1))
                .unwrap_err()
                .contains("paused")
        );
    }

    #[test]
    fn smart_account_terms_drift_is_refused() {
        let info = SmartAccountInfo {
            xrpl_owner: String::new(),
            controller: String::new(),
            personal_account: String::new(),
            deployed: true,
            nonce: 0,
            operator_wallets: vec!["rOperator".into()],
            transfer_fee_drops: 200_000,
            redeem_fee_drops: 1_500_000,
            fxrp_decimals: 6,
            fxrp_balance_units: 0,
            vaults: vec![],
        };
        let p = crate::xrpl::SmartAccountInstructionParams {
            redeem: false,
            value: 1,
            recipient: String::new(),
            operator_xrpl: "rOperator".into(),
            fee_drops: 200_000,
            vault: None,
            skip_mainnet_prompt: false,
            network: crate::network::Network::Testnet,
        };
        assert!(compare_smart_account_terms(&info, &p).is_ok());
        let mut bumped = p.clone();
        bumped.fee_drops = 100;
        assert!(
            compare_smart_account_terms(&info, &bumped)
                .unwrap_err()
                .contains("fee changed")
        );
        let mut other_op = p.clone();
        other_op.operator_xrpl = "rEvil".into();
        assert!(
            compare_smart_account_terms(&info, &other_op)
                .unwrap_err()
                .contains("operator")
        );
    }

    /// TC-240: the RPC-supplied Smart Account operator is paid only when it equals the pinned
    /// `[flare.smart_accounts] expected_operator`; mismatch, unset and empty pins are refused.
    #[test]
    fn check_operator_requires_pinned_match() {
        assert!(check_operator("rOperator", Some(" rOperator ")).is_ok());
        assert!(
            check_operator("rEvil", Some("rOperator"))
                .unwrap_err()
                .contains("differs")
        );
        assert!(
            check_operator("rOperator", None)
                .unwrap_err()
                .contains("expected_operator")
        );
        assert!(check_operator("rOperator", Some("  ")).is_err());
    }

    /// Serve the Flare views `fetch_smart_account_info` reads (registry → controller / AssetManager,
    /// personal account, nonce, operators, fees, FXRP) from a local JSON-RPC mock.
    async fn smart_account_fetch_against_mock(
        balance: U256,
        decimals: u8,
        code: &'static str,
    ) -> (
        color_eyre::Result<SmartAccountInfo>,
        color_eyre::Result<Vec<serde_json::Value>>,
    ) {
        use alloy::sol_types::SolCall;
        use serde_json::json;

        let controller = address!("00000000000000000000000000000000000000c1");
        let asset_manager = address!("00000000000000000000000000000000000000a1");
        let fxrp = address!("00000000000000000000000000000000000000f1");
        let personal = address!("00000000000000000000000000000000000000e1");
        let vault = address!("00000000000000000000000000000000000000d1");
        let respond = move |req: &serde_json::Value| {
            let result = match req["method"].as_str().unwrap_or_default() {
                "eth_getCode" => json!(code),
                "eth_call" => {
                    let call = &req["params"][0];
                    let to: Address = call["to"].as_str().unwrap().parse().unwrap();
                    let input = call["input"].as_str().or(call["data"].as_str()).unwrap();
                    let input = hex::decode(input.trim_start_matches("0x")).unwrap();
                    let out = if to == FLARE_CONTRACT_REGISTRY {
                        let name =
                            FlareContractRegistry::getContractAddressByNameCall::abi_decode(&input)
                                .unwrap()
                                ._name;
                        let addr = match name.as_str() {
                            MASTER_ACCOUNT_CONTROLLER_NAME => controller,
                            ASSET_MANAGER_FXRP_NAME => asset_manager,
                            other => panic!("unexpected registry name {other}"),
                        };
                        FlareContractRegistry::getContractAddressByNameCall::abi_encode_returns(
                            &addr,
                        )
                    } else if to == asset_manager {
                        IAssetManager::fAssetCall::abi_encode_returns(&fxrp)
                    } else if to == vault {
                        if input[..4] == IERC20::balanceOfCall::SELECTOR {
                            IERC20::balanceOfCall::abi_encode_returns(&U256::from(7_000_000))
                        } else {
                            IVault::convertToAssetsCall::abi_encode_returns(&U256::from(7_100_000))
                        }
                    } else if to == fxrp {
                        if input[..4] == IERC20::decimalsCall::SELECTOR {
                            IERC20::decimalsCall::abi_encode_returns(&decimals)
                        } else {
                            IERC20::balanceOfCall::abi_encode_returns(&balance)
                        }
                    } else {
                        assert_eq!(to, controller);
                        let selector: [u8; 4] = input[..4].try_into().unwrap();
                        match selector {
                            IMasterAccountController::getPersonalAccountCall::SELECTOR => {
                                IMasterAccountController::getPersonalAccountCall::abi_encode_returns(
                                    &personal,
                                )
                            }
                            IMasterAccountController::getNonceCall::SELECTOR => {
                                IMasterAccountController::getNonceCall::abi_encode_returns(
                                    &U256::from(5),
                                )
                            }
                            IMasterAccountController::getXrplProviderWalletsCall::SELECTOR => {
                                IMasterAccountController::getXrplProviderWalletsCall::abi_encode_returns(
                                    &vec!["rOperator".to_string(), "rSecond".to_string()],
                                )
                            }
                            IMasterAccountController::getInstructionFeeCall::SELECTOR => {
                                let id = IMasterAccountController::getInstructionFeeCall::abi_decode(&input)
                                    .unwrap()
                                    ._instructionId;
                                let fee = if id == U256::from(1) { 200_000_u64 } else { 1_500_000 };
                                IMasterAccountController::getInstructionFeeCall::abi_encode_returns(
                                    &U256::from(fee),
                                )
                            }
                            IMasterAccountController::getVaultsCall::SELECTOR => {
                                IMasterAccountController::getVaultsCall::abi_encode_returns(
                                    &IMasterAccountController::getVaultsReturn {
                                        vaultIds: vec![U256::from(1)],
                                        vaultAddresses: vec![vault],
                                        vaultTypes: vec![1],
                                    },
                                )
                            }
                            other => panic!("unexpected controller selector {other:?}"),
                        }
                    };
                    json!(format!("0x{}", hex::encode(out)))
                }
                other => panic!("unexpected local RPC method: {other}"),
            };
            json!({"jsonrpc": "2.0", "id": req["id"].clone(), "result": result})
        };

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        // 2 registry + getPersonalAccount + fAsset + nonce/wallets/2 fees/balance/decimals/code
        // + getVaults + vault shares / assets.
        let server = tokio::spawn(crate::test_support::rpc_mock::serve_raw(
            listener, 14, respond,
        ));
        let info = fetch_smart_account_info(&url, "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh").await;
        let requests = if info.is_ok() {
            server.await.expect("join")
        } else {
            server.abort();
            Ok(Vec::new())
        };
        (info, requests)
    }

    /// TC-339: vault positions (zero shares / failing read skipped)
    /// TC-241: `fetch_smart_account_info` decodes the controller views (personal account,
    /// deployed flag from `eth_getCode`, nonce, operators, both instruction fees, FXRP balance)
    /// and rejects an out-of-range remote value with an error instead of a panic.
    #[tokio::test]
    async fn fetch_smart_account_info_decodes_views_against_local_rpc() {
        let (info, requests) =
            smart_account_fetch_against_mock(U256::from(12_500_000), 6, "0x6080").await;
        let info = info.expect("fetch");
        assert_eq!(
            info,
            SmartAccountInfo {
                xrpl_owner: "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into(),
                controller: "0x00000000000000000000000000000000000000c1".into(),
                personal_account: "0x00000000000000000000000000000000000000e1".into(),
                deployed: true,
                nonce: 5,
                operator_wallets: vec!["rOperator".into(), "rSecond".into()],
                transfer_fee_drops: 200_000,
                redeem_fee_drops: 1_500_000,
                fxrp_decimals: 6,
                fxrp_balance_units: 12_500_000,
                vaults: vec![SmartAccountVault {
                    id: 1,
                    address: "0x00000000000000000000000000000000000000d1".into(),
                    kind: 1,
                    shares: 7_000_000,
                    assets: Some(7_100_000),
                }],
            }
        );
        let requests = requests.expect("serve");
        assert_eq!(requests.len(), 14);
        assert!(requests.iter().any(|r| r["method"] == "eth_getCode"));

        // Undeployed account: empty code.
        let (info, _) = smart_account_fetch_against_mock(U256::ZERO, 6, "0x").await;
        assert!(!info.expect("fetch").deployed);

        // A uint256 that does not fit u128 is an error, not a panic or a wrapped value.
        let (info, _) = smart_account_fetch_against_mock(U256::MAX, 6, "0x6080").await;
        let err = info.unwrap_err().to_string();
        assert!(err.contains("out-of-range FXRP balance"), "{err}");

        // `decimals()` comes from the token, not an assumption: 8 and 0 are carried through.
        for decimals in [8_u8, 0] {
            let (info, _) =
                smart_account_fetch_against_mock(U256::from(1_234), decimals, "0x6080").await;
            let info = info.expect("fetch");
            assert_eq!(
                (info.fxrp_decimals, info.fxrp_balance_units),
                (decimals, 1_234)
            );
        }
        // decimals() above 18 is refused rather than overflowing later arithmetic.
        let (info, _) = smart_account_fetch_against_mock(U256::from(1), 19, "0x6080").await;
        let err = info.unwrap_err().to_string();
        assert!(err.contains("out-of-range FXRP decimals"), "{err}");

        // A malformed owner never reaches the RPC.
        let err = fetch_smart_account_info("http://127.0.0.1:1", "not-an-address")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("XRPL owner"), "{err}");
    }

    /// An oversized uint256 from the RPC must not panic the display formatters.
    #[test]
    fn oversized_uint256_does_not_panic_formatters() {
        // Saturates to u128::MAX rather than panicking or wrapping.
        assert_eq!(
            format_price(U256::MAX, 6),
            format_price(U256::from(u128::MAX), 6)
        );
        assert_eq!(
            format_token_balance(U256::MAX, 18),
            format_token_balance(U256::from(u128::MAX), 18)
        );
        assert_eq!(format_price(U256::from(1_500_000_u64), 6), "1.500000");
    }

    #[test]
    fn format_token_units_is_exact_for_any_decimals() {
        assert_eq!(format_token_units(12_500_000, 6), "12.5");
        assert_eq!(format_token_units(1_250_000_000, 8), "12.5");
        assert_eq!(format_token_units(1, 6), "0.000001");
        assert_eq!(format_token_units(1, 18), "0.000000000000000001");
        assert_eq!(format_token_units(1_234, 0), "1234");
        assert_eq!(format_token_units(3_000_000, 6), "3");
        assert_eq!(format_token_units(u128::MAX, 18).len(), 40);
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

    /// TC-172: FTSO feed normalization
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
        // Only the response is wrong (valid 32-byte proof entry): ABI bytes instead of decoded JSON.
        let mut old_shape: serde_json::Value = serde_json::from_str(REAL_DA_PROOF).unwrap();
        old_shape["response"] = serde_json::json!("0xbb");
        assert!(parse_fdc_payment_proof_json(&old_shape.to_string()).is_err());
    }

    /// TC-124: resolved registry addresses are cached per `(RPC URL, contract name)` — a hit never
    /// touches the network, and another URL or another contract name is a miss (provider is
    /// unreachable). Names outside the three hot ones (e.g. `FdcHub`) cache too.
    #[tokio::test]
    async fn registry_cache_is_keyed_by_rpc_url_and_contract() {
        let provider = ProviderBuilder::new().connect_http("http://127.0.0.1:1".parse().unwrap());
        let (url_a, url_b) = ("test://registry-cache-a", "test://registry-cache-b");
        let addr = address!("00000000000000000000000000000000000000aa");
        for name in [FTSO_V2_REGISTRY_NAME, "FdcHub"] {
            flare_registry_cache()
                .lock()
                .unwrap()
                .insert((url_a.to_string(), name.to_string()), addr);
            let hit = resolve_registry_contract(provider.clone(), url_a, name).await;
            assert_eq!(hit.expect("cache hit"), addr, "{name}");
        }
        for (url, name) in [
            (url_a, ASSET_MANAGER_FXRP_NAME),
            (url_b, FTSO_V2_REGISTRY_NAME),
        ] {
            assert!(
                resolve_registry_contract(provider.clone(), url, name)
                    .await
                    .is_err(),
                "{url} {name} must miss the cache"
            );
        }
    }

    /// TC-346: write chain gate on reported chain id
    /// TC-360: chain ids are 14 / 19 / 114; a write needs the reported id to equal the configured
    /// network's, and Flare mainnet (by reported id) needs `--yes`.
    #[test]
    fn write_gate_checks_reported_chain_id() {
        use FlareNetwork::{Coston2, Flare, Songbird};
        assert_eq!(
            [Flare, Songbird, Coston2].map(expected_chain_id),
            [14, 19, 114]
        );
        // RPC says Flare mainnet, config says coston2: refused even with --yes.
        let err = check_write_chain(Coston2, 14, true).unwrap_err();
        assert!(err.contains("chain id 14") && err.contains("114"), "{err}");
        assert!(check_write_chain(Flare, 114, true).is_err());
        assert!(check_chain_id(Songbird, 14).is_err());
        assert!(
            check_write_chain(Flare, 14, false)
                .unwrap_err()
                .contains("--yes")
        );
        assert!(check_write_chain(Flare, 14, true).is_ok());
        assert!(check_write_chain(Coston2, 114, false).is_ok());
        assert!(check_write_chain(Songbird, 19, false).is_ok());
    }

    /// TC-360: `executeDirectMinting` asks `eth_chainId` first and stops on a mismatch.
    #[tokio::test]
    async fn execute_direct_minting_refuses_wrong_chain_before_anything_else() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let respond = |req: &serde_json::Value| {
            assert_eq!(req["method"], "eth_chainId");
            serde_json::json!({"jsonrpc": "2.0", "id": req["id"].clone(), "result": "0xe"})
        };
        let server = tokio::spawn(crate::test_support::rpc_mock::serve_raw(
            listener, 1, respond,
        ));
        let gate = EvmGate {
            network: FlareNetwork::Coston2,
            yes: true,
        };
        let err = execute_direct_minting(&url, &"11".repeat(32), gate, REAL_DA_PROOF)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("chain id 14"), "{err}");
        server.await.expect("join").expect("exactly one request");
    }

    /// TC-346: send_capped fee cap
    /// TC-361: the gas cost cap is `gas × max fee`, saturating.
    #[test]
    fn tx_fee_cap() {
        assert!(check_tx_fee(2_000_000, 100_000_000_000).is_ok());
        assert!(check_tx_fee(10_000_000, MAX_TX_FEE_WEI / 10_000_000).is_ok());
        assert!(check_tx_fee(10_000_001, MAX_TX_FEE_WEI / 10_000_000).is_err());
        assert!(check_tx_fee(u64::MAX, u128::MAX).is_err());
    }

    /// TC-172: batched getFeedsById fetch (live)
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
        // The timestamp is unix seconds and fresh (the panel's `Age` column relies on both).
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        for p in &prices {
            assert!(
                now.abs_diff(p.timestamp) < 120,
                "{} timestamp {} vs now {now}",
                p.pair,
                p.timestamp
            );
        }

        Ok(())
    }

    /// TC-172: unknown feed fails cleanly (live)
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
        // The pause views exist on the deployed AssetManager (a missing view would be `None`).
        assert!(info.minting_paused.is_some() && info.emergency_pause_level.is_some());
        // The mint-limit batch (incl. the full `getSettings()` struct) decodes on the real contract.
        let limits = info.limits.expect("mint limits decode on mainnet");
        assert!(limits.granularity_uba > 0 && limits.large_threshold_uba > 0);
        println!("{limits:?}");
        Ok(())
    }

    /// TC-330: a Direct Mint Payment must exceed max(min fee, bips share) + executor fee; a
    /// payment below the minimum fee is minted entirely to the fee receiver on Flare.
    #[test]
    fn direct_mint_amount_must_exceed_fees() {
        // min fee 0.1 XRP, 0.1 %, executor 0.2 XRP → small payments need > 0.3 XRP.
        assert!(check_direct_mint_amount(50_000, 100_000, 10, 0).is_err());
        assert!(check_direct_mint_amount(300_000, 100_000, 10, 200_000).is_err());
        assert!(check_direct_mint_amount(300_001, 100_000, 10, 200_000).is_ok());
        // Large payment: the percentage share (1 XRP of 1000 XRP) exceeds the minimum fee.
        assert!(check_direct_mint_amount(1_000_000_000, 100_000, 10, 200_000).is_ok());
        assert!(check_direct_mint_amount(1_200_000, 100_000, 10_000, 200_000).is_err());
    }

    /// TC-331: the C3 receipt is classified from AssetManager events only — a delayed mint is not
    /// "minted", a too-small payment is flagged, and a lookalike log from another address is ignored.
    #[test]
    fn direct_minting_outcome_is_decoded_from_asset_manager_events() {
        use alloy::primitives::B256;
        use alloy::sol_types::SolEvent;

        let am = address!("00000000000000000000000000000000000000a1");
        let other = address!("00000000000000000000000000000000000000b2");
        let log = |addr: Address, data: alloy::primitives::LogData| alloy::rpc::types::Log {
            inner: alloy::primitives::Log {
                address: addr,
                data,
            },
            ..Default::default()
        };
        let delayed = IAssetManager::DirectMintingDelayed {
            transactionId: B256::ZERO,
            amount: U256::from(5),
            executionAllowedAt: U256::from(1_700_000_000_u64),
        }
        .encode_log_data();
        let large = IAssetManager::LargeDirectMintingDelayed {
            transactionId: B256::ZERO,
            amount: U256::from(5),
            executionAllowedAt: U256::from(1_700_007_200_u64),
        }
        .encode_log_data();
        let small = IAssetManager::DirectMintingPaymentTooSmallForFee {
            transactionId: B256::ZERO,
            receivedAmountUBA: U256::from(50_000),
            minimumMintingFeeUBA: U256::from(100_000),
        }
        .encode_log_data();
        let executed = IAssetManager::DirectMintingExecuted {
            transactionId: B256::ZERO,
            targetAddress: other,
            executor: other,
            mintedAmountUBA: U256::from(9_800_000),
            mintingFeeUBA: U256::from(100_000),
            executorFeeUBA: U256::from(100_000),
        }
        .encode_log_data();

        assert_eq!(
            decode_direct_minting_outcome(&[log(am, delayed.clone())], am),
            DirectMintOutcome::Delayed {
                execution_allowed_at: 1_700_000_000
            }
        );
        assert_eq!(
            decode_direct_minting_outcome(&[log(am, large)], am),
            DirectMintOutcome::Delayed {
                execution_allowed_at: 1_700_007_200
            }
        );
        assert_eq!(
            decode_direct_minting_outcome(&[log(am, small)], am),
            DirectMintOutcome::PaymentTooSmall {
                received_uba: 50_000,
                minimum_uba: 100_000
            }
        );
        assert_eq!(
            decode_direct_minting_outcome(&[log(am, executed.clone())], am),
            DirectMintOutcome::Minted {
                minted_uba: 9_800_000
            }
        );
        // Same event from a different contract, and a receipt without events, are not trusted.
        assert_eq!(
            decode_direct_minting_outcome(&[log(other, executed), log(other, delayed)], am),
            DirectMintOutcome::Unknown
        );
        assert_eq!(
            decode_direct_minting_outcome(&[], am),
            DirectMintOutcome::Unknown
        );
    }

    /// TC-334: the limiter replay and mint classification follow the contract: stale windows
    /// drain by whole elapsed windows, the headroom is one granule below the limit, the cap
    /// reverts, a mint at the large threshold is delayed (inclusive) and skips the windows.
    #[test]
    fn mint_limits_replay_and_classify() {
        // Hourly limit 100 XRP, granule 1 drop, 40 XRP minted in the window that started at 3600.
        let used = 40_000_000;
        let limit = 100_000_000;
        assert_eq!(window_max_now(3700, 3600, used, 1, limit, 3600), 59_999_999);
        // Two whole windows later the 40 XRP has drained (unused capacity does not roll over).
        assert_eq!(
            window_max_now(3600 + 7200, 3600, used, 1, limit, 3600),
            99_999_999
        );
        // Over-full window (150 XRP minted) leaves nothing; never underflows.
        assert_eq!(window_max_now(3700, 3600, 150_000_000, 1, limit, 3600), 0);

        let limits = MintLimits {
            cap_uba: 1_000_000_000,
            supply_uba: 950_000_000,
            cap_headroom_uba: Some(50_000_000),
            window_max_now_uba: 20_000_000,
            large_threshold_uba: 4_000_000_000,
            large_delay_secs: 7200,
            granularity_uba: 1,
        };
        assert_eq!(classify_mint(20_000_000, &limits), MintClass::Immediate);
        assert_eq!(classify_mint(20_000_001, &limits), MintClass::DelayedWindow);
        assert_eq!(classify_mint(50_000_001, &limits), MintClass::CapExceeded);
        // The cap is checked first, even for an amount that would also be "large".
        assert_eq!(
            classify_mint(4_000_000_000, &limits),
            MintClass::CapExceeded
        );
        let no_cap = MintLimits {
            cap_headroom_uba: None,
            ..limits
        };
        assert_eq!(
            classify_mint(3_999_999_999, &no_cap),
            MintClass::DelayedWindow
        );
        assert_eq!(
            classify_mint(4_000_000_000, &no_cap),
            MintClass::DelayedLarge { secs: 7200 }
        );
    }

    /// TC-339: mainnet getVaults decode (live)
    /// The registered Smart Account vault list decodes on the real controller (mainnet): ids,
    /// addresses and types line up, and a fresh account holds no position.
    #[tokio::test]
    #[ignore = "live network dependency"]
    async fn smart_account_vaults_decode_live() -> color_eyre::Result<()> {
        let provider = ProviderBuilder::new().connect_http(DEFAULT_FLARE_RPC.parse()?);
        let controller_addr = resolve_registry_contract(
            provider.clone(),
            DEFAULT_FLARE_RPC,
            MASTER_ACCOUNT_CONTROLLER_NAME,
        )
        .await?;
        let controller = IMasterAccountController::new(controller_addr, provider.clone());
        let list = controller.getVaults().call().await?;
        eprintln!(
            "vaults: {:?} {:?} {:?}",
            list.vaultIds, list.vaultAddresses, list.vaultTypes
        );
        assert!(!list.vaultAddresses.is_empty());
        assert_eq!(list.vaultIds.len(), list.vaultAddresses.len());
        assert_eq!(list.vaultTypes.len(), list.vaultAddresses.len());
        assert!(list.vaultTypes.iter().all(|t| matches!(t, 1 | 2)));
        // A zero-balance personal account yields no positions.
        let none = fetch_vault_positions(&provider, Address::repeat_byte(0x11), &list).await;
        assert!(none.is_empty());
        Ok(())
    }

    /// The 2026-10-11 Coston2 smoke `executeDirectMinting` receipt (docs/test.md) decodes with the
    /// declared event ABI: guards against a wrong event signature that self-encoded logs cannot catch.
    #[tokio::test]
    #[ignore = "live network dependency"]
    async fn coston2_direct_mint_receipt_decodes_live() -> color_eyre::Result<()> {
        let rpc = crate::config::FlareNetwork::Coston2.rpc_url();
        let provider = ProviderBuilder::new().connect_http(rpc.parse()?);
        let am = resolve_asset_manager_fxrp_address(provider.clone(), rpc).await?;
        let hash: alloy::primitives::B256 =
            "0x1460dc9ae36bbe58beb4a8fd1a6d6bd358d4fd3f5a315583646cd31d111e5f02".parse()?;
        let receipt = provider
            .get_transaction_receipt(hash)
            .await?
            .ok_or_else(|| color_eyre::eyre::eyre!("smoke receipt not found on Coston2"))?;
        let outcome = decode_direct_minting_outcome(receipt.inner.logs(), am);
        println!("{outcome}");
        assert!(
            matches!(outcome, DirectMintOutcome::Minted { .. }),
            "{outcome:?}"
        );
        Ok(())
    }

    /// TC-337: queue aggregation (totals, whole lots, per-agent order, saturation) and the
    /// `getSettings()` word picker (layout check, bounds).
    #[test]
    fn redemption_queue_summary_and_settings_words() {
        let a = Address::repeat_byte(0xaa);
        let b = Address::repeat_byte(0xbb);
        let s = summarize_queue(
            &[(a, 10_000_000), (b, 30_000_000), (a, 25_000_000)],
            20_000_000,
        );
        assert_eq!(s["tickets"], 3);
        assert_eq!(s["total_uba"], "65000000");
        assert_eq!(s["total_xrp"], "65.000000");
        assert_eq!(s["lots"], "3");
        assert_eq!(s["agents"], 2);
        // `a` holds 35 XRP in 2 tickets, more than `b`'s 30 XRP in 1.
        assert_eq!(s["top_agents"][0]["tickets"], 2);
        assert_eq!(s["top_agents"][0]["value_xrp"], "35.000000");
        assert!(summarize_queue(&[], 0)["lots"].is_null());
        assert_eq!(
            summarize_queue(&[(a, u128::MAX), (a, 1)], 1)["total_uba"],
            u128::MAX.to_string()
        );

        let mut raw = vec![0u8; 32 * 40];
        raw[31] = 32;
        raw[32 * (1 + SETTINGS_MAX_REDEEMED_TICKETS_WORD) + 31] = 20;
        assert_eq!(
            settings_word(&raw, SETTINGS_MAX_REDEEMED_TICKETS_WORD).unwrap(),
            U256::from(20)
        );
        assert!(settings_word(&raw[..64], SETTINGS_MAX_REDEEMED_TICKETS_WORD).is_err());
        raw[31] = 64;
        assert!(
            settings_word(&raw, 0)
                .unwrap_err()
                .to_string()
                .contains("layout")
        );
    }
}
