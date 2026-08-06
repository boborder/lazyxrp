# Data Model

> **Read**: `REPO_INVENTORY.md`, `ARCHITECTURE.md`. **Scope**: full repository. **Confidence**: high. **Generated**: 2026-05-14 (Pass 3).

## Core Data Types

### Configuration (`src/config.rs`)

```
Config
├── config: PathConfig { config_dir, data_dir }
├── keybindings: KeyBindings { 0: HashMap<Mode, HashMap<Vec<KeyEvent>, Action>> }
├── styles: Styles
└── xrpl: LedgerConfig
    ├── account: String               // watch account address
    ├── issuer: String                // IOU issuer (fallback: rvYAfWj5gh67oV6fW32ZzP3Aw4Eubs59B)
    ├── currency: String              // quote currency label
    ├── currency_code: String         // hex currency code (fallback: "USD")
    ├── offer_limit: u16             // max book offers (default: 5)
    ├── poll_interval_ms: u64        // polling interval (default: 5000)
    ├── network: Network
    ├── rpc_server: Option<String>    // override from config
    ├── ws_server: Option<String>     // override from config
    ├── signing: RawSigningConfig
    ├── oracles: Vec<OracleId>       // oracle identifiers for get_aggregate_price
    ├── oracle_base_asset: String    // base asset (default: "XRP")
    └── oracle_quote_asset: String   // quote asset (default: "USD")
    //   Note: DIA Oracles register QuoteAsset as plain "USD", not hex.
        ├── seed: Option<String>      // cleared after Config::new()
        └── secret_seed: Option<SecretString>  // memory-masked
```

**FXRP C3 (ticket 13):** optional sibling `flare: FlareConfig { fassets: FlareFassetsConfig { execute: false, evm_key_env: "FLARE_EVM_KEY" } }` — default install never Flare-writes.

### Network (`src/network.rs`)

```
Network ::= Mainnet | Testnet | Devnet
  → rpc_url: "https://xrplcluster.com" | testnet | devnet endpoints
  → ws_url: corresponding WSS endpoint
  → is_mainnet(): bool (mainnet guard for write operations)
```

### Messages (`src/action.rs`)

`Action` is the universal message type flowing through `action_tx/rx`:
- **Lifecycle**: `Tick`, `Render`, `Resize`, `Quit`, `Suspend`, `Resume`, `ClearScreen`
- **Navigation**: `TabNext`, `TabPrev`, `TabJump`, `FocusNext`, `FocusPrev`
- **XRPL data**: `XrplServerInfo`, `XrplFee`, `XrplAccount`, `XrplBookOffers`, `XrplAccountNfts`, `XrplTrustLines`, `XrplAmmInfo`, `XrplTxHistory`, `XrplLedgerClose`, `XrplLedgerObjects`, `XrplWalletOverview`, `XrplRlusdPrice`, `NftImageLoaded`, `NftImageError`, `NftImageReady`
- **User triggers**: `RefreshAccount`, `RefreshBook`, `RefreshNfts`, `RefreshLines`, `RefreshTxHistory`, `RefreshTxHistoryMore`, `RefreshLedgerObjects`
- **Submit flows**: `AccountSetSubmit`, `PaymentSubmit`, `SetRegularKeySubmit`, `EscrowCreateSubmit`, `OfferCreateSubmit` (+ Ok/Err variants)
- **Wallet**: `WalletPropose`, `WalletProposeOk`, `WalletProposeErr`, `XrplWalletNotConfigured`
- **Display**: `Help`, `TxDetailToggle`, `SetKeymapSuppression`, `NetworkChange`

### Poll Commands (`src/xrpl/types.rs`)

`PollCommand` — commands sent from `App` to poll task:
```
PollCommand ::=
  | Tick                           // periodic poll
  | Account | Book | Nfts | Lines   // on-demand refresh
  | TxHistory | TxHistoryMore(marker)  // tx history with pagination
  | LedgerObjects                  // account_objects snapshot
  | WalletOverview
  | AccountSetSubmit(params)       // sign+submit AccountSet
  | PaymentSubmit(params)          // sign+submit Payment
  | SetRegularKeySubmit(params)    // sign+submit SetRegularKey
  | EscrowCreateSubmit(params)     // sign+submit EscrowCreate
  | OfferCreateSubmit(params)      // sign+submit OfferCreate
  | WalletPropose(key_type)        // generate key
```

