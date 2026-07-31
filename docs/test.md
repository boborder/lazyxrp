# Test Strategy & Case List

> Last Updated: 2026-07-31
> Target: lazyxrp (Rust TUI for XRPL)
> Total Test Cases: 97 (P0: 10, P1: 46, P2: 40, P3: 1) — 95 Rust + 2 Vitest（FAssets スキル）
> Implemented: 97 / 97 (100%)
> Estimated Effort (full catalog): 44.5h

---

## Summary

| Category          | Test Count | P0 | P1 | P2 | P3 | Est. Effort | Implemented |
| ----------------- | ---------- | -- | -- | -- | -- | ----------- | ----------- |
| XRPL Core         | 32         | 4  | 17 | 11 | 0  | 13h         | 32/32       |
| Config & Keybinds | 19         | 1  | 9  | 8  | 0  | 8h          | 19/19       |
| Network & Signing | 13         | 4  | 7  | 2  | 0  | 5h          | 13/13       |
| CLI Integration   | 13         | 1  | 8  | 4  | 0  | 12h         | 13/13       |
| Watch & TUI       | 18         | 0  | 5  | 12 | 1  | 6h          | 18/18       |
| FAssets skill scripts (Vitest) | 2 | 0  | 0  | 2  | 0  | 0.5h        | 2/2         |
| **Total**         | **97**     | **10** | **46** | **40** | **1** | **44.5h** | **97/97** |

---

## TDD Cycle Rules

### Red → Green → Refactor

1. **Red**: write exactly one test and confirm it fails as expected.
2. **Green**: minimum code needed to make that test pass.
3. **Refactor**: improve design while all tests stay Green.

### Project-specific rules

- Prefer inline `#[cfg(test)]` modules for unit and integration tests; avoid `tests/` directory.
- **FAssets executor helpers**（`.agents/skills/flare-fassets/scripts/`）だけ Vitest + `npm test`。CI の主系は引き続き `cargo test`。
- Use `pretty_assertions` for readable diffs.
- Map cases to this list by `TC-XXX`.
- **Catalog policy**: this file is an **important-case roster**, not a 1:1 index of every `#[test]`. Unannotated helper/unit tests are allowed; add a TC only when the case is cross-module contract, regression-prone, or user-visible.
- Reference the ID in commit messages (e.g., `test(xrpl): add NFT parse case (TC-013)`).
- For tests that touch `env::var`, mark `unsafe` block and single-threaded constraint.

---

## Test Execution Commands

```bash
# Run all tests
cargo test

# Run with backtrace on failure
RUST_BACKTRACE=1 cargo test

# Run a specific test
cargo test <test_name>

# Run specific module tests
cargo test xrpl::tests
cargo test config::tests

# Check only (fast feedback)
cargo check

# Format & lint
cargo fmt --check
cargo clippy

# FAssets skill — Direct Mint executor helpers (Vitest)
cd .agents/skills/flare-fassets/scripts && npm test
```

CI（`.github/workflows/ci.yml`）は各ジョブで `cargo … --locked`（例: `cargo test --locked --all-features --workspace`）。ローカルでもロック整合を合わせたいときは同様に `--locked` を付けられる。Vitest はスキル配下のオプション検証用で、現状ワークスペース CI には含めていない。

---

## Implemented Test Cases

### XRPL Core (`src/xrpl/`)

実装は `mod.rs` がサブモジュールを束ねて再エクスポートする。`types.rs`（データ型・`PollContext` / `PollCommand`）、`client.rs`（`RpcClient`・JSON-RPC・レスポンスパース・`xrp_to_drops`）、`json_util.rs`（`json_str` / `extract_json_u32`）、`poll.rs`（ポーリング・ウォレット送信）、`ws.rs`（WebSocket）、`cli_exec.rs`（`execute_cli_command`）、`backoff.rs`（WS/ポール共通バックオフ）。

#### TC-001: parse_currency — XRP uppercase

- **Priority**: P0
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `book_currency()`（旧 `parse_currency` 相当）
- **Input**: `"XRP"`, `None`
- **Expected Output**: `Currency::XRP(_)`
- **Test File**: `src/xrpl/client.rs` (inline) — `book_currency_xrp_uppercase`

#### TC-002: parse_currency — XRP case-insensitive

- **Priority**: P0
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `book_currency()`
- **Input**: `"xrp"`, `Some("rIssuer")`
- **Expected Output**: `Currency::XRP(_)`
- **Test File**: `src/xrpl/client.rs` (inline) — `book_currency_xrp_case_insensitive`

#### TC-003: parse_currency — issued currency is not XRP

- **Priority**: P0
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `book_currency()`
- **Input**: `"USD"`, `Some("rIssuer")`
- **Expected Output**: Issued JSON includes `currency` + `issuer` (not bare XRP object)
- **Test File**: `src/xrpl/client.rs` (inline) — `book_currency_issued_includes_issuer`

#### TC-004: json_str — nested path returns value

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/json_util.rs` -> `json_str()`
- **Input**: `{"a": {"b": "hello"}}`, path `["a", "b"]`
- **Expected Output**: `"hello"`
- **Test File**: `src/xrpl/json_util.rs` (inline)

#### TC-005: json_str — missing path returns empty

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/json_util.rs` -> `json_str()`
- **Input**: `{"a": {}}`, path `["a", "b"]` or `["x"]`
- **Expected Output**: `""`
- **Test File**: `src/xrpl/json_util.rs` (inline)

#### TC-006: json_u32 — returns number

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/json_util.rs` -> `extract_json_u32()`
- **Input**: `{"a": 42}`, path `["a"]`
- **Expected Output**: `42`
- **Test File**: `src/xrpl/json_util.rs` (inline)

#### TC-007: json_u32 — missing or non-numeric returns zero

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/json_util.rs` -> `extract_json_u32()`
- **Input**: `{"a": "foo"}`, path `["a"]` or `["x"]`
- **Expected Output**: `0`
- **Test File**: `src/xrpl/json_util.rs` (inline)

