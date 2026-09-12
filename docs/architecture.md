# architecture.md — System & behavioral design

**Role:** 詳細設計・スキーマ設計（ネットワーク・設定・データフロー・パネル契約の SSOT）。UI キー／レイアウトは [`DESIGN.md`](../DESIGN.md)。

Extended product design derived from [`requirements.md`](requirements.md). Describes **how the system behaves** (boundaries, network, config precedence, data flow, panel contracts). Test cases in [`test.md`](test.md) trace back here and to requirements FR/NFR ids.

**Not here:** keybindings, colors, layout splits, scroll/modal interaction — SSOT is root [`DESIGN.md`](../DESIGN.md).

| Topic | SSOT |
|-------|------|
| Look, keys, focus, theme | [`DESIGN.md`](../DESIGN.md) |
| FR/NFR ids | [`requirements.md`](requirements.md) |
| Modules, channels, startup/submit flows | `src/app.rs`, `src/xrpl/poll.rs`, [`directory.md`](directory.md) §3 |
| Rules I-1〜I-11 | [`security.md`](security.md) (guards in `src/`) |
| Directory layout | [`directory.md`](directory.md) |
| TX detail parser registry | §5.2.1 below, `src/components/shared/tx_detail/` |
| Security audit S-xxx / risks R-xxx | [`security.md`](security.md) |
| Doc index | [`AGENTS.md`](../AGENTS.md) progressive disclosure |
| Graphify | [`graphify-out/GRAPH_REPORT.md`](../graphify-out/GRAPH_REPORT.md) |

## System boundary

lazyxrp is a **single Rust binary** run locally. It connects to public **XRP Ledger** endpoints (JSON-RPC / WebSocket) for reads and signed submission when configured. Optional **Flare** RPC reads (FTSO, FXRP AssetManager) and optional C3 execute when explicitly enabled.

## Runtime structure (single process)

| Area | Role | Primary code |
|------|------|--------------|
| Watch (TUI) | Dashboard, panels, keyboard flows | `app.rs`, `tui.rs`, `components/` |
| CLI runner | Non-interactive subcommands | `cli.rs`, `xrpl/cli_exec.rs` |
| XRPL integration | RPC, WebSocket, poll task → `Action` | `xrpl/client.rs`, `ws.rs`, `poll.rs` |
| Config & signing | Merged `config.toml`, credentials | `config.rs`, `signing.rs` |
| Flare | FTSO / FXRP read (+ optional execute) | `flare.rs`, `poll.rs` |

Module wiring summary: [`directory.md`](directory.md) §3. Tab/composer **diagrams**: [`DESIGN.md`](../DESIGN.md).

## 1. 起動モードとスタック（要約）

| 入口 | 用途 | 例 |
|------|------|-----|
| `lazyxrp` | **TUI**（既定） | `lazyxrp --account <ADDR>` |
| `lazyxrp -x <subcmd>` | **スクリプト CLI** | `lazyxrp -x info` |
| `rc <subcmd>` | 同上（シェル alias） | `alias rc='lazyxrp -x'` |
| `rp` | lookup 専用バイナリ | `rp -t <txid\|ADDR>` |
| `lazyxrp --self-uninstall` | アンインストール | `logging::init` 前に終了 |

- エントリ: `src/lib.rs` — TUI → `App::new` → `run`、`-x` CLI → `xrpl::execute_cli_command(...)`.
- レガシー: `lazyxrp watch` / 裸サブコマンドは deprecation 警告付き。正規形は上表。
- スタック: `ratatui` + `crossterm`、非同期 `tokio`、XRPL は `src/xrpl/`。

## 2. ウォレット（行動契約）

`WalletPanel` + `wallet_composer.rs` on Account tab. **Interaction keys and colors:** [`DESIGN.md`](../DESIGN.md) § Wallet.

Behavioral contract:

