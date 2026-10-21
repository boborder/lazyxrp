use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;

/// Seconds between Unix epoch (1970-01-01) and Ripple epoch (2000-01-01).
pub(crate) const RIPPLE_EPOCH_UNIX: i64 = 946_684_800;

/// Newtype wrapper so `Arc<serde_json::Value>` can derive `Serialize`/`Deserialize`.
///
/// # Immutability contract
///
/// Instances are **shared across components** (e.g. `TxRow.tx_json` is passed to both
/// TxHistory panel and TxDetail overlay). **NEVER mutate** shared `ArcValue` via
/// `Arc::make_mut` — clone the inner value first if modification is needed.
/// Invariant I-10 (`docs/security.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArcValue(pub Arc<serde_json::Value>);

impl ArcValue {
    pub fn new(v: serde_json::Value) -> Self {
        Self(Arc::new(v))
    }
}

impl Default for ArcValue {
    fn default() -> Self {
        Self(Arc::new(serde_json::Value::Null))
    }
}

impl serde::Serialize for ArcValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for ArcValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        serde_json::Value::deserialize(deserializer).map(|v| Self(Arc::new(v)))
    }
}

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::config::FALLBACK_CURRENCY_CODE;
use crate::network::Network;

/// Validator list reported by the connected `rippled` (`server_info.info.validator_list`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeValidatorListSummary {
    pub count: u32,
    pub status: String,
    pub expiration: String,
}

/// One validator entry from the XRPLF dUNL blob.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DunlValidatorRow {
    /// `validation_public_key` from the UNL blob (hex, typically `ED…`).
    pub validation_public_key: String,
    /// Base64 manifest blob was present in the UNL entry.
    pub has_manifest: bool,
    /// Claimed domain from the embedded manifest (`sfDomain`), if present.
    pub domain: Option<String>,
    /// Manifest sequence (`sfSequence`) when parseable.
    pub sequence: Option<u32>,
    /// Master key from manifest (`sfPublicKey` blob), hex-encoded.
    pub master_public_key: Option<String>,
}

impl DunlValidatorRow {
    /// Master key differs from the signing (validation) key.
    pub fn master_differs_from_signing(&self) -> bool {
        match self.master_public_key.as_deref() {
            Some(m) => !m.eq_ignore_ascii_case(&self.validation_public_key),
            None => false,
        }
    }
}

/// Aggregated dUNL table stats for the Server panel header/footer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DunlStats {
    pub total: usize,
    pub with_manifest: usize,
    pub with_domain: usize,
    pub master_distinct: usize,
}

/// XRPL Foundation decentralized UNL (`https://unl.xrplf.org`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DunlSummary {
    pub validator_count: u32,
    pub sequence: u64,
    /// Ripple-epoch seconds from the blob (`expiration` field).
    pub expiration_ripple: u64,
    /// Human-readable UTC expiry (from blob `expiration` ripple time).
    pub expiration_utc: String,
    /// When this summary was parsed from the dUNL blob — data generation time,
    /// not receive time (the poll task re-serves the cached summary on errors).
    pub fetched_at: SystemTime,
    pub validators: Vec<DunlValidatorRow>,
}

impl DunlSummary {
    pub fn stats(&self) -> DunlStats {
        let total = self.validators.len();
        let mut with_manifest = 0usize;
        let mut with_domain = 0usize;
        let mut master_distinct = 0usize;
        for v in &self.validators {
            if v.has_manifest {
                with_manifest += 1;
            }
            if v.domain.is_some() {
                with_domain += 1;
            }
            if v.master_differs_from_signing() {
                master_distinct += 1;
            }
        }
        DunlStats {
            total,
            with_manifest,
            with_domain,
            master_distinct,
        }
    }