#### TC-008: drops_to_xrp — basic conversion

- **Priority**: P0
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `drops_to_xrp()`
- **Input**: `"1000000"`, `"250000"`
- **Expected Output**: `"1.000000"`, `"0.250000"`
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-009: drops_to_xrp — invalid string returns zero

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `drops_to_xrp()`
- **Input**: `"not-a-number"`
- **Expected Output**: `"0.000000"`
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-010: format_amount — None returns dash

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `format_amount()`
- **Input**: `None`
- **Expected Output**: `"-"`
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-011: format_amount — XRP drops string

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `format_amount()`
- **Input**: `Some(json!("1000000"))`
- **Expected Output**: `"1.000000"`
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-012: format_amount — issued currency object

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `format_amount()`
- **Input**: `Some(json!({"currency": "USD", "value": "1.5", "issuer": "rXyz"}))`
- **Expected Output**: `"1.5 USD"`
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-013: account_nfts — parse response into NftRow vec

- **Priority**: P1
- **Type**: Unit
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `account_nfts()`
- **Preconditions**: Mock RPC response with `account_nfts` result
- **Expected Output**: `Vec<NftRow>` with correct fields
- **Test File**: `src/xrpl/client.rs` (inline)
- **Notes**: Requires mocked HTTP client or test fixture JSON.

#### TC-074: account_nfts — tfMutable flag (dNFT)

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `parse_account_nfts_value()`
- **Input**: Fixture with `Flags: 16` (`NFTOKEN_FLAG_MUTABLE`)
- **Expected Output**: `NftRow.is_mutable == true`; missing `Flags` → `false`
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-075: xrpl-rust `Payment<'static>` deserialize from JSON

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `payment_static_deserialize()`
- **Input**: Fixture JSON with `TransactionType: "Payment"`, `Account`, `Destination`, `Amount`, `Fee`, `Sequence`, `Flags`
- **Expected Output**: `Payment<'static>` struct with correct `common_fields.account`, `destination`, parsed `Amount`
- **Test File**: `src/xrpl/client.rs` (inline)
- **Notes**: Validates `xrpl-rust` native type deserialization path for typed transaction detail rendering.

#### TC-014: account_lines — parse response into TrustLineRow vec

- **Priority**: P1
- **Type**: Unit
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `account_lines()`
- **Preconditions**: Mock RPC response with `account_lines` result
- **Expected Output**: `Vec<TrustLineRow>` with correct fields
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-015: amm_info — parse response into AmmSummary

- **Priority**: P2
- **Type**: Unit
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `amm_info()`
- **Preconditions**: Mock RPC response with `amm_info` result
- **Expected Output**: `AmmSummary` with asset pair and fee
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-016: account_tx — parse response into TxRow vec (with direction)

- **Priority**: P2
- **Type**: Unit
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `account_tx()`
- **Preconditions**: Mock RPC response with `account_tx` result
- **Expected Output**: `Vec<TxRow>` with hash, type, result, **direction** (`▼` outbound / `▲` inbound / `·` self-only)
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-017: book_offers — parse response into OfferRow vec

- **Priority**: P1
- **Type**: Unit
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `book_offers()`
- **Preconditions**: Mock RPC response with `book_offers` result
- **Expected Output**: `Vec<OfferRow>` with quality and amount
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-070: book_offers — issued quote uses `currency_code`

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/types.rs` -> `BookPair::pays_currency()`
- **Input**: Display quote `RLUSD` and 160-bit `currency_code`
- **Expected Output**: RPC currency value is the 160-bit code, not display symbol
- **Test File**: `src/xrpl/types.rs` (inline)

#### TC-071: account_objects — empty array parses

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `parse_account_objects_value()`
- **Input**: `result.account_objects` = `[]`
- **Expected Output**: empty `Vec`
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-072: account_objects — mixed Check / Ticket / MPT / PayChannel / Escrow

- **Priority**: P1
- **Type**: Unit
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `parse_account_objects_value()`, `summarize_ledger_object()`
- **Input**: JSON fixture with five `LedgerEntryType` variants
- **Expected Output**: five `LedgerObjectRow` with types and non-empty details where applicable
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-073: ledger object filters — tab visibility helpers

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/types.rs` -> `is_objects_tab_ledger_type()`, `is_pay_channel_type()`, `is_escrow_type()`
- **Input**: representative type strings
- **Expected Output**: Objects (misc) panel excludes `PayChannel`/`Escrow` but includes `DID`; channel/escrow predicates match
- **Test File**: `src/xrpl/client.rs` (inline)（`xrpl::types` の公開 API を参照）

#### TC-018: server_info/fee — parse response into summary structs

- **Priority**: P2
- **Type**: Unit
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `server_info()` / `fee()`
- **Preconditions**: Mock RPC response
- **Expected Output**: `ServerInfoSummary`, `FeeSummary`
- **Test File**: `src/xrpl/client.rs` (inline)

### Config & Keybinds (`src/config.rs`)

#### TC-019: parse_style — empty string yields default

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `parse_style()`
- **Input**: `""`
- **Expected Output**: `Style::default()`
- **Test File**: `src/config.rs` (inline)

#### TC-020: parse_style — foreground color

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `parse_style()`
- **Input**: `"red"`
- **Expected Output**: `fg = Some(Color::Indexed(1))`
- **Test File**: `src/config.rs` (inline)

#### TC-021: parse_style — background color

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `parse_style()`
- **Input**: `"on blue"`
- **Expected Output**: `bg = Some(Color::Indexed(4))`
- **Test File**: `src/config.rs` (inline)

#### TC-022: parse_style — modifiers combined

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `parse_style()`
- **Input**: `"underline red on blue"`
- **Expected Output**: fg=red, bg=blue, underlined
- **Test File**: `src/config.rs` (inline)

