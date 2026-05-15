# Architecture Reconstruction

> **Read**: `REPO_INVENTORY.md`. **Scope**: full repository. **Confidence**: high. **Generated**: 2026-05-14 (Pass 2).

## Architecture Overview

**Dominant style**: Elm-like TEA (The Elm Architecture) with unidirectional message flow. Components are pure state machines that receive `Action` messages and produce render output. No framework — hand-rolled using `ratatui` + `tokio::mpsc` channels.

The codebase follows a **layered + vertical-slice hybrid**:
- **`xrpl/`** — data-fetching layer (RPC, WebSocket, polling, CLI exec)
- **`components/`** — presentation layer (panels, tabs, shared widgets)
- **`app.rs`** — orchestration layer (event loop, action routing, lifecycle)
- **`config.rs`** / **`network.rs`** / **`signing.rs`** — infrastructure layer

No formal hexagonal/clean architecture boundaries. Domain logic is interwoven with network I/O in `xrpl/poll.rs` and display logic in `components/`.

## Component Map

```
main.rs
  ├─► app::App::run()                       [orchestration]
  │     ├─► Tui (event loop)                [terminal I/O]
  │     ├─► Component panels (×4 tabs)      [UI rendering]
  │     │     ├─ OverviewTab (ServerPanel | OraclePanel + FlareFtsoPanel, left | right stack)
  │     │     │     ├─ ServerPanel
  │     │     │     └─ OraclePanel
  │     │     ├─ AccountWalletTab
  │     │     │     ├─ WalletPanel
  │     │     │     ├─ AccountPanel
  │     │     │     └─ TxHistoryPanel
  │     │     ├─ MarketOracleTab
  │     │     │     ├─ BookPanel
  │     │     │     ├─ PathFindPanel
  │     │     │     ├─ AmmPanel
  │     │     │     ├─ TrustLinesPanel
  │     │     │     ├─ FlareFtsoPanel
  │     │     │     └─ OraclePanel
  │     │     └─ AssetsTab
  │     │           ├─ NftTab
  │     │           └─ LedgerObjectsPanel (×3 filtered views)
  │     ├─ start_ws_task()                  [WebSocket]
  │     └─ start_poll_task()                [RPC polling]
  │
  └─► execute_cli_command()                 [CLI mode]
```

### Key Channels (mpsc)

| Channel | Type | Producer(s) | Consumer(s) |
|---------|------|-------------|-------------|
| `action_tx/rx` | `UnboundedSender<Action>` | Tui events, WS task, poll task, components | `App::process_actions()` |
| `poll_tx/rx` | `UnboundedSender<PollCommand>` | `App` (on user action), WS task (on ledger close) | `run_poll_loop()` |
| `poll_trigger_tx/rx` | `UnboundedSender<()>` | WS task (on ledger close) | `run_poll_loop()` (triggers immediate poll) |
| `net_tx` | `watch::Sender<Network>` | `App::process_actions()` (`Action::NetworkChange`) | PollContext (cloned for poll task) |
| `cancel` | `CancellationToken` | `App::run()` (on quit) | WS task, poll task |

## Dependency Direction

```
main ──► app ──► components ──► (none, leaf nodes)
  │        │
  │        ├──► xrpl (poll, ws, client, types)
  │        ├──► config
  │        ├──► network
  │        └──► signing
  │
  ├──► cli ──► xrpl (cli_exec)
  ├──► config
  ├──► network
  └──► signing
```

- **`action.rs`** is a dependency hub: referenced by `app`, `config`, `xrpl/*`, `components/*`.
- **`config.rs`** depends on `action` (for `Mode`, `Action` types in keybindings) and `network`.
- **`xrpl/types.rs`** depends on nothing internal — pure data types.
- **`components/`** depends on `action`, `config`, `xrpl/types` — never on `app` or `xrpl/client`.
- **No circular dependencies** observed at the module level.

## Main Execution Flows

### Flow 1: TUI Startup → First Render
1. `main()` parses CLI args, resolves network/URLs/seed
2. `App::new()` creates 4 tab components, splash, status bar
3. `App::run()` enters raw mode, registers all components
4. Spawns `start_ws_task()` and `start_poll_task()`
5. Event loop starts; `Action::XrplServerInfo` triggers `startup_done = true` → splash dismissed