    /// Whole days until blob expiry (negative if already expired).
    pub fn days_until_expiry(&self) -> Option<i64> {
        let expiry_unix =
            RIPPLE_EPOCH_UNIX.saturating_add(self.expiration_ripple.min(i64::MAX as u64) as i64);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs() as i64;
        Some((expiry_unix - now) / 86_400)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerInfoSummary {
    pub ledger_index: u32,
    pub hostid: String,
    /// rippled/Clio `build_version` from `server_info` (empty when absent).
    pub build_version: String,
    pub validation_quorum: Option<u32>,
    pub validator_list: Option<NodeValidatorListSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeeSummary {
    pub open_ledger_fee_drops: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountSummary {
    pub account: String,
    pub balance_xrp: String,
    pub sequence: u32,
    pub owner_count: u32,
    /// AccountRoot flags bitmask (lsfDepositAuth, lsfRequireDestTag, etc.).
    pub flags: u32,
    /// Regular key address, if set.
    pub regular_key: Option<String>,
    /// Domain hex string, if set (lowercase ASCII hex).
    pub domain_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OfferRow {
    pub quality: String,
    pub price: String,
    pub taker_gets: String,
    pub taker_pays: String,
    /// Raw book_offers entry for detail popup.
    #[serde(skip)]
    pub raw_json: ArcValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TxSummary {
    pub hash: String,
}

/// Result of a `simulate` RPC call — dry-run of an unsigned transaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SimulateResult {
    /// Auto-filled transaction JSON (Fee, Sequence, etc. populated by server).
    pub tx_json: serde_json::Value,
    /// Engine result code (e.g. `"tesSUCCESS"`, `"tecPATH_DRY"`).
    pub engine_result: String,
    /// Human-readable engine result message.
    pub engine_result_message: String,
    /// Ledger index used for the simulation.
    pub ledger_index: u32,
    /// Transaction metadata (present for tesSUCCESS and some tec codes).
    pub meta: Option<serde_json::Value>,
}

/// Display row for Path-Find panel (built from `ripple_path_find` alternatives).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathFindRow {
    pub send: String,
    pub hops: String,
    pub path: String,
    pub raw_json: ArcValue,
}

/// Eq-friendly snapshot for `Action::XrplPathFind` (panel + poll).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathFindSnapshot {
    pub dest_summary: String,
    pub rows: Vec<PathFindRow>,
}

/// One path alternative returned by `ripple_path_find`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathAlternative {
    /// Computed payment paths (array of path arrays).
    pub paths_computed: serde_json::Value,
    /// The amount that would need to be sent from the source account,
    /// given the destination amount and the computed paths.
    pub source_amount: serde_json::Value,
}

/// Result of a `ripple_path_find` RPC call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RipplePathFindResult {
    /// All available path alternatives.
    pub alternatives: Vec<PathAlternative>,
    /// The destination account (echoed from request).
    pub destination_account: String,
    /// The destination amount object (echoed from request).
    pub destination_amount: serde_json::Value,
    /// The source account that would send the payment.
    pub source_account: String,
}

/// Key generation result (TUI: local via `signing::generate_wallet_keys_local`; RPC parser in `client`).
///
/// Security: `Debug` redacts `master_seed` / `master_seed_hex`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedWalletKeys {
    /// Master seed (Ed25519 family seed, starts with `s`).
    pub master_seed: String,
    /// Hex-encoded master seed.
    pub master_seed_hex: String,
    /// The derived XRPL address (starts with `r`).
    pub account_id: String,
    /// Public key (starts with `a` for Ed25519).
    pub public_key: String,
    /// Key algorithm: `"ed25519"` or `"secp256k1"`.
    pub key_type: String,
}

impl std::fmt::Debug for GeneratedWalletKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeneratedWalletKeys")
            .field("master_seed", &"[REDACTED]")
            .field("master_seed_hex", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .field("public_key", &self.public_key)
            .field("key_type", &self.key_type)
            .finish()
    }
}

/// Seed copies (Action clones, popup state) are wiped when dropped.
impl Drop for GeneratedWalletKeys {
    fn drop(&mut self) {
        use secrecy::zeroize::Zeroize;
        self.master_seed.zeroize();
        self.master_seed_hex.zeroize();
    }
}

