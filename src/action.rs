use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    network::Network,
    xrpl::{
        AccountDeleteSubmitParams, AccountOfferRow, AccountSetSubmitParams, AccountSummary,
        AggregatePrice, AmmDepositSubmitParams, AmmSummary, AmmWithdrawSubmitParams, BookMidPrice,
        DunlSummary, EscrowCloseSubmitParams, EscrowCreateSubmitParams, EscrowRow, FeeSummary,
        FlareFeedPrice, FlareWalletSummary, FxrpDirectMintInfo, FxrpDirectMintPaymentParams,
        FxrpExecuteDirectMintParams, GeneratedWalletKeys, GenericTxSubmitParams, LedgerObjectRow,
        NftRow, OfferCancelSubmitParams, OfferCreateSubmitParams, OfferRow, PathFindSnapshot,
        PaymentQuoteParams, PaymentSubmitParams, ServerInfoSummary, SetRegularKeySubmitParams,
        SmartAccountInfo, SmartAccountInstructionParams, TrustLineRow, TrustSetSubmitParams, TxRow,
        TxSummary, XrplTomlData,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Tick,
    Resize(u16, u16),
    Suspend,
    Resume,
    Quit,
    ClearScreen,
    /// Request a redraw (e.g. an off-thread image resize finished; `Tick` alone does not mark dirty).
    Render,
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
    /// `None` = no AMM pool for the configured pair.
    XrplAmmInfo(Option<Box<AmmSummary>>),
    XrplTxHistory(Vec<TxRow>, Option<serde_json::Value>),
    /// Append page to existing tx history (pagination).
    XrplTxHistoryAppend(Vec<TxRow>, Option<serde_json::Value>),
    /// Wallet tab account summary (tx history uses `XrplTxHistory` from the same poll cycle).
    XrplWalletOverview(Option<AccountSummary>),
    BookMidPrice(BookMidPrice),
    /// Aggregate price from `get_aggregate_price`.
    XrplOraclePrices(Vec<AggregatePrice>),
    /// FTSOv2 prices from Flare (alloy).
    FlareOraclePrices(Vec<FlareFeedPrice>),
    /// FXRP Direct Mint Core Vault + fees from AssetManager (read-only).
    FxrpDirectMintInfo(Box<FxrpDirectMintInfo>),
    /// Flare Smart Account (personal account, nonce, operator, fees, FXRP balance; read-only).
    SmartAccountInfo(Box<SmartAccountInfo>),
    /// Flare EVM wallet native + FXRP balances (read-only).
    FlareWalletBalance(Box<FlareWalletSummary>),
    /// A flare poll attempt failed entirely — arms the status-bar flare health dot.
    FlareFetchFailed,
    /// Oracle tab shown but no oracles configured.
    XrplOracleNotConfigured,
    /// `account_objects` snapshot; Assets panels filter rows by `LedgerEntryType`.
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
    /// Request preview image for selected NFT URI.
    NftImageRequest {
        nft_id: String,
        uri: String,
    },
    /// Bounded image bytes fetched from an NFT URI or metadata image field.
    NftImageLoaded {
        nft_id: String,
        bytes: Arc<Vec<u8>>,
    },
    NftImageError {
        nft_id: String,
        message: String,
    },
    RefreshTxHistory,
    /// Load next page of tx history (uses current marker).
    RefreshTxHistoryMore(Option<serde_json::Value>),
    /// Re-fetch `account_objects` (Assets: misc / pay channels / escrows).
    RefreshLedgerObjects,
    TabNext,
    TabPrev,
    FocusNext,
    FocusPrev,
    SelectNext,
    SelectPrev,
    NetworkChange(Network),
    /// Hotkey: cycle to the next XRPL network (mainnet→testnet→devnet→xahau→xahau-test→…).
    NetworkSwitchCycle,
    /// Number keys `1..=TAB_TITLES.len()`: switch to tab index (0-based target).
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
    /// Ask the poll task for a `ripple_path_find` Send max quote (Payment composer `p`).
    PaymentQuote(PaymentQuoteParams),
    /// Quoted Send max spec (`XRP:drops` / `CUR:issuer:value`, slippage included).
    PaymentQuoteOk(String),
    PaymentQuoteErr(String),
    /// Queue FXRP Direct Mint Payment (Core Vault + 32-byte memo).
    FxrpDirectMintPaymentSubmit(FxrpDirectMintPaymentParams),
    FxrpDirectMintPaymentSubmitOk(String),
    FxrpDirectMintPaymentSubmitErr(String),
    /// Queue Flare `executeDirectMinting` with pasted FDC proof JSON (C3).
    FxrpExecuteDirectMintSubmit(FxrpExecuteDirectMintParams),
    FxrpExecuteDirectMintSubmitOk(String),
    FxrpExecuteDirectMintSubmitErr(String),
    /// Queue a Smart Account FXRP transfer / redeem instruction Payment to the operator.
    SmartAccountInstructionSubmit(SmartAccountInstructionParams),
    SmartAccountInstructionSubmitOk(String),
    SmartAccountInstructionSubmitErr(String),
    /// Queue SetRegularKey sign+submit from Wallet form (poll task).
    SetRegularKeySubmit(SetRegularKeySubmitParams),
    SetRegularKeySubmitOk(String),
    SetRegularKeySubmitErr(String),
    /// Queue EscrowCreate sign+submit from Wallet form (poll task).
    EscrowCreateSubmit(EscrowCreateSubmitParams),
    EscrowCreateSubmitOk(String),
    EscrowCreateSubmitErr(String),
    /// Fetch escrows the signing account owns or receives (Wallet escrow composer).
    RefreshAccountEscrows,
    XrplAccountEscrows(Vec<EscrowRow>),
    /// Queue EscrowFinish / EscrowCancel sign+submit from Wallet composer (poll task).
    EscrowCloseSubmit(EscrowCloseSubmitParams),
    EscrowCloseSubmitOk(String),
    EscrowCloseSubmitErr(String),
    /// Queue AMMDeposit / AMMWithdraw sign+submit from Wallet composer (poll task).
    AmmDepositSubmit(AmmDepositSubmitParams),
    AmmWithdrawSubmit(AmmWithdrawSubmitParams),
    AmmSubmitOk(String),
    AmmSubmitErr(String),
    /// Queue AccountDelete sign+submit from Wallet composer (poll task; after typing DELETE).
    AccountDeleteSubmit(AccountDeleteSubmitParams),
    AccountDeleteSubmitOk(String),
    AccountDeleteSubmitErr(String),
    /// Queue any spec'd transaction (`xrpl::txspec`) sign+submit from the Wallet composer.
    GenericTxSubmit(GenericTxSubmitParams),
    GenericTxSubmitOk(String),
    GenericTxSubmitErr(String),
    /// Assets tab `x`: start an NFTokenBurn for the selected NFT.
    NftBurnStart,
    /// Open the Wallet "Transaction" composer for `tx_type`, prefilled with `fields`.
    TxComposerOpen {
        tx_type: String,
        fields: Vec<(String, String)>,
    },
    /// Queue OfferCreate sign+submit from Wallet form (poll task).
    OfferCreateSubmit(OfferCreateSubmitParams),
    OfferCreateSubmitOk(String),
    OfferCreateSubmitErr(String),
    /// Fetch the signing account's open offers (Wallet OfferCancel composer).
    RefreshAccountOffers,
    XrplAccountOffers(Vec<AccountOfferRow>),
    /// Queue OfferCancel sign+submit from Wallet composer (poll task).
    OfferCancelSubmit(OfferCancelSubmitParams),
    OfferCancelSubmitOk(String),
    OfferCancelSubmitErr(String),
    /// Queue TrustSet sign+submit from Wallet form (poll task).
    TrustSetSubmit(TrustSetSubmitParams),
    TrustSetSubmitOk(String),
    TrustSetSubmitErr(String),
    /// Request local key generation (Wallet tab, `g`).
    GenerateWalletKeys,
    GenerateWalletKeysOk(GeneratedWalletKeys),
    GenerateWalletKeysErr(String),
    /// Toggle transaction detail overlay in TxHistory panel.
    TxDetailToggle,
}