#### TC-023: extract_color_and_modifiers — extracts modifiers and color

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `extract_color_and_modifiers()`
- **Input**: `"underline bold inverse gray"`
- **Expected Output**: color=`"gray"`, modifiers contain UNDERLINED/BOLD/REVERSED
- **Test File**: `src/config.rs` (inline)

#### TC-024: parse_color — RGB shorthand

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `parse_color()`
- **Input**: `"rgb123"`
- **Expected Output**: `Some(Color::Indexed(16 + 36 + 2*6 + 3))`
- **Test File**: `src/config.rs` (inline)

#### TC-025: parse_color — unknown returns None

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `parse_color()`
- **Input**: `"unknown"`
- **Expected Output**: `None`
- **Test File**: `src/config.rs` (inline)

#### TC-026: Config::new — loads default keybindings

- **Priority**: P0
- **Type**: Unit
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `Config::new()`
- **Preconditions**: Default config file exists or built-in default used
- **Expected Output**: `keybindings.0[&Mode::Splash][&parse_key_sequence("<q>").unwrap()] == Action::Quit`（埋め込み `config.json5` の Splash 既定）
- **Test File**: `src/config.rs` (inline)

#### TC-027: parse_key_event — simple keys

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `parse_key_event()`
- **Input**: `"a"`, `"enter"`, `"esc"`
- **Expected Output**: `KeyEvent` with correct code and no modifiers
- **Test File**: `src/config.rs` (inline)

#### TC-028: parse_key_event — with modifiers

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `parse_key_event()`
- **Input**: `"ctrl-a"`, `"alt-enter"`, `"shift-esc"`
- **Expected Output**: `KeyEvent` with correct modifier flags
- **Test File**: `src/config.rs` (inline)

#### TC-029: parse_key_event — multiple modifiers

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `parse_key_event()`
- **Input**: `"ctrl-alt-a"`, `"ctrl-shift-enter"`
- **Expected Output**: `KeyEvent` with combined modifier flags
- **Test File**: `src/config.rs` (inline)

#### TC-030: key_event_to_string — reverse mapping

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `key_event_to_string()`
- **Input**: `KeyEvent::new(KeyCode::Char('a'), CONTROL | ALT)`
- **Expected Output**: `"ctrl-alt-a"`
- **Test File**: `src/config.rs` (inline)

#### TC-031: parse_key_event — invalid keys error

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `parse_key_event()`
- **Input**: `"invalid-key"`, `"ctrl-invalid-key"`
- **Expected Output**: `Err`
- **Test File**: `src/config.rs` (inline)

#### TC-032: parse_key_event — case insensitivity

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `parse_key_event()`
- **Input**: `"CTRL-a"`, `"AlT-eNtEr"`
- **Expected Output**: Same as lowercase variants
- **Test File**: `src/config.rs` (inline)

#### TC-033: Config merge — user overrides default (`poll_interval_ms` fixture)

- **Priority**: P1
- **Type**: Unit
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `Config::new()` merge logic
- **Preconditions**: `LAZYXRP_CONFIG` temp dir + `config.toml` with `poll_interval_ms` distinct from embedded default
- **Expected Output**: Loaded `xrpl.poll_interval_ms` matches user file
- **Test File**: `src/config.rs` (inline)

#### TC-034: Config merge — XDG directory resolution

- **Priority**: P1
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/config.rs` -> config file discovery
- **Preconditions**: Temp dir with `config.toml` under `$XDG_CONFIG_HOME/lazyxrp/`
- **Expected Output**: Config loaded from XDG path
- **Test File**: `src/config.rs` (inline)

#### TC-035: Config merge — fallback to ~/.config

- **Priority**: P2
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/config.rs` -> config file discovery
- **Preconditions**: No XDG var; file exists at `~/.config/lazyxrp/config.toml`
- **Expected Output**: Config loaded from fallback path
- **Test File**: `src/config.rs` (inline)

#### TC-036: Config validation — invalid key sequence format

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `parse_key_sequence()`
- **Input**: `"<invalid>"`
- **Expected Output**: `Err` or gracefully ignored
- **Test File**: `src/config.rs` (inline)
#### TC-092: Config merge — `XRPL_RPC_SERVER` overrides `rpc_server` from file

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/config.rs` -> `Config::new()` XRPL env merge
- **Preconditions**: `config.toml` sets `rpc_server`; `XRPL_RPC_SERVER` env set to a different URL
- **Expected Output**: `xrpl.rpc_server` matches env
- **Test File**: `src/config.rs` (inline)
- **Notes**: Formerly mislabeled as TC-075 (collision with Payment deserialize).


### Network & Signing

#### TC-037: Network::default — is Mainnet

- **Priority**: P0
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/network.rs` -> `Network::default()`
- **Expected Output**: `Network::Mainnet`
- **Test File**: `src/network.rs` (inline)

#### TC-038: Network::from_str — roundtrip mainnet/testnet/devnet

- **Priority**: P0
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/network.rs` -> `FromStr for Network`
- **Input**: `"mainnet"`, `"testnet"`, `"devnet"`
- **Expected Output**: Corresponding enum variants
- **Test File**: `src/network.rs` (inline)

#### TC-039: Network::from_str — case-insensitive

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/network.rs` -> `FromStr for Network`
- **Input**: `"MAINNET"`, `"Testnet"`
- **Expected Output**: Correct variants
- **Test File**: `src/network.rs` (inline)

#### TC-040: Network::from_str — unknown network errors

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/network.rs` -> `FromStr for Network`
- **Input**: `"foonet"`
- **Expected Output**: `Err`
- **Test File**: `src/network.rs` (inline)

#### TC-041: Network::is_mainnet — correct boolean

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/network.rs` -> `Network::is_mainnet()`
- **Expected Output**: `true` for Mainnet, `false` otherwise
- **Test File**: `src/network.rs` (inline)