/// NFToken mint flag: URI may be updated via `NFTokenModify` (dynamic NFT / dNFT).
pub const NFTOKEN_FLAG_MUTABLE: u32 = 0x0000_0010;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NftRow {
    pub nft_id: String,
    pub taxon: u32,
    pub serial: u32,
    pub transfer_fee: u16,
    pub uri: String,
    /// `true` when `Flags` includes [`NFTOKEN_FLAG_MUTABLE`] (tfMutable).
    #[serde(default)]
    pub is_mutable: bool,
    /// Raw `account_nfts` entry for detail popup.
    #[serde(skip)]
    pub raw_json: ArcValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustLineRow {
    pub currency: String,
    pub account: String,
    pub balance: String,
    pub limit: String,
    /// Raw `account_lines` entry for detail popup.
    #[serde(skip)]
    pub raw_json: ArcValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AmmSummary {
    pub asset1: String,
    pub asset2: String,
    pub lp_token: String,
    pub trading_fee: u16,
    pub pool1: String,
    pub pool2: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookMidPrice {
    pub bid: String,
    pub ask: String,
    pub mid: String,
}

/// One oracle identifier for `get_aggregate_price`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OracleId {
    pub account: String,
    pub oracle_document_id: u32,
}

/// Base/quote pair for oracle price aggregation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OraclePricePair {
    pub base_asset: String,
    pub quote_asset: String,
}

/// Display name of an XRPL currency code; 40-hex (160-bit) codes are decoded:
///
/// - all zero → `XRP` (the native asset);
/// - standard form (first 12 and last 5 bytes zero, 3 ASCII letters between) → the 3 letters
///   (`XRP` excluded: it is the native asset's name, never an issued code);
/// - non-standard form that is printable ASCII, 4+ characters (`RLUSD`, `USDC`…) → that text;
/// - anything else, including non-standard codes that decode to only 1–3 characters → the first
///   8 hex digits plus `…`, so it can never read like a standard 3-letter code from another issuer.
///
/// Non-hex input is returned unchanged.
#[must_use]
pub fn asset_display_name(asset: &str) -> String {
    let Some(bytes) = hex::decode(asset).ok().filter(|b| b.len() == 20) else {
        return asset.to_string();
    };
    if bytes.iter().all(|b| *b == 0) {
        return "XRP".to_string();
    }
    let printable = |b: &[u8]| !b.is_empty() && b.iter().all(u8::is_ascii_graphic);
    let standard = bytes[..12].iter().chain(&bytes[15..]).all(|b| *b == 0);
    let text = &bytes[..bytes.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1)];
    if standard && printable(&bytes[12..15]) && &bytes[12..15] != b"XRP" {
        String::from_utf8_lossy(&bytes[12..15]).into_owned()
    } else if !standard && text.len() >= 4 && printable(text) {
        String::from_utf8_lossy(text).into_owned()
    } else {
        format!("{}…", asset[..8].to_ascii_uppercase())
    }
}

/// Price statistics subset (entire_set or trimmed_set).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PriceStats {
    pub mean: String,
    pub size: u32,
    pub standard_deviation: String,
}

/// Result of `get_aggregate_price`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AggregatePrice {
    pub entire_set: PriceStats,
    pub trimmed_set: Option<PriceStats>,
    pub time: u64,
    pub base_asset: String,
    pub quote_asset: String,
}

/// One FTSOv2 feed value fetched from Flare.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlareFeedPrice {
    pub pair: String,
    pub price: String,
    pub timestamp: u64,
    pub source: String,
}

/// Flare EVM wallet read snapshot for the Overview wallet panel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlareWalletSummary {
    pub address: String,
    pub native_balance_display: String,
    pub fxrp_balance_display: String,
    /// `[flare.fassets] execute` gate (C3 writes allowed when true).
    pub execute_enabled: bool,
    /// Whether the configured executor env var is set in this session.
    pub executor_key_configured: bool,
}

/// Direct Mint throttles read from AssetManagerFXRP, already replayed to the block time
/// (the limiter views are stale between writes). All amounts in UBA (XRP drops).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MintLimits {
    /// `mintingCapAMG × granularity`; 0 = cap disabled.
    pub cap_uba: u128,
    /// FXRP total supply (the cap headroom derived from it is an upper bound).
    pub supply_uba: u128,
    /// Largest single mint the global cap still accepts (`None` = cap disabled).
    pub cap_headroom_uba: Option<u128>,
    /// Largest mint that neither window delays right now (min of hourly / daily).
    pub window_max_now_uba: u128,
    /// Mints at or above this are delayed by `large_delay_secs`.
    pub large_threshold_uba: u128,
    pub large_delay_secs: u64,
    /// `assetMintingGranularityUBA` (mint amounts are floored to it).
    pub granularity_uba: u128,
}