### Flow 2: Periodic Data Refresh
1. `poll_batch()` runs every `poll_interval_ms` (min 10s guarded)
2. Fires `tokio::join!` on: `server_info`, `fee`, `account_info`, `book_offers` (parallel), then `account_nfts`, `account_lines`, `account_tx` (sequential after 500ms sleep)
3. Oracle refresh path: XRPL `get_aggregate_price` + Flare FTSOv2 (`crate::flare::fetch_ftso_prices`) are polled and normalized
4. Results dispatched as `Action::XrplServerInfo`, `Action::XrplDunl` (XRPL Foundation UNL from `https://unl.xrplf.org`), `Action::XrplFee`, `Action::XrplBookOffers`, `Action::XrplPathFind`, `Action::XrplOraclePrices`, `Action::FlareOraclePrices`, etc.
5. `App::process_actions()` routes to active components via `update()`

### Flow 3: WebSocket Ledger Close → Poll Trigger
1. WS receives `ledgerClosed` event
2. Sends `Action::XrplLedgerClose` + `()` to `poll_trigger_tx`
3. Poll task checks trigger channel, fires immediate poll on any pending trigger

### Flow 4: Transaction Submit (e.g. Payment)
1. User fills form in WalletPanel → `Action::PaymentSubmit(params)`
2. `App` forwards to poll task via `PollCommand::PaymentSubmit`
3. Poll task: validate params → `simulate_tx` → check `engine_result` → extract `Sequence`/`Fee` → `sign` → `submit`
4. Result dispatched as `Action::PaymentSubmitOk` or `Action::PaymentSubmitErr`

### Flow 5: Domain Verification (xrp-ledger.toml)
1. User selects a validator in `ServerPanel` → `Action::RequestXrplToml { domain, expected_pubkey }`
2. `App::process_actions()` spawns an async `fetch_xrpl_toml` call (10s timeout)
3. Result dispatched as `Action::XrplTomlFetched { domain, result }` back to `ServerPanel`

### Flow 6: CLI Command
1. `main()` matches `Cmd::*` → calls `execute_cli_command()`
2. Synchronous-style async: creates `RpcClient`, calls relevant RPC, formats stdout
3. No TUI, no channels — direct function calls

## Side-Effect Boundaries

| Boundary | Location | Mechanism |
|----------|----------|-----------|
| **Terminal I/O** | `tui.rs` | `crossterm` raw mode, alternate screen, `EventStream` |
| **Network (RPC)** | `xrpl/client.rs` | `reqwest` HTTP POST to JSON-RPC endpoints |
| **Network (Flare FTSOv2)** | `flare.rs` | `alloy` provider + ContractRegistry/FtsoV2 read calls |
| **Network (WS)** | `xrpl/ws.rs` | `xrpl-rust` WebSocket client |
| **File system (config)** | `config.rs` | `config` crate reading `config.toml` |
| **File system (logging)** | `logging.rs` | `tracing` file appender to data dir |
| **File system (uninstall)** | `uninstall.rs` | `std::fs::remove_dir_all` |
| **Environment** | `config.rs`, `main.rs`, `app.rs` | `std::env::var` for `XRPL_*`, `FLARE_*`, `LAZYXRP_*` |
| **Process signal** | `tui.rs` | `signal_hook` SIGTSTP for suspend |
| **Panic handler** | `errors.rs` | `human-panic`, `better-panic` |

## Observed Inconsistencies

- **Observed** (medium confidence): Config merge order in `Config::new()` is complex (~1000 lines). The exact precedence (built-in → file → env → CLI) is spread across `config.rs` and `main.rs`. This could lead to confusion about what value is "final."
- **Observed** (medium confidence): `poll.rs` contains both data-fetching logic AND transaction-signing/submit logic. The submit flows (~800 lines) are interleaved with polling — could be separated into `submit.rs`.
- **Observed** (low confidence): The `Component` trait has optional methods with default no-op implementations. Some components override `handle_events`, others strictly use `update`. Not all panels use the same pattern.

## Confidence Notes

- Architecture style and module boundaries: **high confidence** — verified by reading all source files and existing docs.
- Execution flows: **high confidence** — traced through source code.
- Inconsistencies: **medium confidence** — surface-level observations; deeper coupling analysis would require Pass 4.