#### TC-042: resolve_network — CLI flag overrides env

- **Priority**: P0
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/main.rs` -> `resolve_network()`
- **Preconditions**: `XRPL_NETWORK=testnet` env set; CLI `--network devnet`
- **Expected Output**: `Network::Devnet`
- **Test File**: `src/main.rs` (inline)

#### TC-043: resolve_rpc_url — mainnet default when no CLI/env/config override

- **Priority**: P0
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/main.rs` -> `resolve_rpc_url()`
- **Preconditions**: No env; config has no custom server; network = Mainnet
- **Expected Output**: `https://xrplcluster.com`
- **Test File**: `src/main.rs` (inline)

#### TC-044: resolve_ws_url — CLI --ws-server is top priority

- **Priority**: P1
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/main.rs` -> `resolve_ws_url()`
- **Preconditions**: Env `XRPL_WS_SERVER=wss://custom` and CLI `--ws-server wss://cli`
- **Expected Output**: `wss://cli`
- **Test File**: `src/main.rs` (inline)


#### TC-045: SigningConfig::load — no source returns no seed

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/signing.rs` -> `SigningConfig::load()`
- **Preconditions**: No env var; `None` passed as config seed
- **Expected Output**: `has_seed() == false`
- **Test File**: `src/signing.rs` (inline)
- **Notes**: Marked `unsafe` for env removal; single-threaded only.

#### TC-046: SigningConfig::load — config raw seed resolves

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/signing.rs` -> `SigningConfig::load()`
- **Preconditions**: Env unset; `Some("sTest1234")` passed
- **Expected Output**: `has_seed() == true`
- **Test File**: `src/signing.rs` (inline)

#### TC-047: prompt_mainnet_confirmation — testnet/devnet skips

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/signing.rs` -> `prompt_mainnet_confirmation()`
- **Input**: `("Payment", Testnet/Devnet, false)`
- **Expected Output**: `true` (skip prompt)
- **Test File**: `src/signing.rs` (inline)

#### TC-048: prompt_mainnet_confirmation — --yes skips on mainnet

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/signing.rs` -> `prompt_mainnet_confirmation()`
- **Input**: `("Payment", Mainnet, true)`
- **Expected Output**: `true` (skip prompt)
- **Test File**: `src/signing.rs` (inline)

#### TC-049: SigningConfig::load — env XRPL_SEED overrides config

- **Priority**: P1
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/signing.rs` -> seed priority resolution
- **Preconditions**: `XRPL_SEED=sEnv` env set; config also has seed
- **Expected Output**: Env seed is used (memory-masked)
- **Test File**: `src/signing.rs` (inline)
- **Notes**: Requires `unsafe` env block; run single-threaded.

### CLI Integration

#### TC-050: CLI info — exits with code 0

- **Priority**: P0
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/cli_exec.rs` -> `execute_cli_command(Cmd::Info, ...)`
- **Preconditions**: Outbound HTTPS to `https://xrplcluster.com` (90s timeout)
- **Expected Output**: `Ok(())`; server_info JSON printed
- **Test File**: `src/xrpl/cli_exec.rs` (`integration_live_network`)
- **Notes**: Live mainnet RPC; CI needs network access.

#### TC-051: CLI account — exits with code 0

- **Priority**: P1
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/cli_exec.rs` -> `execute_cli_command(Cmd::Account)`
- **Input**: Valid r-address
- **Expected Output**: Exit code 0; account_info printed
- **Test File**: `src/xrpl/cli_exec.rs` (`integration_live_network`)

#### TC-052: CLI book — exits with code 0

- **Priority**: P1
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/cli_exec.rs` -> `execute_cli_command(Cmd::Book)`
- **Input**: `--base XRP --quote USD --issuer <r-addr>`
- **Expected Output**: Exit code 0; offers printed
- **Test File**: `src/xrpl/cli_exec.rs` (`integration_live_network`)

#### TC-053: CLI summary — exits with code 0

- **Priority**: P1
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/cli_exec.rs` -> `execute_cli_command(Cmd::Summary)`
- **Expected Output**: Exit code 0; combined output
- **Test File**: `src/xrpl/cli_exec.rs` (`integration_live_network`)

#### TC-054: CLI nfts — exits with code 0

- **Priority**: P1
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/cli_exec.rs` -> `execute_cli_command(Cmd::Nfts)`
- **Expected Output**: Exit code 0; NFT list printed
- **Test File**: `src/xrpl/cli_exec.rs` (`integration_live_network`)

#### TC-055: CLI lines — exits with code 0

- **Priority**: P1
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/cli_exec.rs` -> `execute_cli_command(Cmd::Lines)`
- **Expected Output**: Exit code 0; trust lines printed
- **Test File**: `src/xrpl/cli_exec.rs` (`integration_live_network`)

#### TC-056: CLI amm — exits with code 0

- **Priority**: P2
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/cli_exec.rs` -> `execute_cli_command(Cmd::Amm)`
- **Expected Output**: Exit code 0; AMM pool printed
- **Test File**: `src/xrpl/cli_exec.rs` (`integration_live_network`)

#### TC-057: CLI txhistory — exits with code 0

- **Priority**: P2
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/cli_exec.rs` -> `execute_cli_command(Cmd::TxHistory)`
- **Expected Output**: Exit code 0; tx list printed
- **Test File**: `src/xrpl/cli_exec.rs` (`integration_live_network`)

#### TC-058: CLI — invalid parameters error

- **Priority**: P1
- **Type**: Integration
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/cli.rs` -> `clap` validation
- **Input**: `book` with only `--quote` or only `--base` (missing paired arg)
- **Expected Output**: Non-zero exit; usage error message
- **Test File**: `src/cli.rs` (inline)

#### TC-059: CLI — invalid r-address format