/// FXRP Direct Mint read-only snapshot from AssetManagerFXRP (C1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FxrpDirectMintInfo {
    /// XRPL classic address of the Core Vault payment destination.
    pub core_vault_xrpl: String,
    /// Flare AssetManagerFXRP contract address (0x…).
    pub asset_manager: String,
    /// Minimum direct-mint fee in underlying base units (UBA; XRP drops).
    pub min_fee_uba: u128,
    /// Direct mint fee in BIPS (100 = 1%).
    pub fee_bips: u64,
    /// Preferred-executor fee in UBA (XRP drops).
    pub executor_fee_uba: u128,
    /// `mintingPaused()` (governance mint-only pause); `None` when the contract has no such view.
    pub minting_paused: Option<bool>,
    /// `emergencyPauseLevel()` (0 = none, 1 = start operations, 2 = full, 3 = full + transfers);
    /// `None` when the view is missing.
    pub emergency_pause_level: Option<u8>,
    /// Mint rate-limit / cap snapshot; `None` when any of its views could not be read.
    pub limits: Option<MintLimits>,
}

/// Flare Smart Account read snapshot for one XRPL owner (`MasterAccountController` views).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SmartAccountInfo {
    /// XRPL classic address the personal account belongs to.
    pub xrpl_owner: String,
    /// `MasterAccountController` address resolved via the Flare contract registry.
    pub controller: String,
    /// `getPersonalAccount(xrpl_owner)` (deterministic; valid before deployment).
    pub personal_account: String,
    /// Whether the personal account has code on Flare (deployed on its first instruction).
    pub deployed: bool,
    /// `getNonce(personal_account)`: memo-instruction nonce.
    pub nonce: u128,
    /// `getXrplProviderWallets()`: XRPL operator addresses; instructions go to `[0]`.
    pub operator_wallets: Vec<String>,
    /// `getInstructionFee(1)` in XRP drops (FXRP transfer).
    pub transfer_fee_drops: u64,
    /// `getInstructionFee(2)` in XRP drops (FXRP redeem).
    pub redeem_fee_drops: u64,
    /// FXRP `decimals()` as read from the token (never assumed).
    pub fxrp_decimals: u8,
    /// FXRP `balanceOf(personal_account)` in base units (`10^-fxrp_decimals` FXRP).
    pub fxrp_balance_units: u128,
    /// Registered vault positions (`getVaults()`; Firelight / Upshift) of the personal account
    /// with a non-zero share balance. Advisory: empty when the vault reads fail.
    pub vaults: Vec<SmartAccountVault>,
}

/// One Smart Account vault position (vault list comes from `MasterAccountController.getVaults()`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SmartAccountVault {
    pub id: u64,
    pub address: String,
    /// `1` = Firelight, `2` = Upshift (`getVaults()` vault type).
    pub kind: u8,
    /// `balanceOf(personal_account)` in vault share base units.
    pub shares: u128,
    /// ERC-4626 `convertToAssets(shares)` in FXRP base units; `None` if the vault lacks it.
    pub assets: Option<u128>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TxRow {
    pub hash: String,
    pub tx_type: String,
    pub ledger_index: u32,
    pub result: String,
    /// Direction marker: "▼" outbound, "▲" inbound, "·" self-only.
    pub direction: String,
    /// Shared raw transaction JSON (avoid deep clones on Action routing).
    pub tx_json: ArcValue,
    /// Shared raw metadata JSON.
    pub meta_json: ArcValue,
}

/// Result page from `account_tx` (includes optional marker for pagination).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountTxPage {
    pub rows: Vec<TxRow>,
    pub marker: Option<serde_json::Value>,
}

/// One row from `account_objects` (Check, Ticket, MPToken, PayChannel, Escrow, …).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerObjectRow {
    pub ledger_type: String,
    pub index: String,
    pub detail: String,
    /// Raw ledger object entry for detail popup.
    #[serde(skip)]
    pub raw_json: ArcValue,
}

/// True for ledger types shown in the upper «misc objects» panel (excludes PayChannel / Escrow).
#[must_use]
pub fn is_misc_ledger_type(t: &str) -> bool {
    matches!(
        t,
        "Check"
            | "Ticket"
            | "MPToken"
            | "MPTokenIssuance"
            | "DepositPreauth"
            | "SignerList"
            | "DID"
    )
}

#[derive(Debug, Clone)]
pub struct BookPair {
    pub base: String,
    pub quote: String,
    pub quote_code: String,
    pub issuer: String,
    pub limit: u16,
}

