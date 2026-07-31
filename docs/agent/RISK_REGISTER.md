# Risk Register

> **Read**: `ARCHITECTURE.md`, `DATA_MODEL.md`, `INVARIANTS.md`, `DEPENDENCY_RULES.md`. **Scope**: full repository. **Confidence**: medium. **Generated**: 2026-05-14 (Pass 5).

**SSOT:** Implementation risks **R-001〜R-010** live here. Security audit history **S-001〜S-011** is in [`../security.md`](../security.md) with an S↔R table. Enforced rules **I-1〜I-11** are in [`INVARIANTS.md`](INVARIANTS.md).

| R-ID | S-ID (if any) | One-line |
|------|---------------|----------|
| R-001 | S-001, S-002, S-010, S-011 | Seed resolution / `secret_seed` vs cleared `seed` |
| R-002 | — | Submit errors dropped on closed `action_tx` |
| R-003 | — | `ArcValue` shared JSON mutation |
| R-004 | — | Unbounded channel growth |
| R-005 | S-006 | TUI Drop / terminal raw mode |
| R-006 | — | Mainnet `--yes` guard bypass |
| R-007 | — | Config merge precedence per-key |
| R-008 | — | RPC 429 / backoff |
| R-009 | — | Submit hash not verified |
| R-010 | — | Duplicate poll on ledger close |
| R-011 | — | Poll `RpcClient::connect` “instant death” (**accepted / non-issue**) |

## R-001: Seed priority chain inconsistency

- **Severity**: High
- **Confidence**: Medium
- **Evidence**: Seed can arrive from 3 sources: `--seed` CLI flag (`main.rs:67-74`), `XRPL_SEED` env var (`signing.rs:prime_seed_source`), `config.toml [xrpl.signing] seed` (`config.rs:Config::new`). The CLI path sets `config.xrpl.signing.secret_seed` directly, then `prime_seed_source` is called with that value. If someone adds a new code path that reads `config.xrpl.signing.seed` directly (not `secret_seed`), they'd get stale/cleared data.
- **Failure scenario**: New feature adds seed resolution in a different location, reads the cleared `seed` field instead of `secret_seed`, signs with wrong key or fails to sign.
- **Affected files**: `src/main.rs`, `src/config.rs`, `src/signing.rs`
- **Suggested test**: Integration test that sets seed via CLI, env, and file simultaneously, asserts correct wallet address derived.
- **Suggested fix**: Centralize all seed resolution into `SigningConfig::resolve()` that takes Option<cli_seed> and returns the canonical seed source.

## R-002: Submit pipeline error silently swallowed

- **Severity**: High
- **Confidence**: Medium
- **Evidence**: Each submit function in `poll.rs` (~5 functions) handles errors locally with `action_tx.send(Action::*SubmitErr(...))`. If the `action_tx` channel is closed (e.g., app quit during submit), the error is silently dropped. The `let _ = action_tx.send(...)` pattern in `dispatch!` macro also loses errors.
- **Failure scenario**: User submits transaction, app quits, submit completes but result is lost. User doesn't know if TX went through.
- **Affected files**: `src/xrpl/poll.rs`
- **Suggested test**: Test that submitting during shutdown produces a logged warning at minimum.
- **Suggested fix**: Replace `let _ = action_tx.send(...)` with a `warn!` on send failure for critical operations.

## R-003: ArcValue mutation corrupts shared state

- **Severity**: Medium
- **Confidence**: Low
- **Evidence**: `TxRow.tx_json` and `TxRow.meta_json` use `ArcValue(Arc<Value>)` shared across components (e.g., TxHistory panel and TxDetail overlay). Nothing prevents `Arc::make_mut` from cloning and mutating.
- **Failure scenario**: A component inadvertently calls `Arc::make_mut` on shared JSON, changing display data for another component mid-render.
- **Affected files**: `src/xrpl/types.rs`, `src/components/shared/tx_detail/mod.rs`, `src/components/panels/tx_history.rs`
- **Suggested test**: Test that JSON values in TxRow are not mutated by TxDetail overlay.
- **Suggested fix**: Document the immutability contract on `ArcValue` struct. Consider a `ReadOnly<T>` wrapper.

## R-004: Unbounded channel memory growth under load

- **Severity**: Low
- **Confidence**: Low
- **Status**: Partially mitigated (Stage 3, 2026-05-15)
- **Evidence**: `action_tx`, `poll_tx`, `poll_trigger_tx` are all `UnboundedSender`. On fast networks with rapid ledger closes, `poll_trigger_tx` could accumulate events if poll task is slow.
- **Failure scenario**: Busy network → 50 ledger closes in 1 second → 50 triggers queued → poll task processes all serially → each fires a poll → UI lag.
- **Affected files**: `src/app.rs`, `src/xrpl/ws.rs`, `src/xrpl/poll.rs`
- **Mitigation applied**: `poll_trigger_rx.try_recv()` loop in `drive_poll_loop()` coalesces accumulated triggers before each poll execution.
- **Remaining risk**: `action_tx` and `poll_tx` are still unbounded; backpressure not yet addressed.
- **Suggested test**: Benchmark with simulated rapid trigger events.
- **Suggested fix**: Use `try_send()` with drop-oldest semantics for trigger channel, or coalesce triggers.