- **Priority**: P2
- **Type**: Integration
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/cli.rs` / `src/xrpl/cli_exec.rs`
- **Input**: `account not-an-address`
- **Expected Output**: `execute_cli_command` returns `Err`
- **Test File**: `src/xrpl/cli_exec.rs` (`integration_live_network`)

#### TC-076: CLI — `--self-uninstall` flag parses; backup path helper

- **Priority**: P1
- **Type**: Integration (clap) / Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/cli.rs` (`--self-uninstall`, `--yes`), `src/uninstall.rs` (`backup_candidate`)
- **Input**: `Cli::try_parse_from` with `--self-uninstall` / `--yes`; path suffix for `.bak`
- **Expected Output**: Flag parsed; Windows/Unix backup names end with `.bak`
- **Test File**: `src/cli.rs`, `src/uninstall.rs` (inline)

### Watch & TUI

#### TC-060: Watch mode — App builds 4 tabs with watch account (I-9)

- **Priority**: P1
- **Type**: Integration
- **Size**: L
- **Status**: [x] Done
- **Target**: `src/app.rs` -> `App::new()`
- **Preconditions**: Valid config (`Config::new` via `ENV_TEST_LOCK`)
- **Expected Output**: 4 tabs/panels; `active_tab == 0`; watch account set; not quit / help closed
- **Test File**: `src/app.rs` (inline)
- **Notes**: Full `App::run()` loop not exercised here. `test_app` passes `Config::new()` into `App::new`; production Watch passes the same `Config` as `main` (which cleared `XRPL_SEED` in `Config::new`), so merged `XRPL_SEED` is not lost.

#### TC-061: Watch mode — Quit action stops background tasks

- **Priority**: P1
- **Type**: Integration
- **Size**: L
- **Status**: [x] Done
- **Target**: `src/app.rs` -> `Action::Quit` → `drain_and_dispatch_actions`
- **Preconditions**: None (calls `drain_and_dispatch_actions(None)` — no TTY required on CI)
- **Input**: `Action::Quit` on action channel
- **Expected Output**: `should_quit == true` (mirrors `run()` cancel path)
- **Test File**: `src/app.rs` (inline)

#### TC-062: Watch mode — RefreshAccount sends PollCommand

- **Priority**: P2
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/app.rs` -> `Action::RefreshAccount` → poll channel
- **Preconditions**: `App::new` (test build keeps `test_poll_rx`)
- **Input**: `Action::RefreshAccount`
- **Expected Output**: `PollCommand::Account` received
- **Test File**: `src/app.rs` (inline)

#### TC-063: Watch mode — RefreshBook sends PollCommand

- **Priority**: P2
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/app.rs` -> `Action::RefreshBook` → poll channel
- **Input**: `Action::RefreshBook`
- **Expected Output**: `PollCommand::Book` received
- **Test File**: `src/app.rs` (inline)

#### TC-064: Watch mode — TabNext/TabPrev wrap around panels

- **Priority**: P2
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/app.rs` -> `Action::TabNext` / `Action::TabPrev`
- **Input**: Repeated `Action::TabNext` then `Action::TabPrev`
- **Expected Output**: With 4 tabs, `active_tab` cycles `0→1→2→3→0` then `0→3→2→1→0`
- **Test File**: `src/app.rs` (inline)

#### TC-065: Watch mode — HelpOverlay toggles on `?` and Esc

- **Priority**: P3
- **Type**: Integration
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/app.rs` / `src/components/help_overlay.rs`
- **Input**: `Action::Help` toggle; `?` key + `Esc` via `on_key_event`
- **Expected Output**: `show_help` toggles; Esc clears overlay when open
- **Test File**: `src/app.rs` (inline)
- **Notes**: Extra tests: `question_opens_help_overlay`, `esc_while_help_sends_help_action`.

#### TC-066: CLI — `account-status` against live RPC

- **Priority**: P1
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/cli_exec.rs` → `Cmd::AccountStatus`
- **Input**: Genesis account address on configured RPC
- **Expected Output**: Command completes (`Ok`) with account status printed
- **Test File**: `src/xrpl/cli_exec.rs` (inline, `#[tokio::test]`)
- **Notes**: Live RPC; may be environment-sensitive in CI.

#### TC-067: CLI — `send` simulation with seed

- **Priority**: P2
- **Type**: Integration
- **Size**: M
- **Status**: [x] Done
- **Target**: `src/xrpl/cli_exec.rs` → `Cmd::Send`
- **Input**: Destination + small amount; requires `XRPL_SEED`
- **Expected Output**: Simulation path succeeds
- **Test File**: `src/xrpl/cli_exec.rs` (inline, `#[ignore]` without seed)
- **Notes**: Ignored when `XRPL_SEED` is unset.

#### TC-068: XRPL RPC error — not found is not silently swallowed

- **Priority**: P1
- **Type**: Functional
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `ensure_no_xrpl_rpc_error`
- **Input**: XRPL response with `error = "actNotFound"`
- **Expected Output**: Function returns an error that remains classifiable as not-found
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-069: XRPL submit response — requires `tesSUCCESS` and hash

- **Priority**: P1
- **Type**: Functional
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `parse_submit_success`
- **Input**: Successful and failed `submit` response fixtures
- **Expected Output**: Success returns `TxSummary`; failed engine result returns an error
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-093: TxDetail render cache — invalidated on open

- **Priority**: P1
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/components/shared/tx_detail/mod.rs` -> `TxDetailState::open()`
- **Input**: `TxDetailState::open()` with two different JSON payloads
- **Expected Output**: `cached_lines` is `None` after each `open()` call
- **Test File**: `src/components/shared/tx_detail/mod.rs` (inline)
- **Notes**: Formerly mislabeled as TC-070 (collision with book_offers currency_code). Identity `to_static_lines` test removed as non-behavioral.

#### TC-094: TxDetail parser registry — required types, no duplicates

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/components/shared/tx_detail/parsers.rs` -> `TX_DETAIL_PARSERS`
- **Input**: Registry table
- **Expected Output**: Contains required TransactionType names; no duplicate names (avoids magic `len() == 29` drift)
- **Test File**: `src/components/shared/tx_detail/parsers.rs` (inline)
- **Notes**: Replaces the old count-only registry assert. Noise microbench `bench_detail_lines_payment_current` was removed (not a TC).

