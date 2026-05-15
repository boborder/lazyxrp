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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerInfoSummary {
    pub ledger_index: u32,
    pub hostid: String,
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
#[allow(dead_code)]
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

/// One path alternative returned by `ripple_path_find`.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathAlternative {
    /// Computed payment paths (array of path arrays).
    pub paths_computed: serde_json::Value,
    /// The amount that would need to be sent from the source account,
    /// given the destination amount and the computed paths.
    pub source_amount: serde_json::Value,
}

/// Result of a `ripple_path_find` RPC call.
#[allow(dead_code)]
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

/// Result from `wallet_propose` RPC.
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

/// Convert an XRPL currency code (hex or 3-letter string) to a display name.
#[must_use]
pub fn asset_display_name(asset: &str) -> String {
    let upper = asset.to_ascii_uppercase();
    for (hex, display) in CURRENCY_HEX_MAP {
        if upper == *hex {
            return (*display).to_string();
        }
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
/// **IOU mode** (iou_currency set): `amount` = IOU value, uses `iou_currency` + `iou_issuer`.
/// Auto-bridging: self-payment with `iou_currency` → swap via DEX order books.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentSubmitParams {
    pub destination: String,
    /// XRP amount (XRP mode) or IOU value (IOU mode).
    pub amount: String,
    /// If set, triggers IOU mode: `"USD"`, `"BTC"` etc.
    pub iou_currency: Option<String>,
    /// Issuer address for IOU mode.
    pub iou_issuer: Option<String>,
    pub skip_mainnet_prompt: bool,
    pub config_seed: Option<String>,
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
    /// Sign and submit SetRegularKey (wallet form).
    SetRegularKeySubmit(SetRegularKeySubmitParams),
    /// Sign and submit EscrowCreate (wallet form).
    EscrowCreateSubmit(EscrowCreateSubmitParams),
    OfferCreateSubmit(OfferCreateSubmitParams),
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
    /// Oracle identifiers for `get_aggregate_price`.
    pub oracles: Vec<OracleId>,
    /// Price pairs to query via `get_aggregate_price`.
    pub oracle_pairs: Vec<OraclePricePair>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
