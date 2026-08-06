# Risk-to-Tests Plan

> **Read**: `RISK_REGISTER.md`, `INVARIANTS.md`, `DATA_MODEL.md`, `CHANGE_GUIDE.md`. **Scope**: top 5 risks → test plan. **Generated**: 2026-05-14 (Pass 10).

## Selection Summary

Top 5 risks selected by: severity × confidence × testability × regression likelihood.

| Priority | Risk | Severity | Confidence | Why test first |
|----------|------|----------|------------|----------------|
| 1 | R-006: Mainnet write guard bypass | Critical | Low | Catastrophic if triggered; guard is safety-critical |
| 2 | R-007: Config merge precedence untested | Medium | Medium | Cheap to test; high regression surface (~1000 line config.rs) |
| 3 | R-002: Submit channel-close observability | Medium | High | Warn is emitted; durable result remains unimplemented |
| 4 | R-001: Seed priority chain inconsistency | High | Medium | Affects signing correctness |
| 5 | R-003: ArcValue shared JSON immutability | Medium | Low | Hard to trigger, high debugging cost |

---

## Test 1: R-006 — Mainnet write guard bypass

### Why test first
The mainnet write guard (`network.is_mainnet() && !skip_mainnet_prompt`) is the only barrier preventing accidental mainnet transaction submission. If bypassed, real XRP could be lost.

### Test type
**Integration test** (crosses `app.rs` → `poll.rs` boundary)

### Existing/new test file
New: `tests/mainnet_guard.rs` or extend existing `src/app.rs` tests

### Exact scenario
1. Construct `PaymentSubmitParams` with `skip_mainnet_prompt: true`
2. Set network to `Mainnet`
3. Send `Action::PaymentSubmit(params)` through action channel
4. Verify poll task rejects with `PaymentSubmitErr` containing "mainnet" / "--yes"
5. Repeat with `Testnet` — verify submission proceeds (network check passes)

### Expected behavior
- Mainnet + `skip_mainnet_prompt: false` → submit proceeds (user confirmed)
- Mainnet + `skip_mainnet_prompt: true` → reject with mainnet error
- Testnet + either value → submit proceeds

### Minimal implementation plan
1. Create test helper: `fn test_poll_context(network: Network) -> PollContext`
2. Set up `mpsc` channels for action_tx/rx
3. Send `PaymentSubmitParams { skip_mainnet_prompt: true, .. }` with Mainnet
4. Assert `action_rx` receives `PaymentSubmitErr` with "mainnet" in message
5. Repeat for Testnet → assert no `PaymentSubmitErr`

---

## Test 2: R-007 — Config merge precedence per-key

### Why test first
Config merge (built-in → file → env → CLI) is ~1000 lines of logic but only tested for network/URL. Every new config key risks incorrect precedence. Cheap to test, high regression value.

### Test type
**Unit test** (in `src/config.rs`)

### Existing/new test file
`src/config.rs` (extend existing test module)

### Exact scenario
1. Set each config key at all 4 layers (built-in via `config.json5`, file via temp `config.toml`, env via `TestEnvGuard`, CLI via parsed args)
2. Assert final resolved value matches the highest-priority layer that sets it
3. Test keys: `account`, `poll_interval_ms`, `offer_limit`, `issuer`, `currency`, `network`

### Expected behavior
- Built-in only → built-in value
- File overrides built-in → file value
- Env overrides file → env value
- CLI overrides env → CLI value
- Each key behaves consistently per the merge contract

### Minimal implementation plan
1. Create `fn test_config_merge(key: &str, builtin_val, file_val, env_val, cli_val)`
2. Use `TestEnvGuard` + temp config dir pattern (existing in test suite)
3. Assert `Config::new()` resolves each layer correctly
4. Generate per-key test cases (or use a macro for DRY)

---

## Test 3: R-002 — Submit pipeline errors on channel close

### Why test first
When the action channel closes (e.g., app quit during submit), submit results are silently dropped via `let _ = action_tx.send(...)`. User can't know if their TX succeeded. Affects invariant I-3.

### Test type
**Unit test** (in `src/xrpl/poll.rs`)

### Existing/new test file
`src/xrpl/poll.rs` (add test module)

### Exact scenario
1. Set up poll task with a mock RPC client (or real client hitting testnet)
2. Drop the `action_rx` receiver (simulating app quit)
3. Trigger a submit
4. Assert the submit function logs a warning (via tracing subscriber capture) OR returns a Result instead of silently dropping

### Expected behavior
- `action_tx.send(...)` failure is logged at `warn!` level
- Submit doesn't panic when channel is closed

### Minimal implementation plan
1. Create `mpsc::unbounded_channel()`, immediately drop `rx`
2. Call `tx.send(Action::PaymentSubmitErr("test".into()))`
3. Assert that either: (a) the code under test uses `warn!` on send failure, or (b) the test captures that the send was attempted and failed gracefully
4. If current code doesn't log: this test will fail, confirming the gap

---

## Test 4: R-001 — Seed priority chain

### Why test first
Seed can arrive from 3 sources (CLI, env, file). The priority chain must be correct in all code paths. A bug means signing with wrong key or failing to sign.

### Test type
**Integration test** (crosses `main.rs` → `config.rs` → `signing.rs`)

### Existing/new test file
`src/main.rs` (extend `network_resolve_tests`) or new `tests/seed_resolution.rs`

### Exact scenario
1. Set seed via `config.toml` (`[xrpl.signing] seed = "sEd..._file"`)
2. Set `XRPL_SEED` env var to a different seed (`"sEd..._env"`)
3. Pass `--seed sEd..._cli` via CLI
4. Assert the resolved seed is the CLI seed (highest priority)
5. Remove CLI seed; assert env seed wins
6. Remove env seed; assert file seed wins
7. Assert derived wallet address matches expected for each seed

### Expected behavior
- Priority: CLI > env > file
- `secret_seed` has correct value, `seed` is `None` after `Config::new()`
- Derived wallet address matches the winning seed

### Minimal implementation plan
1. Generate 3 distinct test seeds (e.g., via `wallet_propose` on testnet)
2. Compute expected addresses for each
3. Write temp config.toml with seed_1
4. Set `XRPL_SEED` to seed_2
5. Parse CLI with `--seed seed_3`
6. Call `Config::new()` + seed resolution
7. Derive wallet from `secret_seed`, assert address matches seed_3

---

## Test 5: R-003 — ArcValue immutability under concurrent access

### Why test first
`ArcValue(Arc<Value>)` is shared across TxHistory panel and TxDetail overlay. If any code path calls `Arc::make_mut`, it would corrupt shared state in hard-to-debug ways.

### Test type
**Unit test** (in `src/xrpl/types.rs`)

### Existing/new test file
`src/xrpl/types.rs` (add test module)

### Exact scenario
1. Create an `ArcValue` from a JSON object `{"hash": "ABC", "type": "Payment"}`
2. Clone it (simulating sharing between components)
3. Verify both clones point to the same `Arc` allocation (same strong count)
4. Verify `Arc::strong_count` > 1 when shared
5. Access JSON through one clone and verify content is unchanged

### Expected behavior
- `ArcValue::clone()` shares the same allocation (not a deep copy)
- JSON content is identical through both handles
- `Arc::strong_count()` reflects sharing

### Minimal implementation plan
1. `let v = ArcValue::new(serde_json::json!({"hash": "test"}))`
2. `let v2 = v.clone()`
3. `assert_eq!(v.0.as_ref(), v2.0.as_ref())` — same pointer
4. `assert_eq!(Arc::strong_count(&v.0), 2)` — shared
5. This is a characterization test — it documents expected sharing behavior