- Credential resolution: `Config::new()` merges `XRPL_SEED` / `XRPL_MNEMONIC`, then `credential_from_secrets` → `PollContext.signing_seed` (`Option<SigningCredential>`).
- All wallet TX: **simulate → sign → submit** (I-3). Mainnet / Xahau mainnet writes require CLI **`--yes`** (I-2).
- Composer phases shipped: Payment, AccountSet, SetRegularKey, OfferCreate, TrustSet, FXRP Mint (C2), FXRP Execute (C3). EscrowCreate: signing path exists; UI unwired (P-003).
- Payment validation: destination shape, positive amount, IOU issuer `r…`, currency non-empty.
- FXRP C2: 32-byte memo to Core Vault; C3: pasted FDC proof JSON + `[flare.fassets] execute=true`.

## 3. ランタイム・データフロー

Implementation: `src/xrpl/poll.rs`, `src/app.rs`.

| Direction | Path |
|-----------|------|
| Input | Keyboard → `tui::Event` → `Action`; XRPL RPC/WS → `Action` |
| Manual refresh | `Action::Refresh*` → `PollCommand` → poll task |
| Submit | `Action::*Submit` → `PollCommand` → simulate/sign/submit in poll |
| Output | `Component::update` → `draw` (gated by `needs_draw`) |

WebSocket ledger close coalesces poll triggers (`MIN_POLL_INTERVAL`). Scheduled poll uses `poll_interval_ms` from config (default **5000** ms for interactive use; **15000–30000** ms recommended for long-running / always-on sessions to reduce public RPC load).

**Public-network politeness:** XRPL Foundation dUNL (`unl.xrplf.org`) is cached in-process for **10 minutes** (`DUNL_CACHE_TTL`). Flare `ContractRegistry` lookups (`FtsoV2`, `AssetManagerFXRP`) are cached per RPC URL for the process lifetime. Poll failures use exponential backoff (2s→60s cap); HTTP 429 is retried with delay.

## 4. 設定値と起動パラメータ

### CLI

| Flag | Effect |
|------|--------|
| `--yes` | Skip mainnet write confirmation (CLI + TUI submit) |
| `--network` | `mainnet` \| `testnet` \| `devnet` \| `xahau` \| `xahau-test` |
| `--server`, `--ws-server` | Custom RPC/WS (highest priority) |
| `--allow-insecure-rpc` | Allow `http://` / `ws://` |
| `--tick-rate`, `--frame-rate` | TUI loop rates |
| `--seed` / `--mnemonic` | Override credentials (**non-preferred** — use env/config) |
| `--account` | Watch address override |
| `-x` / `--exec` | Script CLI mode |

### Config file (`Config::xrpl`)

- Paths: `$XDG_CONFIG_HOME/lazyxrp/config.toml` → `~/.config/lazyxrp/config.toml`
- Fields: `network`, `account`, `issuer`, `currency`, `currency_code`, `offer_limit`, `poll_interval_ms` (default 5000; raise for 24/7 dashboards), `oracles`, `oracle_pairs`, `[xrpl.signing]`, optional `rpc_server` / `ws_server`
- Default issuer fallback (mainnet Bitstamp USD): `rvYAfWj5gh67oV6fW32ZzP3Aw4Eubs59B`
- `currency_code`: 160-bit hex for `book_offers` (default display `"USD"`)

### Flare (`[flare]`)

| Field | Default | Notes |
|-------|---------|-------|
| `network` | `flare` | `flare` \| `songbird` \| `coston2` — RPC preset; `FLARE_RPC_URL` env wins |
| `display` | `full` | `full` \| `compact` \| `off` — panel layout + FTSO/FXRP poll skip when `off` (C3 URL unchanged) |
| `fassets.execute` | `false` | Must be true for C3 `executeDirectMinting` |
| `wallet.address` | _(unset)_ | Optional `0x…` EVM address — Overview read panel (native FLR + FXRP); invalid hex fails config load |

## 5. Read APIs & panels (FR-08–FR-11)

Types, RPC methods, `Action` variants: `src/xrpl/types.rs`. UX layout: [`DESIGN.md`](../DESIGN.md).

### 5.1 Panel inventory