impl BookPair {
    /// Base currency is what the taker gets (offer maker pays).
    pub fn gets_currency(&self) -> &str {
        &self.base
    }
    pub fn gets_issuer(&self) -> Option<&str> {
        if self.base.eq_ignore_ascii_case("XRP") {
            None
        } else {
            Some(&self.issuer)
        }
    }
    /// Quote currency is what the taker pays (offer maker gets).
    pub fn pays_currency(&self) -> &str {
        if self.quote.eq_ignore_ascii_case("XRP") {
            &self.quote
        } else if self.quote_code.trim().is_empty() {
            FALLBACK_CURRENCY_CODE
        } else {
            &self.quote_code
        }
    }
    pub fn pays_issuer(&self) -> Option<&str> {
        if self.quote.eq_ignore_ascii_case("XRP") {
            None
        } else {
            Some(&self.issuer)
        }
    }

    /// Preview amount for `ripple_path_find` (self-payment swap: receive 1 unit of quote).
    pub fn path_find_destination_amount_preview(&self) -> serde_json::Value {
        if self.quote.eq_ignore_ascii_case("XRP") {
            serde_json::json!("1000000")
        } else {
            serde_json::json!({
                "currency": self.pays_currency(),
                "issuer": self.pays_issuer().unwrap_or(self.issuer.as_str()),
                "value": "1",
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSetSubmitParams {
    /// Uppercase flag name (`RequireDest`, `DefaultRipple`, …) or empty / `(none)`.
    pub set_flag: Option<String>,
    pub clear_flag: Option<String>,
    pub domain_ascii: String,
    pub tick_size: String,
    pub transfer_rate: String,
    /// From CLI `--yes` via Wallet panel.
    pub skip_mainnet_prompt: bool,
    /// XRPL network the user composed this on; the poll task refuses it if the active network differs.
    pub network: Network,
}

/// XRP or IOU Payment from the Wallet modal.
///
/// **XRP mode** (iou_currency == None): `amount` = XRP value, classic payment.
/// **IOU mode** (iou_currency + iou_issuer set): `amount` = issued-currency value.
/// Self-payment DEX swap previews use `PathFind` / `ripple_path_find`, not this params type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentSubmitParams {
    pub destination: String,
    /// XRP amount (XRP mode) or IOU value (IOU mode).
    pub amount: String,
    /// If set, triggers IOU mode: `"USD"`, `"BTC"` etc.
    pub iou_currency: Option<String>,
    /// Issuer address for IOU mode.
    pub iou_issuer: Option<String>,
    /// Destination tag extracted from an X-address (or set explicitly).
    pub destination_tag: Option<u32>,
    /// Optional SendMax spec (`XRP:drops` or `CUR:issuer:value`) → cross-currency payment via default paths.
    pub send_max: Option<String>,
    pub skip_mainnet_prompt: bool,
    pub network: Network,
}

/// Payment composer quote request (`ripple_path_find` from the signing account).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentQuoteParams {
    pub destination: String,
    pub amount: String,
    pub iou_currency: Option<String>,
    pub iou_issuer: Option<String>,
    /// Raw Send max field; its currency prefix picks the source currency.
    pub send_max: String,
    pub slippage_bps: u32,
}

/// FXRP Direct Mint Payment: XRP to Core Vault with 32-byte recipient memo (C2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FxrpDirectMintPaymentParams {
    /// Core Vault XRPL classic address (from C1 / AssetManager).
    pub core_vault_xrpl: String,
    /// Fee views from the same C1 snapshot; the amount must leave something after them.
    pub min_fee_uba: u128,
    pub fee_bips: u64,
    pub executor_fee_uba: u128,
    /// Flare/EVM recipient address (`0x` + 40 hex).
    pub flare_recipient: String,
    /// XRP amount to send (fees deducted on Flare side from this payment).
    pub amount_xrp: String,
    /// Complete `MemoData` hex (no `0x`) replacing the recipient memo: Smart Account memo
    /// instructions (`0xE0`–`0xE2`, `0xD0`, `0xD1`) travel in a Direct Mint Payment too.
    pub custom_memo: Option<String>,
    pub skip_mainnet_prompt: bool,
    pub network: Network,
}

/// FXRP C3: paste FDC `XRPPayment` proof JSON and call `executeDirectMinting` (flagged).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FxrpExecuteDirectMintParams {
    /// DA layer `proof-by-request-round` answer `{response, proof}` (`response` = decoded
    /// attestation object), or the contract-shaped `{data, merkleProof}`.
    pub proof_json: String,
    pub skip_mainnet_prompt: bool,
    pub network: Network,
}

/// Smart Account proof-based FXRP instruction: Payment to the operator with a 32-byte memo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartAccountInstructionParams {
    /// `false` = `0x01` FXRP transfer, `true` = `0x02` FXRP redeem.
    pub redeem: bool,
    /// FXRP base units (transfer; raw on-chain `transfer` amount) or lots (redeem); at most 80 bits.
    pub value: u128,
    /// Flare recipient `0x…` (transfer only; ignored for redeem).
    pub recipient: String,
    /// Operator XRPL address from `getXrplProviderWallets()[0]` (pinned before signing).
    pub operator_xrpl: String,
    /// `getInstructionFee(id)` in XRP drops: the exact Payment `Amount`.
    pub fee_drops: u64,
    /// `Some` → a Firelight / Upshift vault instruction (`redeem` / `recipient` are ignored and
    /// `value` is the instruction's value field verbatim).
    pub vault: Option<SmartAccountVaultOp>,
    pub skip_mainnet_prompt: bool,
    pub network: Network,
}