#### TC-095: wallet_propose — ed25519 seed/address

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` wallet_propose parsing
- **Input**: ed25519 propose fixture
- **Expected Output**: Seed/address fields parsed
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-096: get_aggregate_price — full response

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` oracle aggregate parser
- **Input**: Full aggregate price JSON
- **Expected Output**: Price + stats fields
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-097: get_aggregate_price — trimmed_set omitted

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` oracle aggregate parser
- **Input**: Response without `trimmed_set`
- **Expected Output**: Still parses (defaults / empty trim set)
- **Test File**: `src/xrpl/client.rs` (inline)



#### TC-087: Poll trigger — coalesces rapid bursts

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/poll.rs` -> `drain_poll_trigger_burst()`, `should_skip_poll_trigger()`
- **Input**: Multiple `()` queued on `poll_trigger_rx`; `last_poll` within / past `MIN_POLL_INTERVAL`
- **Expected Output**: Burst drained to one trigger; rate-limit helper skips poll when interval not elapsed
- **Test File**: `src/xrpl/poll.rs` (inline)

#### TC-088: Mainnet Payment guard (R-006)

- **Priority**: P0
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/poll.rs` -> `submit_payment_transaction()`, `mainnet_write_guard_blocks()`
- **Input**: `Network::Mainnet`, `skip_mainnet_prompt: false` vs `true`
- **Expected Output**: Without `--yes` → `PaymentSubmitErr` mentioning `mainnet` / `--yes`; with `--yes` → guard skipped (later error allowed)
- **Test File**: `src/xrpl/poll.rs` (inline)

#### TC-089: account_tx not-found → empty history (I-7)

- **Priority**: P0
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` -> `empty_account_tx_page_on_not_found()`; `src/xrpl/poll.rs` -> `account_tx_poll_action()`
- **Input**: `actNotFound` error string / `Result::Err`
- **Expected Output**: Empty `AccountTxPage` or `Action::XrplTxHistory(vec![], None)` — never `XrplError` for not-found
- **Test File**: `src/xrpl/client.rs`, `src/xrpl/poll.rs` (inline)

#### TC-077: TxHistoryPanel — filter by tx_type

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/components/panels/tx_history.rs` -> `reapply_filter()`
- **Input**: Rows with mixed `tx_type`; filter `payment`
- **Expected Output**: Only matching rows remain visible
- **Test File**: `src/components/panels/tx_history.rs` (inline)

#### TC-078: TxHistoryPanel — filter by hash partial

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/components/panels/tx_history.rs` -> `reapply_filter()`
- **Input**: Partial hash substring
- **Expected Output**: Rows whose hash contains substring
- **Test File**: `src/components/panels/tx_history.rs` (inline)

#### TC-079: TxHistoryPanel — transaction filter

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/components/panels/tx_history.rs` -> `reapply_filter()`
- **Input**: empty `filter_input`
- **Expected Output**: All rows shown (`filtered` cleared / full list)
- **Test File**: `src/components/panels/tx_history.rs` (inline) — `filter_empty_shows_all`
- **Notes**: Complements TC-077/078. Account tab lower pane owns history UI and filtering.

#### TC-080: ripple_path_find — parse alternatives

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` path-find parsers
- **Input**: Fixture JSON with path alternatives
- **Expected Output**: Parsed path rows / amounts
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-081: ripple_path_find — empty alternatives

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs`
- **Input**: Response with empty `alternatives`
- **Expected Output**: Empty path list (not error)
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-082: ripple_path_find — `source_amount` as XRP drops string

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs`
- **Input**: `source_amount` string (drops)
- **Expected Output**: Parsed amount display
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-083: summarize_paths_computed — hop chain abbreviation

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` → `summarize_paths_computed`
- **Input**: Multi-hop path
- **Expected Output**: Abbreviated hop label
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-084: path_find_rows_from — display rows

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/client.rs` → `path_find_rows_from`
- **Input**: Parsed path alternatives
- **Expected Output**: UI row structs
- **Test File**: `src/xrpl/client.rs` (inline)