| Tab | Components | Primary RPC / source |
|-----|------------|----------------------|
| Overview | `ServerPanel`, `CombinedOraclePanel`, `FlareWalletPanel` | `server_info`, `fee`, `get_aggregate_price`, Flare FTSO, FXRP AssetManager, `eth_getBalance` + FXRP `balanceOf` (Overview tab only) |
| Account | `WalletPanel`, `AccountPanel`, `TxHistoryPanel` | `account_info`, `account_tx`, wallet submit |
| Market | Book, PathFind, AMM, TrustLines, FTSO, Oracle | `book_offers`, `ripple_path_find`, `amm_info`, `account_lines`, oracles |
| Assets | `NftTab`, `LedgerObjectsPanel` | `account_nfts`, `account_objects` |

Poll skips heavy market fetches when tab inactive (`should_poll_market_book`, `should_poll_asset_panels` in `poll.rs`).

### 5.2 Shared behaviors

- **TX detail:** Any table row with `ArcValue` tx/meta can open unified overlay (`tx_detail::render_tx_detail`). Parser contract: §5.2.1. Overlay **keys**: [`DESIGN.md`](../DESIGN.md) § TX detail overlay.
- **Tx history pagination:** `marker` → `PollCommand::TxHistoryMore` → `XrplTxHistoryAppend`.
- **Tx history filter:** In-panel filter state; does not change RPC query (client-side).
- **Path find:** Self-payment preview via `ripple_path_find` using configured book pair amount.

#### 5.2.1 TX detail parser pipeline

| Symbol | File | Role |
|--------|------|------|
| `TxDetailState` | `tx_detail/mod.rs` | Overlay visibility, scroll offset, line cache |
| `render_tx_detail` | `tx_detail/mod.rs` | Draw centered popup |
| `typed_detail_lines` | `tx_detail/parsers.rs` | Dispatch by `TransactionType` |
| `detail_lines_for` | `tx_detail/mod.rs` | Fallback for unknown fields |
| `TX_DETAIL_PARSERS` | `tx_detail/parsers.rs` | Static registry (29 types) |

Any table row carrying `raw_json: ArcValue` (tx + meta) can open the overlay via `Action::TxDetailToggle` when focused.

**Pipeline**

1. **Header** — `Result` always; `hash` / `ledger` / `date` when present in JSON.
2. **Typed branch** — `typed_detail_lines` looks up `TransactionType` in `TX_DETAIL_PARSERS`; parser returns formatted `Line` list or `None`.
3. **Fallback** — `detail_lines_for` walks known field names; unlisted keys via `format_value`. `Amount2` is known for AMM typed sections (not duplicated under “Other fields”).
4. **Cache** — First open per TX builds `cached_lines`; redraw reuses cache; scroll only moves paragraph offset. New TX invalidates cache.

**Overlay guard** (`TxDetailState::handle_action`)

- While **closed**: panels handle `TxDetailToggle` with pre-resolved `(tx, meta)` from selected row.
- While **open**: consumes `Quit`, data refresh, `Tick`, and navigation actions so background tables do not change selection under the popup.
- `SelectNext` / `FocusNext` (and prev) scroll detail text with saturating arithmetic.

**Registered types (29)**

`Payment`, `AccountSet`, `TrustSet`, `OfferCreate`, `OfferCancel`, `NFTokenMint`, `NFTokenBurn`, `NFTokenCreateOffer`, `NFTokenAcceptOffer`, `NFTokenCancelOffer`, `CheckCreate`, `CheckCash`, `CheckCancel`, `SignerListSet`, `SetRegularKey`, `DepositPreauth`, `EscrowCreate`, `EscrowFinish`, `EscrowCancel`, `PaymentChannelCreate`, `PaymentChannelFund`, `PaymentChannelClaim`, `AMMCreate`, `AMMDeposit`, `AMMWithdraw`, `AMMVote`, `AMMBid`, `AMMDelete`, `TicketCreate`.

Six AMM variants share `amm_detail_lines` (ordered fields, explicit `null`, `TradingFee` formatting on create/vote).

**Adding a parser**

1. Implement `fn foo_detail_lines(tx: &Value) -> Option<Vec<Line>>` in `parsers.rs` (prefer `typed_detail` helper).
2. Add one row to `TX_DETAIL_PARSERS`.
3. Add `/// TC-xxx` test in `parsers.rs` or `registry_tests` when behavior is user-visible or regression-prone.
4. Update the type list in this section if the registry grows.

No `DESIGN.md` change unless overlay interaction changes.