/// Which vault instruction to encode (`id` = `0x11`–`0x13` Firelight, `0x21`–`0x23` Upshift).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartAccountVaultOp {
    pub instruction_id: u8,
    /// Vault id registered in `MasterAccountController.getVaults()`.
    pub vault_id: u16,
}

/// SetRegularKey from the Wallet form.
/// An empty `regular_key` means clear (remove) the existing regular key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetRegularKeySubmitParams {
    pub regular_key: String,
    pub skip_mainnet_prompt: bool,
    pub network: Network,
}

/// EscrowCreate from the Wallet composer (XRP only, time-based; no crypto-condition).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscrowCreateSubmitParams {
    pub destination: String,
    pub amount_xrp: String,
    /// Relative duration from submit time (`30m`, `2h`, `7d`); becomes `FinishAfter`.
    pub finish_after: String,
    /// Optional relative duration (empty = none); becomes `CancelAfter`, must exceed `finish_after`.
    pub cancel_after: String,
    pub skip_mainnet_prompt: bool,
    pub network: Network,
}

/// One escrow the signing account owns or receives (`account_objects` type `escrow`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscrowRow {
    pub owner: String,
    pub destination: String,
    /// Formatted amount (`1.5 XRP`).
    pub amount: String,
    /// Ripple-epoch seconds.
    pub finish_after: Option<u32>,
    pub cancel_after: Option<u32>,
    /// Crypto-condition escrows cannot be finished here (no fulfillment input).
    pub has_condition: bool,
    /// EscrowCreate's `Sequence` (or `TicketSequence`); `None` when it could not be resolved.
    pub offer_sequence: Option<u32>,
    /// The EscrowCreate tx (only EscrowCreate modifies an Escrow) — used to resolve `offer_sequence`.
    pub previous_txn_id: String,
}

/// EscrowFinish (`cancel == false`) or EscrowCancel from the Wallet composer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscrowCloseSubmitParams {
    pub owner: String,
    pub offer_sequence: u32,
    pub cancel: bool,
    pub skip_mainnet_prompt: bool,
    pub network: Network,
}

/// Two-asset AMMDeposit: amount specs (`XRP:drops` / `CUR:issuer:value`) are maxima and name the pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmmDepositSubmitParams {
    pub amount: String,
    pub amount2: String,
    pub skip_mainnet_prompt: bool,
    pub network: Network,
}

/// AMMWithdraw: pool assets (`XRP` / `CUR:issuer`); empty `lp_amount` = withdraw all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmmWithdrawSubmitParams {
    pub asset: String,
    pub asset2: String,
    pub lp_amount: String,
    pub skip_mainnet_prompt: bool,
    pub network: Network,
}

/// AccountDelete: classic or X-address (tag from the X-address). Irreversible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountDeleteSubmitParams {
    pub destination: String,
    pub skip_mainnet_prompt: bool,
    pub network: Network,
}

/// OfferCreate from the Wallet form.
/// taker_gets / taker_pays use compact spec: `"XRP:100"` (XRP) or `"USD:rIssuer:10"` (IOU).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferCreateSubmitParams {
    pub taker_gets: String,
    pub taker_pays: String,
    pub skip_mainnet_prompt: bool,
    pub network: Network,
}

/// One of the watched account's open DEX offers (`account_offers`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountOfferRow {
    pub seq: u32,
    pub taker_gets: String,
    pub taker_pays: String,
}

/// OfferCancel from the Wallet composer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferCancelSubmitParams {
    pub offer_sequence: u32,
    pub skip_mainnet_prompt: bool,
    pub network: Network,
}