## R-005: TUI Drop panic leaves terminal in raw mode

- **Severity**: High
- **Confidence**: High
- **Evidence**: `Tui::drop()` catches `exit()` errors with `eprintln!` (line 234 in `tui.rs`). This is explicit by design (S-006). However, if `exit()` panics internally (e.g., crossterm bug), the `eprintln!` won't execute.
- **Failure scenario**: Crossterm internal panic during `disable_raw_mode()` → Drop panics (abort) → terminal stuck in raw mode → user must `reset` terminal.
- **Affected files**: `src/tui.rs`
- **Suggested test**: Hard to test (requires injecting a failing crossterm). Risk accepted with current mitigation.
- **Suggested fix**: Add `std::panic::catch_unwind` in Drop as defense-in-depth.

## R-006: Mainnet write guard bypass via config merging

- **Severity**: Critical
- **Confidence**: Low
- **Evidence**: Mainnet write guard checks `network.is_mainnet() && !params.skip_mainnet_prompt`. The `skip_mainnet_prompt` flag is set from `app.rs` (derived from CLI `--yes`) and passed through `Action::*Submit`. Default is `false` (guard active). If a future code path constructs `PaymentSubmitParams` with `skip_mainnet_prompt: true` without verifying the user passed `--yes`, mainnet writes could succeed without confirmation.
- **Failure scenario**: Feature adds "quick send" action that constructs `PaymentSubmitParams { skip_mainnet_prompt: true, .. }` without checking `--yes` flag.
- **Affected files**: `src/app.rs`, `src/xrpl/poll.rs`, `src/components/panels/wallet.rs`
- **Suggested test**: Test that `skip_mainnet_prompt` is always `false` when constructed outside the `--yes` path.
- **Suggested fix**: Make the guard server-side in poll task: require `--yes` be validated once and stored in PollContext, not per-request.

## R-007: Config merge precedence not tested per-key

- **Severity**: Medium
- **Confidence**: Medium
- **Evidence**: Config merge order (built-in → file → env → CLI) is tested for network/URL resolution in `main.rs`, but individual config keys like `poll_interval_ms`, `offer_limit`, `issuer` have no per-key precedence tests.
- **Failure scenario**: Adding a new config key without updating the merge logic correctly.
- **Affected files**: `src/config.rs`, `src/main.rs`
- **Suggested test**: Per-key merge tests: set each key in built-in, file, env; assert correct final value.
- **Suggested fix**: Test helper that constructs a `Config` with layer overrides and verifies each field.

## R-008: XRPL RPC 429 rate-limit not handled

- **Severity**: Medium
- **Confidence**: Low
- **Evidence**: `RpcClient` in `client.rs` has no explicit 429 rate-limit handling. `poll.rs` has `next_backoff_secs()` for WS but not for RPC. A busy account with rapid `account_tx` pagination could hit rate limits.
- **Failure scenario**: User rapidly paginates through tx history → 429 from xrplcluster → error displayed but not retried.
- **Affected files**: `src/xrpl/client.rs`, `src/xrpl/poll.rs`
- **Suggested test**: Mock test for 429 response with retry-after header.
- **Suggested fix**: Add exponential backoff on 429 responses in RPC client.

## R-009: submit_tx does not verify tx_hash in response

- **Severity**: Low
- **Confidence**: Low
- **Evidence**: Submit result parsing (`parse_submit_success`) checks for `tesSUCCESS` and `hash` presence but does not verify the returned hash matches the signed transaction hash.
- **Failure scenario**: Server returns success for a different transaction → user believes their TX went through.
- **Affected files**: `src/xrpl/client.rs`
- **Suggested test**: Mock submit response with mismatched hash.
- **Suggested fix**: Compute expected hash from signed tx_blob and compare with response hash.

## R-010: Ledger close WS event may arrive after poll completes

- **Severity**: Low
- **Confidence**: Low
- **Evidence**: WS `ledgerClosed` triggers poll via `poll_trigger_tx`. If poll is already running when trigger arrives, the trigger queues but may fire an unnecessary second poll for the same ledger.
- **Failure scenario**: Minor — wasted RPC call (same data returned).
- **Affected files**: `src/xrpl/ws.rs`, `src/xrpl/poll.rs`
- **Suggested test**: Check for duplicate poll within one ledger index.
- **Suggested fix**: Track last polled ledger index, skip if trigger is for same index.

## R-011: Poll task exits on `RpcClient::connect` failure (accepted)

- **Severity**: Low (accepted / non-issue)
- **Confidence**: High
- **Status**: Resolved as **non-harmful** (quality-pass B4, 2026-07-31)
- **Evidence**: `RpcClient::connect` only parses the URL and builds `AsyncJsonRpcClient` + `reqwest` (`HttpClient::new()`). No network I/O. `xrpl-rust` defers HTTP to `request_impl`. `drive_poll_loop` returns early only on URL/builder failure; runtime RPC failures use existing `backoff_until` in `select!`.
- **Failure scenario**: Only malformed URL / HTTP client builder errors kill the poll task — not unreachable nodes.
- **Affected files**: `src/xrpl/client.rs`, `src/xrpl/poll.rs`
- **Decision**: **Do not** add a connect-retry loop. Document only; keep backoff for request failures.