**Tests:** **TC-094** — registry has no duplicates; required types dispatch. Panel integration — tables open overlay on `Enter` (see `docs/test.md` wallet/history rows).

### 5.3 Path-Find (Market)

- Polling: `book_pair.path_find_destination_amount_preview()` as destination amount.
- Display: routes sorted cheapest-send-first; raw route JSON available via TX detail pattern.

### 5.4 Ledger objects

`PollCommand::LedgerObjects` → `account_objects` → `Action::XrplLedgerObjects`. Assets tab filters by `LedgerEntryType` (objects / pay channels / escrows).

---

## 6. Network & signing (FR-12–FR-13)

### 6.1 Network presets

| Network | RPC | WS |
|---------|-----|-----|
| mainnet | `https://xrplcluster.com` | `wss://xrplcluster.com` |
| testnet | `https://s.altnet.rippletest.net:51234` | `wss://s.altnet.rippletest.net:51233` |
| devnet | `https://s.devnet.rippletest.net:51234` | `wss://s.devnet.rippletest.net:51233` |
| xahau | `https://xahau.network` | `wss://xahau.network` |
| xahau-test | `https://xahau-test.net` | `wss://xahau-test.net` |

**TUI session switch:** `<Ctrl-n>` cycles networks (see DESIGN.md). Poll/WS rebound via `network_watch`; panels refresh. **Not persisted** to config. Custom `--server` / `XRPL_RPC_SERVER` → `custom_rpc`: endpoint unchanged on switch. **Mainnet guard** applies to `mainnet` and `xahau`.

### 6.2 Resolution precedence

```
CLI --network > XRPL_NETWORK env > config.toml [xrpl] network > default (mainnet)
CLI --server / --ws-server > XRPL_RPC_SERVER / XRPL_WS_SERVER env > config rpc_server / ws_server > network preset
```

### 6.3 Config example

```toml
[xrpl]
network = "mainnet"   # mainnet | testnet | devnet | xahau | xahau-test

[flare]
network = "flare"     # flare | songbird | coston2
display = "full"      # full | compact | off
```

### 6.4 Connection bootstrap

`src/lib.rs`: `resolve_network`, `resolve_rpc_url`, `resolve_ws_url`. `src/main.rs` is thin `lazyxrp::run()`.

### 6.5 StatusBar network indicator

Right-aligned badge `format!(" {} ", display_name())`; mainnet/xahau use warning styling. Updates on `Action::NetworkChange` (TC-115).

### 6.6 Secrets (FR-13)

Load order (seed **xor** mnemonic):

1. CLI `--seed` or `--mnemonic`
2. `XRPL_SEED` / `XRPL_MNEMONIC`
3. `[xrpl.signing]` in config

`SecretString` + `SigningCredential`; mainnet prompt via `prompt_mainnet_confirmation` unless `--yes`.

### 6.7 Flare display levels

| Level | Overview | Market | Poll |
|-------|----------|--------|------|
| `full` | 44/56 split, full FTSO/FXRP panels | Full FTSO table | FTSO + FXRP fetch on Overview tab |
| `compact` | 44/56, one-line FTSO/FXRP summaries | One-line FTSO | Same as full |
| `off` | Server full width | FTSO hidden; oracle remains | Skip FTSO/FXRP **read** fetch; C3 execute URL kept |

### 6.8 Flare wallet read panel (FR-16)

- Config: `[flare.wallet] address` (optional EVM `0x…`).
- Overview right column: `CombinedOraclePanel` (top) + `FlareWalletPanel` (~6 lines, bottom) when `display != off`.
- Poll: `fetch_flare_wallet_balance` on Overview tab only; emits `Action::FlareWalletBalance`. Unconfigured address → in-panel setup guidance (no RPC).
- Read-only: shows native FLR, FXRP ERC20 balance, `[flare.fassets] execute` gate, executor env key indicator. No Flare writes.

---

## 7. Operations notes

- `start_poll_task` = `tokio::spawn` with `PollContext`.
- WS triggers debounced; interval poll uses `poll_interval_ms`. dUNL + Flare registry addresses are session-cached (see §3).
- Risks / debt: [`problems.md`](problems.md), [`security.md`](security.md).
