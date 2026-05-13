use serde::{Deserialize, Serialize};
use strum::Display;

use crate::{
    network::Network,
    xrpl::{
        AccountSetSubmitParams, AccountSummary, AmmSummary, FeeSummary, LedgerObjectRow, NftRow,
        OfferRow, PaymentSubmitParams, ServerInfoSummary, TrustLineRow, TxRow, TxSummary,
        XrplRlusdPrice,
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
    XrplFee(FeeSummary),
    XrplAccount(Box<AccountSummary>),
    XrplBookOffers(Vec<OfferRow>),
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
    XrplWalletOverview(
        Option<AccountSummary>,
        Vec<TxRow>,
        Option<serde_json::Value>,
    ),
    XrplRlusdPrice(XrplRlusdPrice),
    /// `account_objects` snapshot; each tab filters rows by `LedgerEntryType`.
    XrplLedgerObjects(Vec<LedgerObjectRow>),
    XrplError(String),
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
    /// Toggle transaction detail overlay in TxHistory panel.
    TxDetailToggle,
}