#### TC-085: asset_display_name — known hex codes

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/types.rs` -> `asset_display_name()`
- **Input**: RLUSD / USDC hex currency codes
- **Expected Output**: Human-readable symbols
- **Test File**: `src/xrpl/types.rs` (inline)

#### TC-086: asset_display_name — unknown passthrough

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `src/xrpl/types.rs` -> `asset_display_name()`
- **Input**: `"UNKNOWN"`
- **Expected Output**: Unchanged string
- **Test File**: `src/xrpl/types.rs` (inline)

### FAssets skill scripts (Vitest)

#### TC-090: FAssets — `normalizeTxId` (XRPL payment hash for FDC)

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `.agents/skills/flare-fassets/scripts/execute-direct-mint.helpers.ts` → `normalizeTxId()`
- **Input**: 64 hex with/without `0x`、不正長さ・非 hex
- **Expected Output**: 大文字 64 hex の正規化、不正入力は `Error`
- **Test File**: `.agents/skills/flare-fassets/scripts/execute-direct-mint.helpers.test.ts`（Vitest 4 ケース）

#### TC-091: FAssets — `resolveWatchStartBlock` (watch 起点ブロック)

- **Priority**: P2
- **Type**: Unit
- **Size**: S
- **Status**: [x] Done
- **Target**: `.agents/skills/flare-fassets/scripts/execute-direct-mint.helpers.ts` → `resolveWatchStartBlock()`
- **Input**: `WATCH_FROM_BLOCK` 相当の文字列 or 未設定、`latest` ブロック番号
- **Expected Output**: 既定は `latest - 5000`（下限 0）、`requested > latest` は clamp + `clamped` メタ
- **Test File**: `.agents/skills/flare-fassets/scripts/execute-direct-mint.helpers.test.ts`（Vitest 7 ケース）

---

## Implementation Roadmap

### Sprint 1: Foundation (P0 — Stabilize)

| Order | ID     | Test Case Name                       | Size | Status | Completed |
| ----- | ------ | ------------------------------------ | ---- | ------ | --------- |
| 1     | TC-042 | resolve_network — CLI overrides env  | M    | [x]    | 2026-05-01 |
| 2     | TC-043 | resolve_rpc_url — network default    | M    | [x]    | 2026-05-01 |
| 3     | TC-050 | CLI info — exit code 0               | M    | [x]    | 2026-05-01 |

### Sprint 2: Core Coverage (P1 — CLI + Network)

| Order | ID     | Test Case Name                              | Size | Status | Completed |
| ----- | ------ | ------------------------------------------- | ---- | ------ | --------- |
| 4     | TC-013 | account_nfts parse                          | M    | [x]    | 2026-05-01 |
| 5     | TC-014 | account_lines parse                         | M    | [x]    | 2026-05-01 |
| 6     | TC-017 | book_offers parse                           | M    | [x]    | 2026-05-01 |
| 7     | TC-051 | CLI account                                 | M    | [x]    | 2026-05-01 |
| 8     | TC-052 | CLI book                                    | M    | [x]    | 2026-05-01 |
| 9     | TC-053 | CLI summary                                 | M    | [x]    | 2026-05-01 |
| 10    | TC-044 | resolve_ws_url priority                     | M    | [x]    | 2026-05-01 |
| 11    | TC-049 | SigningConfig env overrides config          | M    | [x]    | 2026-05-01 |
| 12    | TC-033 | Config merge — user `poll_interval_ms`      | M    | [x]    | 2026-05-01 |
| 13    | TC-034 | Config merge — XDG resolution               | M    | [x]    | 2026-05-01 |
| 14    | TC-058 | CLI invalid parameters error                | S    | [x]    | 2026-05-01 |

### Sprint 3: Watch Stability (P1–P2 — TUI)

| Order | ID     | Test Case Name                        | Size | Status | Completed |
| ----- | ------ | ------------------------------------- | ---- | ------ | --------- |
| 15    | TC-060 | App builds 4 tabs (I-9)               | L    | [x]    | 2026-07-30 |
| 16    | TC-061 | Quit stops background tasks           | L    | [x]    | 2026-05-16 |
| 17    | TC-062 | RefreshAccount sends PollCommand      | M    | [x]    | 2026-05-16 |
| 18    | TC-063 | RefreshBook sends PollCommand         | M    | [x]    | 2026-05-16 |

### Sprint 4: Edge Cases & Completeness (P2–P3)

| Order | ID     | Test Case Name                             | Size | Status | Completed |
| ----- | ------ | ------------------------------------------ | ---- | ------ | --------- |
| 19    | TC-015 | amm_info parse                             | M    | [x]    | 2026-05-01 |
| 20    | TC-016 | account_tx parse                           | M    | [x]    | 2026-05-01 |
| 21    | TC-018 | server_info/fee parse                      | M    | [x]    | 2026-05-01 |
| 22    | TC-035 | Config fallback (`HOME` + `config_dir`)  | M    | [x]    | 2026-05-01 |
| 23    | TC-036 | Config invalid key sequence                | S    | [x]    | 2026-05-01 |
| 24    | TC-054 | CLI nfts                                   | M    | [x]    | 2026-05-01 |
| 25    | TC-055 | CLI lines                                  | M    | [x]    | 2026-05-01 |
| 26    | TC-056 | CLI amm                                    | M    | [x]    | 2026-05-01 |
| 27    | TC-057 | CLI txhistory                              | M    | [x]    | 2026-05-01 |
| 28    | TC-059 | CLI invalid r-address format               | S    | [x]    | 2026-05-01 |
| 29    | TC-064 | TabNext/TabPrev cycles panels              | M    | [x]    | 2026-05-01 |
| 30    | TC-065 | HelpOverlay toggles                        | S    | [x]    | 2026-05-01 |
| 31    | TC-068 | XRPL not-found error classification         | S    | [x]    | 2026-05-05 |
| 32    | TC-069 | XRPL submit success validation              | S    | [x]    | 2026-05-05 |
| 33    | TC-070 | book_offers quote uses currency_code        | S    | [x]    | 2026-05-05 |
| 34    | TC-071 | account_objects empty parse               | S    | [x]    | 2026-05-11 |
| 35    | TC-072 | account_objects mixed types                 | M    | [x]    | 2026-05-11 |
| 36    | TC-073 | ledger object tab filters                   | S    | [x]    | 2026-05-11 |
| 37    | TC-074 | account_nfts tfMutable (dNFT)             | S    | [x]    | 2026-05-11 |
| 38    | TC-075 | xrpl-rust Payment<'static> deserialize    | S    | [x]    | 2026-05-13 |
| 38b   | TC-092 | Config XRPL_RPC_SERVER overrides file      | S    | [x]    | 2026-07-30 |
| 38c   | TC-093 | TxDetail cache invalidated on open         | S    | [x]    | 2026-07-30 |
| 38d   | TC-094 | TxDetail parser registry coverage          | S    | [x]    | 2026-07-30 |
| 39    | TC-076 | `--self-uninstall` parse + `.bak` path    | S    | [x]    | 2026-05-12 |
| 40    | TC-077 | TxHistoryPanel filter by tx_type          | S    | [x]    | 2026-05-14 |
| 41    | TC-078 | TxHistoryPanel filter by hash partial     | S    | [x]    | 2026-05-14 |
| 42    | TC-079 | TxHistoryPanel empty filter               | S    | [x]    | 2026-05-14 |
| 42a   | TC-066 | CLI account-status live RPC               | M    | [x]    | 2026-07-31 |
| 42b   | TC-067 | CLI send simulation (seed)                | M    | [x]    | 2026-07-31 |
| 42c   | TC-080 | ripple_path_find parse                    | S    | [x]    | 2026-07-31 |
| 42d   | TC-081 | ripple_path_find empty alternatives       | S    | [x]    | 2026-07-31 |
| 42e   | TC-082 | ripple_path_find source_amount string     | S    | [x]    | 2026-07-31 |
| 42f   | TC-083 | summarize_paths_computed                  | S    | [x]    | 2026-07-31 |
| 42g   | TC-084 | path_find_rows_from                       | S    | [x]    | 2026-07-31 |
| 42h   | TC-095 | wallet_propose ed25519                    | S    | [x]    | 2026-07-31 |
| 42i   | TC-096 | get_aggregate_price full                  | S    | [x]    | 2026-07-31 |
| 42j   | TC-097 | get_aggregate_price trimmed omitted       | S    | [x]    | 2026-07-31 |
| 43    | TC-087 | Poll trigger burst coalesce               | S    | [x]    | 2026-05-16 |
| 44    | TC-088 | Mainnet Payment guard (R-006)             | S    | [x]    | 2026-05-16 |
| 45    | TC-089 | account_tx not-found → empty (I-7)        | S    | [x]    | 2026-05-16 |
| 46    | TC-090 | FAssets normalizeTxId                    | S    | [x]    | 2026-05-16 |
| 47    | TC-091 | FAssets resolveWatchStartBlock           | S    | [x]    | 2026-05-16 |

---

## Progress Dashboard

### Overall

- **Total Cases**: 97（うち TC-090/091 は Vitest・スキル配下; TC-066/067・080–084・095–097 は 2026-07-31 にカタログ欠落/ID衝突を解消）
- **Implemented**: 97
- **Passing**: `cargo test --locked` green after 2026-07-30 cleanup + Vitest 11（`cd .agents/skills/flare-fassets/scripts && npm test`）
- **Failing**: 0
- **Ignored**: 5 (live RPC / benchmark / flare; see case notes)
- **Todo**: 0
- **Coverage**: CLI + watch paths exercised; line % not measured here

### Recently Completed

| Date       | ID     | Action      | Notes |
| ---------- | ------ | ----------- | ----- |
| 2026-05-01 | TC-001 | Verified    | Existing inline tests confirmed passing |
| 2026-05-01 | TC-037 | Verified    | `cargo test` all green |
| 2026-05-01 | TC-013–018, TC-033–036, TC-042 | Implemented | JSON fixture parsers + config env + `resolve_network` |
| 2026-05-01 | TC-043–065 | Implemented | URL resolve, signing env priority, live RPC CLI |
| 2026-05-16 | TC-061–065 | Implemented | `App` integration tests use `#[tokio::test]` (no `#[ignore]`) |
| 2026-05-16 | TC-087–089 | Implemented | Poll burst coalesce, mainnet Payment guard, I-7 not-found |
| 2026-05-16 | TC-090–091 | Implemented | Vitest: `normalizeTxId` + `resolveWatchStartBlock`（`execute-direct-mint` watch clamp） |
| 2026-07-30 | TC cleanup | Fixed | Removed identity/mirror tests; fixed TC-014 mislabel; renumbered colliding IDs → TC-092/093/094; strengthened asserts |
| 2026-05-16 | TC-001–003 | Implemented | Direct `book_currency()` unit tests |
| 2026-05-01 | — | Refactored | `TestEnvGuard` + `env_lock()` added to fix Mutex poison across env tests |
| 2026-05-05 | TC-068–070 | Implemented | XRPL not-found handling, submit response validation, and book_offers currency_code selection |
| 2026-05-12 | TC-076 | Implemented | `lazyxrp --self-uninstall` CLI + uninstall helper tests |