/// TrustSet from the Wallet form (v1: Limit + Currency + Issuer only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustSetSubmitParams {
    pub currency: String,
    pub issuer: String,
    pub limit: String,
    pub skip_mainnet_prompt: bool,
    pub network: Network,
}

/// Any spec'd transaction type (`xrpl::txspec`): canonical field name → raw value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenericTxSubmitParams {
    pub tx_type: String,
    pub fields: std::collections::BTreeMap<String, String>,
    pub skip_mainnet_prompt: bool,
    pub network: Network,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollCommand {
    Account,
    Book,
    Nfts,
    Lines,
    TxHistory,
    /// Load next page using the given marker.
    TxHistoryMore(Option<serde_json::Value>),
    /// `account_objects` (limit 200); Assets panels filter by ledger type.
    LedgerObjects,
    /// Sign and submit AccountSet (wallet form).
    AccountSetSubmit(AccountSetSubmitParams),
    /// Sign and submit Payment (wallet modal).
    PaymentSubmit(PaymentSubmitParams),
    FxrpDirectMintPayment(FxrpDirectMintPaymentParams),
    /// Flare `executeDirectMinting` with pasted FDC proof (C3; gated by config).
    FxrpExecuteDirectMint(FxrpExecuteDirectMintParams),
    /// Smart Account FXRP transfer / redeem Payment to the operator.
    SmartAccountInstruction(SmartAccountInstructionParams),
    /// Sign and submit SetRegularKey (wallet form).
    SetRegularKeySubmit(SetRegularKeySubmitParams),
    /// Sign and submit EscrowCreate (wallet form).
    EscrowCreateSubmit(EscrowCreateSubmitParams),
    /// Escrows the signing account owns or receives (Wallet escrow composer).
    AccountEscrows,
    /// Sign and submit EscrowFinish / EscrowCancel.
    EscrowCloseSubmit(EscrowCloseSubmitParams),
    AmmDepositSubmit(AmmDepositSubmitParams),
    AmmWithdrawSubmit(AmmWithdrawSubmitParams),
    /// Sign and submit AccountDelete (wallet form, after typing DELETE).
    AccountDeleteSubmit(AccountDeleteSubmitParams),
    /// Sign and submit any spec'd transaction type (wallet "Transaction" composer).
    GenericTx(GenericTxSubmitParams),
    OfferCreateSubmit(OfferCreateSubmitParams),
    /// `account_offers` for the signing account (Wallet OfferCancel composer).
    AccountOffers,
    OfferCancelSubmit(OfferCancelSubmitParams),
    /// `ripple_path_find` quote → Send max spec for the Payment composer.
    PaymentQuote(PaymentQuoteParams),
    /// Sign and submit TrustSet (wallet form).
    TrustSetSubmit(TrustSetSubmitParams),
    /// Generate a new key pair locally ("ed25519" or "secp256k1").
    GenerateWalletKeys(String),
}

#[derive(Debug)]
pub struct PollContext {
    pub rpc_url: String,
    /// True when a custom `--server`/`XRPL_RPC_SERVER` was set: network switching
    /// keeps the custom endpoint instead of the per-network preset.
    pub custom_rpc: bool,
    pub watch_address: String,
    pub book_pair: BookPair,
    pub poll_interval: Duration,
    pub seed_address: Option<String>,
    /// Signing credential held by the poll task (never sent over Action/params channels).
    pub signing_credential: Option<crate::signing::SigningCredential>,
    pub network_watch: watch::Receiver<Network>,
    /// Active UI tab index (0 Overview …) for optional heavy-RPC skips.
    pub tab_watch: watch::Receiver<usize>,
    /// Oracle identifiers for `get_aggregate_price`.
    pub oracles: Vec<OracleId>,
    /// Price pairs to query via `get_aggregate_price`.
    pub oracle_pairs: Vec<OraclePricePair>,
    /// Optional Flare FTSOv2 RPC endpoint for Oracle tab integration.
    pub flare_rpc_url: Option<String>,
    /// FTSOv2 feed names (e.g. `FXRP/USD`).
    pub flare_feeds: Vec<String>,
    /// `[flare.fassets] expected_core_vault` — pinned Core Vault for FXRP Direct Mint Payments.
    pub flare_expected_core_vault: Option<String>,
    /// `[flare.fassets] execute` — when false, C3 execute path is refused.
    pub flare_fassets_execute: bool,
    /// `[flare] network` — C3 gates (`--yes`-equivalent mainnet guard, chain id, XRPL pairing) use it.
    pub flare_network: crate::config::FlareNetwork,
    /// `[flare.smart_accounts] expected_operator` — pinned operator for Smart Account instructions.
    pub flare_expected_operator: Option<String>,
    /// `[flare] display` — Off skips FTSO/FXRP read fetches (C3 execute URL unaffected).
    pub flare_display: crate::config::FlareDisplay,
    /// Env var name for Flare executor key (`[flare.fassets] evm_key_env`).
    pub flare_evm_key_env: String,
    /// Executor key captured by `Config::new()` (env removed at startup).
    pub flare_evm_key: Option<secrecy::SecretString>,
    /// `[flare.wallet] address` for Overview Flare wallet panel reads.
    pub flare_wallet_address: Option<String>,
    /// Serializes account_info → simulate → sign → submit for one wallet.
    pub submit_lock: Arc<Mutex<()>>,
}

