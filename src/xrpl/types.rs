use std::sync::Arc;
use std::time::Duration;

/// Newtype wrapper so `Arc<serde_json::Value>` can derive `Serialize`/`Deserialize`.
///
/// # Immutability contract
///
/// Instances are **shared across components** (e.g. `TxRow.tx_json` is passed to both
/// TxHistory panel and TxDetail overlay). **NEVER mutate** shared `ArcValue` via
/// `Arc::make_mut` — clone the inner value first if modification is needed.
/// See I-10 in `docs/agent/INVARIANTS.md`.
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
        const RIPPLE_EPOCH_UNIX: i64 = 946_684_800;
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

/// Key generation result (TUI: local via `signing::propose_wallet_local`; RPC parser in `client`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletProposeResult {
    /// Master seed (Ed25519 family seed, starts with `s`).
    pub master_seed: String,
    /// Hex-encoded master seed.
    pub master_seed_hex: String,
    /// The derived XRPL address (starts with `r`).
    pub account_id: String,
    /// Public key (starts with `a` for Ed25519).
    pub public_key: String,
    /// Hex-encoded public key.
    pub public_key_hex: String,
    /// Key algorithm: `"ed25519"` or `"secp256k1"`.
    pub key_type: String,
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
pub struct XrplRlusdPrice {
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

/// Known XRPL hex currency codes → human-readable display names.
pub static CURRENCY_HEX_MAP: &[(&str, &str)] = &[
    ("524C555344000000000000000000000000000000", "RLUSD"),
    ("5553444300000000000000000000000000000000", "USDC"),
    ("5553445400000000000000000000000000000000", "USDT"),
    ("4254430000000000000000000000000000000000", "BTC"),
    ("4554480000000000000000000000000000000000", "ETH"),
    ("5852500000000000000000000000000000000000", "XRP"),
    ("4555520000000000000000000000000000000000", "EUR"),
    ("584C4D0000000000000000000000000000000000", "XLM"),
    ("534F4C0000000000000000000000000000000000", "SOL"),
    ("4144410000000000000000000000000000000000", "ADA"),
    ("444F474500000000000000000000000000000000", "DOGE"),
    ("4C54430000000000000000000000000000000000", "LTC"),
    ("504F4C0000000000000000000000000000000000", "DOT"),
];

/// Decode a standard XRPL 160-bit currency (3 ASCII letters + zero padding).
fn currency_from_standard_hex(hex: &str) -> Option<String> {
    if hex.len() != 40 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let suffix = &hex[6..];
    if !suffix.chars().all(|c| c == '0') {
        return None;
    }
    let mut letters = [0u8; 3];
    for (i, byte) in letters.iter_mut().enumerate() {
        let pair = &hex[i * 2..i * 2 + 2];
        *byte = u8::from_str_radix(pair, 16).ok()?;
        if !byte.is_ascii_graphic() {
            return None;
        }
    }
    std::str::from_utf8(&letters).ok().map(str::to_string)
}

/// Convert an XRPL currency code (hex or 3-letter string) to a display name.
#[must_use]
pub fn asset_display_name(asset: &str) -> String {
    let upper = asset.to_ascii_uppercase();
    for (hex, display) in CURRENCY_HEX_MAP {
        if upper == *hex {
            return (*display).to_string();
        }
    }
    if let Some(code) = currency_from_standard_hex(&upper) {
        return code;
    }
    asset.to_string()
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
pub fn is_objects_tab_ledger_type(t: &str) -> bool {
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

#[must_use]
pub fn is_pay_channel_type(t: &str) -> bool {
    t == "PayChannel"
}

#[must_use]
pub fn is_escrow_type(t: &str) -> bool {
    t == "Escrow"
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
    pub config_seed: Option<String>,
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
    pub skip_mainnet_prompt: bool,
    pub config_seed: Option<String>,
}

/// FXRP Direct Mint Payment: XRP to Core Vault with 32-byte recipient memo (C2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FxrpDirectMintPaymentParams {
    /// Core Vault XRPL classic address (from C1 / AssetManager).
    pub core_vault_xrpl: String,
    /// Flare/EVM recipient address (`0x` + 40 hex).
    pub flare_recipient: String,
    /// XRP amount to send (fees deducted on Flare side from this payment).
    pub amount_xrp: String,
    pub skip_mainnet_prompt: bool,
    pub config_seed: Option<String>,
}

/// FXRP C3: paste FDC Payment proof JSON and call `executeDirectMinting` (flagged).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FxrpExecuteDirectMintParams {
    /// FDC DA proof JSON: `{proof,response}` or `{merkleProof,data}` (hex strings).
    pub proof_json: String,
    pub skip_mainnet_prompt: bool,
}

/// SetRegularKey from the Wallet form.
/// An empty `regular_key` means clear (remove) the existing regular key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetRegularKeySubmitParams {
    pub regular_key: String,
    pub skip_mainnet_prompt: bool,
    pub config_seed: Option<String>,
}

