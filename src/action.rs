use serde::{Deserialize, Serialize};
use strum::Display;

use crate::{
    network::Network,
    xrpl::{
        AccountSetSubmitParams, AccountSummary, AggregatePrice, AmmSummary, DunlSummary,
        EscrowCreateSubmitParams, FeeSummary, FlareFeedPrice, FxrpDirectMintInfo, LedgerObjectRow, NftRow,
        OfferCreateSubmitParams, OfferRow, PathFindSnapshot, PaymentSubmitParams,
        ServerInfoSummary, SetRegularKeySubmitParams, TrustLineRow, TrustSetSubmitParams, TxRow,
        TxSummary, WalletProposeResult, XrplRlusdPrice, XrplTomlData,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Display, Serialize, Deserialize)]
pub enum Action {
    Tick,
    Render,
    Resize(u16, u16),
    Suspend,
    Resume,
    Quit,
    ClearScreen,
    Error(String),
    Help,
    XrplServerInfo(Box<ServerInfoSummary>),
    /// XRPL Foundation dUNL manifest (`https://unl.xrplf.org`).
    XrplDunl(DunlSummary),
    XrplFee(FeeSummary),
    XrplAccount(Box<AccountSummary>),
    XrplBookOffers(Vec<OfferRow>),
    /// `ripple_path_find` swap-route preview (self-payment, configured quote amount).
    XrplPathFind(PathFindSnapshot),
    XrplLedgerClose {
        ledger_index: u32,
        base_fee: u32,
        reserve_base: u32,
        reserve_inc: u32,
    },
    XrplAccountTx(Box<TxSummary>),
    XrplAccountNfts(Vec<NftRow>),
    XrplTrustLines(Vec<TrustLineRow>),
    XrplAmmInfo(Box<AmmSummary>),
    XrplTxHistory(Vec<TxRow>, Option<serde_json::Value>),
    /// Append page to existing tx history (pagination).
    XrplTxHistoryAppend(Vec<TxRow>, Option<serde_json::Value>),
    /// Wallet tab account summary (tx history uses `XrplTxHistory` from the same poll cycle).
    XrplWalletOverview(Option<AccountSummary>),
    /// Wallet tab shown but no seed configured — show hint instead of loading spinner.
    XrplWalletNotConfigured,
    XrplRlusdPrice(XrplRlusdPrice),
    /// Aggregate price from `get_aggregate_price`.
    XrplOraclePrices(Vec<AggregatePrice>),
    /// FTSOv2 prices from Flare (alloy).
    FlareOraclePrices(Vec<FlareFeedPrice>),
    /// FXRP Direct Mint Core Vault + fees from AssetManager (read-only).
    FxrpDirectMintInfo(Box<FxrpDirectMintInfo>),
    /// Oracle tab shown but no oracles configured.
    XrplOracleNotConfigured,
    /// `account_objects` snapshot; each tab filters rows by `LedgerEntryType`.
    XrplLedgerObjects(Vec<LedgerObjectRow>),
    XrplError(String),
    /// Request xrp-ledger.toml fetch for domain verification.
    RequestXrplToml {
        domain: String,
        expected_pubkey: String,
    },
    /// Result of xrp-ledger.toml fetch.
    XrplTomlFetched {
        domain: String,
        /// HTTP status code.
        status: u16,
        /// Content-Type header value, if present.
        content_type: Option<String>,
        /// Raw HTTP body (may be HTML on 404, TOML on 200).
        raw: Option<String>,
        result: Result<XrplTomlData, String>,
    },
    RefreshAccount,
    RefreshBook,
    RefreshNfts,
    RefreshLines,
    RefreshTxHistory,
    /// Load next page of tx history (uses current marker).
    RefreshTxHistoryMore(Option<serde_json::Value>),
    /// Re-fetch `account_objects` (shared by Objects / PayChan+Escrow tabs).
    RefreshLedgerObjects,
    TabNext,
    TabPrev,
    FocusNext,
    FocusPrev,
    SelectNext,
    SelectPrev,
    NetworkChange(Network),
    /// Number keys `1`–`6`: switch to tab index (0-based target).
    TabJump(usize),
    /// While `true`, global Splash keybindings (e.g. `h`/`l` focus) are ignored so inline typing works.
    SetKeymapSuppression(bool),
    /// Queue AccountSet sign+submit from Wallet form (poll task).
    AccountSetSubmit(AccountSetSubmitParams),
    /// Result of submitting an AccountSet from the Wallet form (poll task).
    AccountSetSubmitOk(String),
    AccountSetSubmitErr(String),
    /// Queue Payment sign+submit from Wallet modal (poll task).
    PaymentSubmit(PaymentSubmitParams),
    PaymentSubmitOk(String),
    PaymentSubmitErr(String),
    /// Queue SetRegularKey sign+submit from Wallet form (poll task).
    SetRegularKeySubmit(SetRegularKeySubmitParams),
    SetRegularKeySubmitOk(String),
    SetRegularKeySubmitErr(String),
    /// Queue EscrowCreate sign+submit from Wallet form (poll task).
    EscrowCreateSubmit(EscrowCreateSubmitParams),
    EscrowCreateSubmitOk(String),
    EscrowCreateSubmitErr(String),
    /// Queue OfferCreate sign+submit from Wallet form (poll task).
    OfferCreateSubmit(OfferCreateSubmitParams),
    OfferCreateSubmitOk(String),
    OfferCreateSubmitErr(String),
    /// Queue TrustSet sign+submit from Wallet form (poll task).
    TrustSetSubmit(TrustSetSubmitParams),
    TrustSetSubmitOk(String),
    TrustSetSubmitErr(String),
    /// Request local key generation (Wallet tab, `g`).
    WalletPropose,
    WalletProposeOk(WalletProposeResult),
    WalletProposeErr(String),
    /// Toggle transaction detail overlay in TxHistory panel.
    TxDetailToggle,
}