### XRPL Data Types (row structs in `src/xrpl/types.rs`)

| Type | Fields | Source RPC |
|------|--------|------------|
| `ServerInfoSummary` | `ledger_index`, `hostid`, `validation_quorum?`, `validator_list?` | `server_info` |
| `DunlValidatorRow` | `validation_public_key`, `has_manifest`, optional `domain` / `sequence` / `master_public_key` | dUNL blob per validator |
| `DunlSummary` | `validator_count`, `sequence`, `expiration_ripple`, `expiration_utc`, `validators[]`; `stats()`, `days_until_expiry()` | HTTPS `https://unl.xrplf.org` |
| `FeeSummary` | `open_ledger_fee_drops` | `fee` |
| `AccountSummary` | `account`, `balance_xrp`, `sequence`, `owner_count`, `flags`, `regular_key?`, `domain_hex?` | `account_info` |
| `TxSummary` | `hash` | WebSocket `tx` event |
| `TxRow` | `hash`, `tx_type`, `ledger_index`, `result`, `direction`, `tx_json`, `meta_json` | `account_tx` |
| `AccountTxPage` | `rows: Vec<TxRow>`, `marker?` | `account_tx` (paginated) |
| `OfferRow` | `quality`, `price`, `taker_gets`, `taker_pays`, `raw_json` | `book_offers` |
| `NftRow` | `nft_id`, `taxon`, `serial`, `transfer_fee`, `uri`, `is_mutable`, `raw_json` | `account_nfts` |
| `TrustLineRow` | `currency`, `account`, `balance`, `limit`, `raw_json` | `account_lines` |
| `AmmSummary` | `asset1`, `asset2`, `lp_token`, `trading_fee`, `pool1`, `pool2` | `amm_info` |
| `LedgerObjectRow` | `ledger_type`, `index`, `detail`, `raw_json` | `account_objects` |
| `SimulateResult` | `tx_json`, `engine_result`, `engine_result_message`, `ledger_index`, `meta?` | `simulate` |
| `PathAlternative` | `paths_computed`, `source_amount` | `ripple_path_find` |
| `RipplePathFindResult` | `alternatives`, `destination_account`, `destination_amount`, `source_account` | `ripple_path_find` |
| `WalletProposeResult` | `master_seed`, `master_seed_hex`, `account_id`, `public_key`, `public_key_hex`, `key_type` | `wallet_propose` |
| `XrplRlusdPrice` | `bid`, `ask`, `mid` | `book_offers` (RLUSD) |

### ArcValue (`src/xrpl/types.rs`)

`ArcValue(Arc<serde_json::Value>)` — reference-counted newtype for sharing raw JSON across components without deep clones on `Action` routing.

## Data Lifecycle

1. **Startup**: `Config::new()` loads from built-in defaults → user `config.toml` → env vars. Seed is extracted from file and cleared; `SecretString` takes over.
2. **Watch mode**: `Action` messages flow from WS/poll tasks → `action_rx` → `App::drain_and_dispatch_actions()` → component `update()` calls → internal state mutation → `draw()`.
3. **CLI mode**: Direct async RPC calls → formatted stdout output → exit. No message channels.
4. **Submit**: User form → `Action::*Submit(params)` → `PollCommand::*Submit` → poll task validates → `simulate_tx` → `sign` → `submit` → `Action::*SubmitOk/Err` → component display.
5. **Shutdown**: `CancellationToken` cancels WS/poll tasks. `Tui::exit()` restores terminal.

## Persisted / Serialized Formats

| Artifact | Format | Location | Reader |
|----------|--------|----------|--------|
| Built-in defaults | JSON5 | `config.json5` (embedded via `include_str!`) | `config::Config` |
| User config | TOML | `$XDG_CONFIG_HOME/lazyxrp/config.toml` or `~/.config/lazyxrp/config.toml` | `config::Config` |
| Logs | Tracing formatted | `$XDG_DATA_HOME/lazyxrp/` or `~/.local/share/lazyxrp/` | `logging::init()` |
| Panic reports | `human-panic` | `$XDG_DATA_HOME/lazyxrp/` | `errors::init()` |
| Signing seed (env) | Plain text | `XRPL_SEED` env var | `SigningConfig` |
| Signing seed (memory) | Zero-on-drop | `secrecy::SecretString` | `signing` module |
