# Repository Inventory

> **Scope**: full repository. **Confidence**: high. **Generated**: 2026-05-14 (Pass 1).

## Project Summary

**lazyxrp** — Rust terminal UI (TUI) for the XRP Ledger. Monitor accounts, order books, NFTs, trust lines, and AMM pools. Supports both an interactive `watch` mode (ratatui dashboard) and CLI subcommands (`info`, `account`, `book`, `nfts`, `lines`, `amm`, `tx-history`, `account-status`, `send`). Single Rust binary.

- **Language**: Rust 2024 edition
- **MSRV**: 1.91
- **License**: MIT
- **Repo**: `github.com/boborder/lazyxrp`
- **Version**: 0.1.14

## Build / Test / Validate Commands

| Command | Purpose |
|---------|---------|
| `cargo check` | Minimum verification after code changes |
| `cargo fmt` | Format code |
| `cargo test` | Run all tests |
| `cargo build --release` | Release build |
| `./install.sh` or `mise run install` | Install binary |
| `mise run bench` | Full benchmark suite (~10 min) |
| `mise run bench-fast` | Quick benchmarks |

## Entry Points

| Entry | File | Description |
|-------|------|-------------|
| `main()` | `src/main.rs:44` | Async runtime start. Routes to `App::run()` (TUI watch) or `execute_cli_command()` (CLI). Resolves network/seed/URL priority chain. |
| `Cli` struct | `src/cli.rs:11` | Clap-derived CLI. Subcommands: `Watch`, `Info`, `Account`, `Book`, `Summary`, `Nfts`, `Lines`, `Amm`, `TxHistory`, `AccountStatus`, `Send`. |
| `App::run()` | `src/app.rs:171` | TUI main loop: event handling → action processing → render. Spawns WS + poll background tasks. |
| `execute_cli_command()` | `src/xrpl/cli_exec.rs` | Non-TUI CLI command dispatcher. |
| `Config::new()` | `src/config.rs` | Config loading: built-in defaults → user config.toml → env vars. |
| `build.rs` | `build.rs` | Build-time metadata (vergen-gix for commit hash, date). |

## Major Directories

```
src/
├── main.rs              Entry point, network/seed URL resolution
├── app.rs               TUI application loop (Elm-like TEA)
├── action.rs            Action enum (all internal messages)
├── cli.rs               CLI argument parsing (clap)
├── config.rs            Config loading/merging, keybinds, styles (~1000 lines)
├── tui.rs               Terminal management (raw mode, event loop, suspend/resume)
├── network.rs           Network enum (mainnet/testnet/devnet) + endpoint URLs
├── signing.rs           Seed handling, signing helpers, mainnet confirmation prompt
├── errors.rs            Error handling init (color-eyre, human-panic, better-panic)
├── logging.rs           Tracing/logging initialization
├── uninstall.rs         --self-uninstall logic
├── xrpl/                XRPL integration
│   ├── mod.rs           Re-exports
│   ├── client.rs        RpcClient (JSON-RPC calls, response parsing, xrp_to_drops)
│   ├── ws.rs            WebSocket subscription task
│   ├── poll.rs          Polling task (periodic + on-demand RPC, submit flows)
│   ├── cli_exec.rs      CLI command execution
│   ├── types.rs         Data types: row structs, BookPair, PollContext/Command
│   ├── json_util.rs     JSON path helpers (json_str, extract_json_u32)
│   └── backoff.rs       Reconnection backoff timing
└── components/          UI components
    ├── mod.rs           Component trait definition
    ├── panels/          Standalone panels (account, book, amm, server, wallet, etc.)
    ├── tabs/            Composite tab views (overview, account_wallet, market_oracle, assets, nft)
    └── shared/          Shared widgets (theme, fps, splash, status_bar, help_overlay, tx_detail, etc.)

docs/
├── architecture/        C4 architecture diagrams
├── design.md            Architecture overview, module responsibilities, data flow
├── tech.md              Tech stack, dependencies, build config
├── test.md              Test policy, TC-ID case list, TDD roadmap
├── tasks.md             Task status and milestones
├── directory.md         Directory structure index (this doc's source)
├── requirements.md      Functional/non-functional requirements
├── security.md          Security design, threat model
├── problems.md          Known issues and workarounds
├── references.md        External references
└── benchmark.md         Benchmark suite usage
```

## External Dependencies / Boundaries

- **XRPL Ledger**: JSON-RPC (HTTPS) + WebSocket (WSS) to public endpoints (xrplcluster.com, s1.ripple.com, etc.)
- **xrpl-rust crate** (v1.1): XRPL SDK for transaction building/signing
- **ratatui** (v0.30) + **crossterm** (v0.29): Terminal UI framework
- **tokio** (v1): Async runtime
- **reqwest** (v0.13): HTTP client for JSON-RPC
- **clap** (v4): CLI argument parsing
- **config** (v0.15): Config file loading/merging
- **secrecy** (v0.10): Memory-masked secret storage for signing seeds
- **directories** (v6): XDG-compliant config/data directory resolution
- **tracing** ecosystem: Observability (tracing, tracing-subscriber, tracing-error)
- **color-eyre** / **human-panic** / **better-panic**: Error reporting and panic handling
- **signal-hook** (v0.4): SIGTSTP handling for terminal suspend

## Unknowns

- **Observed** (low confidence): Windows support status — code has `#[cfg(not(windows))]` guards for SIGTSTP but no explicit Windows testing.
- **Observed** (low confidence): The `graphify` knowledge graph reports 281 isolated nodes (≤1 connection), suggesting undocumented or weakly-connected components.
- **Observed** (medium confidence): No explicit integration tests for WS reconnect scenarios under network failure conditions.

## Next Recommended Analysis Targets

1. `src/app.rs` — application loop and component orchestration (Pass 2)
2. `src/xrpl/poll.rs` — polling task architecture and submit flows (Pass 2-3)
3. `src/components/shared/tx_detail/` — transaction detail rendering pipeline (Pass 2-3)
4. `src/config.rs` — config merge logic and keybinding resolution (Pass 3)
