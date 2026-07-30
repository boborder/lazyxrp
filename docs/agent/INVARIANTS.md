# Invariants

> **Read**: `REPO_INVENTORY.md`, `ARCHITECTURE.md`, `DATA_MODEL.md`. **Scope**: full repository. **Confidence**: medium-high. **Generated**: 2026-05-14 (Pass 3).

## Invariants

### I-1: Seed memory safety
**Rule**: Plaintext seeds in `config.toml` (`RawSigningConfig.seed`) MUST be cleared to `None` during `Config::new()`. Only `secret_seed: SecretString` remains.
**Enforcement**: `config.rs` — `Config::new()` post-processing.
**Confidence**: high.

### I-2: Mainnet write guard
**Rule**: Transaction submits (AccountSet, Payment, SetRegularKey, EscrowCreate, OfferCreate) MUST require `--yes` flag or `skip_mainnet_prompt: true` when `Network::is_mainnet()`.
**Enforcement**: `poll.rs` — each `submit_*_transaction()` checks `network.is_mainnet() && !params.skip_mainnet_prompt`.
**Confidence**: high.

### I-3: submit flow requires simulate → sign → submit
**Rule**: All transaction submits MUST follow: `simulate_tx` → check `engine_result == "tesSUCCESS"` → extract `Sequence`/`Fee` → `sign` → `submit`. Never sign+submit without simulation.
**Enforcement**: `poll.rs` — each submit function implements this flow.
**Confidence**: high.

### I-4: Single-threaded seed env mutation
**Rule**: `SigningConfig::prime_seed_source()` clears `XRPL_SEED` env var via `unsafe`. MUST only be called during single-threaded startup before tokio runtime starts.
**Enforcement**: Called from `main()` before `App::run()`. Comment in `signing.rs` documents the constraint.
**Confidence**: high.

### I-5: Config merge priority
**Rule**: CLI flags > env vars > user config.toml > built-in defaults.
**Enforcement**: `main.rs` for network/URL/seed — resolve functions apply CLI first, then env, then config default.
**Confidence**: high.

### I-6: Poll interval minimum
**Rule**: Periodic poll must not fire faster than `MIN_POLL_INTERVAL` (10s).
**Enforcement**: `poll.rs` — `run_poll_loop()` uses `tokio::time::interval(ctx.poll_interval).max(MIN_POLL_INTERVAL)`.
**Confidence**: high.

### I-7: Not-found is not an error
**Rule**: `account_tx` returning "not found" MUST produce empty TX list, not an error.
**Enforcement**: `poll.rs` — `poll_batch()` matches `account_tx` result: `Err(e) if is_not_found_error(&msg)` → sends `XrplTxHistory(vec![], None)`.
**Confidence**: high.

### I-8: TUI cleanup on drop
**Rule**: `Tui::drop()` MUST restore terminal (disable raw mode, leave alternate screen). MUST NOT panic during drop.
**Enforcement**: `tui.rs` — `Drop` impl catches `exit()` errors with `eprintln!`.
**Confidence**: high.

### I-9: Tab index matches panel order
**Rule**: `TAB_TITLES` array and `panels` Vec MUST have same length and index correspondence (currently 4).
**Enforcement**: `app.rs` — `debug_assert_eq!(TAB_TITLES.len(), panels.len(), ...)` at construction; unit test asserts equality.
**Confidence**: high.

### I-10: ArcValue sharing prevents deep clones
**Rule**: `TxRow.tx_json` and `TxRow.meta_json` use `ArcValue` to share JSON across components. Components MUST NOT mutate shared JSON.
**Enforcement**: Convention. `ArcValue` wraps `Arc<Value>` — mutation requires `Arc::make_mut`, not blocked at type level.
**Confidence**: medium (no compile-time immutability enforcement beyond `Arc` semantics).

### I-11: RPC timeout for all calls
**Rule**: Every RPC call MUST have a timeout (`RPC_TIMEOUT`).
**Enforcement**: `poll.rs` wraps all `rpc.*()` calls in `tokio::time::timeout(RPC_TIMEOUT, ...)`.
**Confidence**: high.

## Enforcement Locations

| Invariant | File(s) | Mechanism |
|-----------|---------|-----------|
| I-1 | `config.rs:Config::new()` | Post-processing clears seed |
| I-2 | `poll.rs` submit functions | Mainnet check before sign |
| I-3 | `poll.rs` submit functions | Sequential simulate→sign→submit |
| I-4 | `main.rs` + `signing.rs` | Startup ordering |
| I-5 | `main.rs` resolve_* functions | Precedence chain |
| I-6 | `poll.rs:run_poll_loop()` | `.max(MIN_POLL_INTERVAL)` |
| I-7 | `poll.rs:poll_batch()` | `is_not_found_error()` guard |
| I-8 | `tui.rs:Drop` | Error-caught exit |
| I-9 | `app.rs` | `debug_assert_eq!` + unit test (currently 4) |
| I-10 | `types.rs:ArcValue` | Arc immutability convention |
| I-11 | `poll.rs` | `tokio::time::timeout` |

## Unenforced Assumptions

- **UA-1** (resolved): Tab/panel length is asserted in `app.rs` (`debug_assert_eq!`) and covered by a unit test. Keep both `TAB_TITLES` and `panels` updates in the same change.
- **UA-2** (medium risk): `ArcValue` sharing relies on convention — nothing prevents a component from calling `Arc::make_mut` and mutating shared JSON.
- **UA-3** (low risk): `CancellationToken` cancellation is fire-and-forget. No guarantee that WS/poll tasks observe cancellation before `Tui::exit()`.
- **UA-4** (low risk): `Action` channel is `unbounded`. If the consumer (`App::process_actions`) blocks or lags, memory grows without bound.
- **UA-5** (low risk): `poll_trigger_tx` uses `unbounded` channel — WS task could flood poll task with trigger messages on rapid ledger closes.

## Risk Notes

- The submit flow in `poll.rs` is ~800 lines of intertwined validation, simulation, signing, and submission logic. A bug in one submit type could be replicated across others (copy-paste risk).
- Seed handling splits responsibility between `main.rs` (CLI seed), `config.rs` (file/env seed), and `signing.rs` (wallet derivation). The same seed can arrive from 3 sources — priority chain must be correct in all code paths.