/// EscrowCreate from the Wallet form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscrowCreateSubmitParams {
    pub destination: String,
    pub amount_xrp: String,
    /// Seconds since Ripple Epoch; if 0, defaults to 30 days from now.
    pub finish_after: String,
    pub skip_mainnet_prompt: bool,
    pub config_seed: Option<String>,
}

/// OfferCreate from the Wallet form.
/// taker_gets / taker_pays use compact spec: `"XRP:100"` (XRP) or `"USD:rIssuer:10"` (IOU).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferCreateSubmitParams {
    pub taker_gets: String,
    pub taker_pays: String,
    pub skip_mainnet_prompt: bool,
    pub config_seed: Option<String>,
}

/// TrustSet from the Wallet form (v1: Limit + Currency + Issuer only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustSetSubmitParams {
    pub currency: String,
    pub issuer: String,
    pub limit: String,
    pub skip_mainnet_prompt: bool,
    pub config_seed: Option<String>,
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
    /// `account_objects` (limit 200); UI filters by ledger type per tab.
    LedgerObjects,
    /// Sign and submit AccountSet (wallet form).
    AccountSetSubmit(AccountSetSubmitParams),
    /// Sign and submit Payment (wallet modal).
    PaymentSubmit(PaymentSubmitParams),
    FxrpDirectMintPayment(FxrpDirectMintPaymentParams),
    /// Flare `executeDirectMinting` with pasted FDC proof (C3; gated by config).
    FxrpExecuteDirectMint(FxrpExecuteDirectMintParams),
    /// Sign and submit SetRegularKey (wallet form).
    SetRegularKeySubmit(SetRegularKeySubmitParams),
    /// Sign and submit EscrowCreate (wallet form).
    EscrowCreateSubmit(EscrowCreateSubmitParams),
    OfferCreateSubmit(OfferCreateSubmitParams),
    /// Sign and submit TrustSet (wallet form).
    TrustSetSubmit(TrustSetSubmitParams),
    /// Generate a new key pair via wallet_propose ("ed25519" or "secp256k1").
    WalletPropose(String),
}

#[derive(Debug)]
pub struct PollContext {
    pub rpc_url: String,
    pub watch_address: String,
    pub book_pair: BookPair,
    pub poll_interval: Duration,
    pub seed_address: Option<String>,
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
    /// `[flare.fassets] execute` — when false, C3 execute path is refused.
    pub flare_fassets_execute: bool,
    /// Env var name for Flare executor key (`[flare.fassets] evm_key_env`).
    pub flare_evm_key_env: String,
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
    }

    /// TC-085: asset_display_name maps known hex codes to readable names
    #[test]
    fn asset_display_name_maps_hex() {
        assert_eq!(
            asset_display_name("524C555344000000000000000000000000000000"),
            "RLUSD"
        );
        assert_eq!(
            asset_display_name("5553444300000000000000000000000000000000"),
            "USDC"
        );
        assert_eq!(asset_display_name("BTC"), "BTC");
    }

    /// TC-086: asset_display_name passes through unknown values
    #[test]
    fn asset_display_name_unknown_passthrough() {
        assert_eq!(asset_display_name("UNKNOWN"), "UNKNOWN");
    }

    #[test]
    fn asset_display_name_decodes_standard_hex_usd() {
        assert_eq!(
            asset_display_name("5553440000000000000000000000000000000000"),
            "USD"
        );
    }
}
