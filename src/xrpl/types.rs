use std::time::Duration;

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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OfferRow {
    pub quality: String,
    pub price: String,
    pub taker_gets: String,
    pub taker_pays: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TxSummary {
    pub hash: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustLineRow {
    pub currency: String,
    pub account: String,
    pub balance: String,
    pub limit: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TxRow {
    pub hash: String,
    pub tx_type: String,
    pub ledger_index: u32,
    pub result: String,
}

/// One row from `account_objects` (Check, Ticket, MPToken, PayChannel, Escrow, …).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerObjectRow {
    pub ledger_type: String,
    pub index: String,
    pub detail: String,
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

/// XRP Payment from the Wallet modal (classic or X-destination).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentSubmitParams {
    pub destination: String,
    pub amount_xrp: String,
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
    /// `account_objects` (limit 200); UI filters by ledger type per tab.
    LedgerObjects,
    /// Sign and submit AccountSet (wallet form).
    AccountSetSubmit(AccountSetSubmitParams),
    /// Sign and submit Payment (wallet modal).
    PaymentSubmit(PaymentSubmitParams),
}

#[derive(Debug)]
pub struct PollContext {
    pub rpc_url: String,
    pub watch_address: String,
    pub book_pair: BookPair,
    pub poll_interval: Duration,
    pub seed_address: Option<String>,
    pub network_watch: watch::Receiver<Network>,
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
}