---

## Coverage Goals

- **Unit-tested helpers** (`book_currency` / RPC 構築、`drops_to_xrp`、`format_amount`、キーパース): 80%+ line coverage
- **Config loading & merging**: 70%+ line coverage
- **Network/Signing resolution**: 80%+ line coverage
- **CLI commands** (`info`/`account`/`book`/`summary`/`nfts`/`lines`/`amm`/`txhistory`): at least 1 success path each
- **Watch mode**: startup, quit, refresh triggers each covered by 1+ integration test

---

## Known Issues & Constraints

- **Environment variable tests** (`TC-033`–`TC-036`, `TC-042`–`TC-044`, `TC-045`–`TC-049`, `TC-092`) use `config::env_lock()` + `TestEnvGuard` for RAII save/restore of env vars; `env_lock()` recovers from Mutex poison automatically.
- **`xrpl::tests::integration_live_network`** hits `https://xrplcluster.com` (90s timeout per case) and serializes calls with a test-local mutex to reduce public-node rate limiting. Offline / blocked CI: non-ignored live tests fail; prefer runners with outbound HTTPS or mark live tests ignored in CI if needed.
- **`tokio::spawn` lifetime issue** (rust-lang/rust#100013) was previously tracked for watch startup; current `start_poll_task` / `start_ws_task` paths compile with direct `tokio::spawn` and should remain covered by `cargo check`.
- **macOS linking warnings** from upstream deps are non-fatal; do not treat as test failures.
- **TUI tests** (`TC-060`–`TC-065`) run under `#[tokio::test]`; `TC-061`–`TC-065` exercise `drain_and_dispatch_actions(None)` so they pass on headless CI without `Tui::new()`. Live RPC CLI tests remain `#[ignore]` when network/seed-dependent.
- **TC-ID note**: TC-079 = TxHistory empty filter. Path-find parsers = TC-080–084; `wallet_propose` = TC-095; oracle aggregate = TC-096/097. CLI live helpers = TC-066/067.