/// Parsed subset of xrp-ledger.toml relevant to domain verification.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct XrplTomlData {
    pub domain: String,
    pub validator_found: bool,
    pub attestation: Option<String>,
    pub validator_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC-070: issued quote uses currency_code (160-bit), not display symbol
    #[test]
    fn book_pair_uses_currency_code_for_issued_quote() {
        let pair = BookPair {
            base: "XRP".into(),
            quote: "RLUSD".into(),
            quote_code: "524C555344000000000000000000000000000000".into(),
            issuer: "rIssuer".into(),
            limit: 5,
        };
        assert_eq!(
            pair.pays_currency(),
            "524C555344000000000000000000000000000000"
        );
        assert_eq!(pair.pays_issuer(), Some("rIssuer"));

        // XRP quote (any case): currency is the quote itself and there is no issuer,
        // even though `quote_code`/`issuer` are populated.
        let xrp = BookPair {
            base: "rIssuer".into(),
            quote: "xrp".into(),
            quote_code: "524C555344000000000000000000000000000000".into(),
            issuer: "rIssuer".into(),
            limit: 5,
        };
        assert_eq!(xrp.pays_currency(), "xrp");
        assert_eq!(xrp.pays_issuer(), None);

        // Blank quote_code on an issued quote falls back to the fallback code.
        let blank = BookPair {
            quote_code: "  ".into(),
            quote: "RLUSD".into(),
            ..xrp
        };
        assert_eq!(blank.pays_currency(), FALLBACK_CURRENCY_CODE);
    }

    /// TC-085/086: 4+ char non-standard codes decode to text; standard 160-bit codes show their
    /// 3 letters; a non-standard code that decodes to ≤3 chars (incl. `XRP`) must not read like
    /// the real 3-letter code; plain strings pass through.
    #[test]
    fn asset_display_name_mapping() {
        assert_eq!(
            asset_display_name("524C555344000000000000000000000000000000"),
            "RLUSD"
        );
        assert_eq!(
            asset_display_name("5553444300000000000000000000000000000000"),
            "USDC"
        );
        assert_eq!(
            asset_display_name("0000000000000000000000005553440000000000"),
            "USD"
        );
        assert_eq!(
            asset_display_name("0000000000000000000000000000000000000000"),
            "XRP"
        );
        for impostor in [
            "5553440000000000000000000000000000000000",
            "5852500000000000000000000000000000000000",
            "0000000000000000000000005852500000000000",
        ] {
            let shown = asset_display_name(impostor);
            assert!(shown.ends_with('…'), "{impostor} -> {shown}");
            assert_ne!(shown, "XRP");
            assert_ne!(shown, "USD");
        }
        assert_eq!(asset_display_name("BTC"), "BTC");
        assert_eq!(asset_display_name("XRP"), "XRP");
        assert_eq!(asset_display_name("UNKNOWN"), "UNKNOWN");
    }
}

#[cfg(test)]
mod generated_wallet_keys_security_tests {
    use super::*;

    #[test]
    fn generated_wallet_keys_debug_redacts_seed() {
        let r = GeneratedWalletKeys {
            master_seed: "sEdSecretShouldNotAppear".into(),
            master_seed_hex: "DEADBEEF".into(),
            account_id: "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh".into(),
            public_key: "aPub".into(),
            key_type: "ed25519".into(),
        };
        let dbg = format!("{r:?}");
        assert!(dbg.contains("[REDACTED]"));
        assert!(!dbg.contains("sEdSecretShouldNotAppear"));
        assert!(!dbg.contains("DEADBEEF"));
    }
}
