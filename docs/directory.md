# directory.md

[日本語](directory-ja.md)

**Role:** Directory layout and naming rules (index of `src/` module responsibilities and the `docs/` hierarchy).

## 1. Directory layout

```txt
lazyxrp/
├── Cargo.toml
├── rust-toolchain.toml
├── Cargo.lock
├── build.rs
├── README.md
├── DESIGN.md              # UI/UX SSOT (keys, layout, theme, modals)
├── ROADMAP.md             # goal / milestone SSOT (docs/roadmap-log.md: cautions, pivots)
├── ROADMAP-ja.md          # Japanese twin of ROADMAP.md
├── CHANGELOG.md           # user-facing changes per release (Keep a Changelog)
├── SECURITY.md            # vulnerability reporting + safety model (details: docs/security.md)
├── config.json5
├── .env.example
├── install.sh
├── mise.toml
├── deny.toml
├── AGENTS.md
├── .github/workflows/     # ci.yml / cd.yml / benchmark.yml
├── src/
│   ├── lib.rs                 # shared library (lazyxrp + rp binaries)
│   ├── main.rs                # thin `lazyxrp` binary entry
│   ├── bin/
│   │   └── rp.rs              # thin `rp` lookup binary entry
│   ├── app.rs
│   ├── action.rs
│   ├── cli.rs
│   ├── config.rs
│   ├── network.rs
│   ├── signing.rs
│   ├── flare.rs
│   ├── flare_cmd.rs           # Flare EVM write commands (`-x fxrp-execute` / `fxrp-redeem` / `fxrp-tag-*` / `sa-*`)
│   ├── fdc.rs                 # Flare Data Connector proof flow (`prove_xrp_payment`)
│   ├── coverage.rs            # XRPL TX coverage SSOT (`-x coverage`)
│   ├── test_support.rs        # test helpers (`#[cfg(test)]`)
│   ├── tui.rs
│   ├── uninstall.rs
│   ├── logging.rs
│   ├── errors.rs
│   ├── xrpl/
│   │   ├── mod.rs
│   │   ├── address.rs
│   │   ├── client.rs
│   │   ├── cli_exec.rs
│   │   ├── dunl.rs
│   │   ├── format.rs
│   │   ├── parse.rs
│   │   ├── nft_image.rs          # NFT metadata/image fetch and limits
│   │   ├── poll.rs               # poll loop + read batch; PollCommand dispatch
│   │   ├── submit.rs             # write paths: guard → simulate → sign → submit, quote
│   │   ├── txspec/               # declarative tx specs
│   │   │   ├── mod.rs            # TxSpec / FieldSpec / registry
│   │   │   ├── nft.rs            # NFT family
│   │   │   ├── channel_amm.rs    # PaymentChannel / AMM families
│   │   │   ├── checks_misc.rs    # Check / Ticket / DepositPreauth / SignerListSet / Clawback
│   │   │   ├── identity.rs       # Credential / DID / Oracle / PermissionedDomain
│   │   │   ├── mpt.rs            # MPT family
│   │   │   └── xchain.rs         # XChain bridge family
│   │   ├── toml.rs
│   │   ├── types.rs
│   │   ├── util.rs               # JSON path helpers + reconnect backoff
│   │   └── ws.rs
│   └── components/
│       ├── mod.rs
│       ├── panels/
│       │   ├── mod.rs
│       │   ├── account.rs
│       │   ├── amm.rs
│       │   ├── book.rs
│       │   ├── combined_oracle.rs
│       │   ├── flare_ftso.rs
│       │   ├── flare_wallet.rs
│       │   ├── fxrp_direct_mint.rs
│       │   ├── ledger_objects.rs
│       │   ├── oracle.rs
│       │   ├── path_find.rs
│       │   ├── server.rs
│       │   ├── server_detail.rs
│       │   ├── server_dunl.rs
│       │   ├── trust_lines.rs
│       │   ├── tx_history.rs
│       │   ├── wallet.rs
│       │   ├── wallet_composer.rs
│       │   ├── wallet_keygen.rs
│       │   └── wallet_keys.rs
│       ├── tabs/
│       │   ├── mod.rs
│       │   ├── overview.rs          # Tab 0: Server + Oracle/FTSO/FXRP
│       │   ├── account_wallet.rs    # Tab 1: Wallet/Account + TxHistory
│       │   ├── market_oracle.rs     # Tab 2: Book / Lines / AMM / FTSO
│       │   ├── assets.rs            # Tab 3: NFT + ledger objects
│       │   └── nft.rs               # used by AssetsTab
│       └── shared/
│           ├── mod.rs
│           ├── fmt.rs
│           ├── fps.rs
│           ├── help_overlay.rs
│           ├── selectable_table.rs
│           ├── splash.rs
│           ├── status_bar.rs
│           ├── theme.rs
│           ├── widgets.rs
│           └── tx_detail/
│               ├── mod.rs
│               ├── format.rs
│               └── parsers.rs
├── tests/
│   ├── cli_smoke.rs           # binary boundary smoke
│   └── testnet_e2e.rs         # opt-in live testnet/devnet writes
├── docs/
│   ├── architecture.md  # system & behavior design (incl. TX detail §5.2.1)
│   ├── architecture-ja.md
│   ├── requirements.md  # FR/NFR (what it does)
│   ├── requirements-ja.md
│   ├── tech.md
│   ├── tech-ja.md
│   ├── maintenance.md  # release / publish flow (operations)
│   ├── maintenance-ja.md
│   ├── test.md  # TC catalog (traced to requirements / architecture)
│   ├── test-ja.md
│   ├── roadmap-log.md  # pivot records, planning cautions
│   ├── roadmap-log-ja.md
│   ├── directory.md
│   ├── directory-ja.md
│   ├── references.md
│   ├── references-ja.md
│   ├── security.md
│   ├── security-ja.md
│   ├── problems.md
│   └── problems-ja.md
└── .agents/skills/         # domain skills (xrpl-rust, flare-*)
```

**Docs naming:** every `docs/*.md` and `ROADMAP.md` has a Japanese twin `*-ja.md` in the same directory. The English file is canonical for agents. Any change to one MUST be made to the other in the same change, and the ID sets (TC/FR/NFR/S/R/I/P/F) MUST match.

## 2. Root files

- `Cargo.toml`: crate definition, dependencies, and features.
- `rust-toolchain.toml`: the Rust channel **shared by CI and local builds** (currently `stable`). `dtolnay/rust-toolchain` in `.github/workflows/ci.yml` (SHA-pinned, `# v1`) is given the same string as this file's `channel` as its `toolchain` input (required in v1; `rustfmt` / `clippy` are specified via `components` in the relevant jobs). Actions SHA updates are handled by `.github/dependabot.yml`.
- `config.json5`: built-in defaults such as keybindings (the `include_str!` target of `src/config.rs`). Separate from the user config `config.toml`.
- `Cargo.lock`: pinned dependency versions (**committed for the binary**; CI runs `cargo … --locked`).
- `build.rs`: build-time helper.
- `README.md`: user-facing overview and startup steps.
- `DESIGN.md`: UI/UX SSOT (keys, layout, theme).
- `ROADMAP.md` / `ROADMAP-ja.md`: goal hierarchy (final goal / long / mid / short term) and milestones (progress SSOT); the Japanese file is its twin. Cautions and pivot records are in `roadmap-log.md`.
- `CHANGELOG.md`: user-facing change history (Keep a Changelog). Add entries under `[Unreleased]`; finalize to a version number at release. Also bundled in the crates.io package.
- `SECURITY.md`: vulnerability reporting channel (GitHub private vulnerability reporting) and a summary of the safety model. Details and history are in `docs/security.md`.
- `.env.example`: example `XRPL_*` environment variables (optional; see `docs/tech.md` and the implementation for the full list).
- `install.sh`: interactive installer (requires `curl` **or** `wget`). Prompts and messages are in English. `--help` lists the CLI (`--method cargo|binary`, `--install-rust` / `--no-install-rust`, `--install-mise` / `--no-install-mise`, `-q`). `CI=1` is equivalent to `-q`. Can append to the shell profile if PATH is not set. If the release archive contains `rp` it is installed; otherwise an `rp` → `lazyxrp` symlink is created. **Manual uninstall** is described by `--uninstall-help` (`lazyxrp --self-uninstall`, removing `INSTALL_DIR/rp`, etc.).
- `mise.toml`: [mise](https://mise.jdx.dev/) tasks (`install`, `tags` (list), `tag-push` (manual tag fallback for emergencies), `bench` / `bench-fast` / `bench-ci` (benchmarks)). For the publish flow (CI detects the `Cargo.toml` version → auto-tag → CD publish) see [`maintenance.md`](maintenance.md).
- `AGENTS.md`: project operating rules and execution contract (prohibitions, reading order). graphify structure output (`graphify-out/`) is gitignored and generated locally only.
- `deny.toml`: cargo-deny configuration (advisories / licenses / bans / sources). Used by CI `security-advisories` and `mise run audit`.

## 3. Responsibilities under `src/`

- `lib.rs`: library shared by both the `lazyxrp` and `rp` binaries. `run()` (TUI+CLI) and `run_rp()` (lookup only).
- `coverage.rs`: XRPL TX type coverage (`rows()` = Existing / Spec (from `xrpl::txspec::registry()`) / Planned / Unsupported; reads come from tx_detail parser registration). `lazyxrp -x coverage` prints `markdown_table()`, and the drift test (TC-248) verifies the counts in ROADMAP.md "XRPL coverage" and the README.
- `xrpl/txspec/`: declarative TX specs (`mod.rs` = `TxSpec` / `FieldSpec` / `FAMILIES` registry, validation, `prepare`; `nft.rs` = NFTokenMint / Burn / CreateOffer / AcceptOffer / CancelOffer). Common submit is `submit::submit_generic_transaction`. How to add a type: `architecture.md` §2.1. `tests/testnet_e2e.rs` has an opt-in NFT mint → burn test.
- `main.rs`: thin entry of the `lazyxrp` binary (`lazyxrp::run()`).
- `bin/rp.rs`: thin entry of the `rp` binary (`lazyxrp::run_rp()`). Always shipped with `cargo install` / release tarballs.
- `uninstall.rs`: `lazyxrp --self-uninstall` — removes the running binary, the `{name}.bak` beside it, the `rp` beside it (file/symlink), and the config/data directories resolved via `Config` (does not call `cargo uninstall`).
- `app.rs`: the TUI application itself. Event loop, component management, and launching background work.
- `xrpl/`: everything XRPL-related. `mod.rs` only re-exports. `address.rs` (classic/X-Address resolution, network-match check), `client.rs` (`RpcClient` façade, JSON-RPC / HTTPS dUNL fetch, `tx` lookup), `dunl.rs` (XRPLF dUNL JSON, manifest ST), `format.rs` (amount / path / ripple-time formatting, `xrp_to_drops` / `hex_to_ascii` / `path_find_*`), `parse.rs` (JSON-RPC response parsers, book helpers), `util.rs` (JSON path helpers + reconnect backoff), `types.rs` (row data types, `BookPair`, `PollContext` / `PollCommand`), `poll.rs` (periodic polling, `PollCommand` dispatch), `submit.rs` (wallet submit path: mainnet guard → simulate → sign → submit, the various `submit_*`, `simulate_tx_requiring_tes_success`, Payment quote), `ws.rs` (WebSocket), `cli_exec.rs` (non-TUI `execute_cli_command` / `execute_rp_lookup`), `toml.rs` (`xrp-ledger.toml` parser).
- `cli.rs`: command-line arguments and subcommand definitions (`Cli`), and `RpCli` for `rp`.
- `action.rs`: the `Action` passed around inside the app.
- `config.rs`: defaults + config-file load and merge, and applying `XRPL_*` environment variables (seed, RPC/WS, network).
- `components/mod.rs`: common UI component trait and submodule aggregation.
- `tui.rs`: shared TUI infrastructure (rendering, events, terminal management).
- `logging.rs`: log initialization.
- `errors.rs`: installs the `color-eyre` panic/eyre hooks (with TUI-exit cleanup).
- `network.rs`: the `Network` enum (mainnet / testnet / devnet / xahau / xahau-test), endpoint definitions, and session switching via `next_network()`.
- `signing.rs`: `SigningCredential` (family seed / BIP39 mnemonic handling), `prompt_production_confirmation`, and for Payment `create_and_sign_payment` (builds the blob to submit) / `build_payment_tx_json_for_simulate`.
- `test_support.rs`: shared test helpers (`TempRootGuard` / `env_lock` / `env_lock_async` / `TestEnvGuard`; `#[cfg(test)]` only).
- `../tests/cli_smoke.rs`: binary-boundary smoke test (verifies argv, exit code, and output of the `lazyxrp` / `rp` executables against a local JSON-RPC server, TC-158).

**Dependency direction (import rules):**
- `components/` may import `action`, `config` (`Arc<Config>`), `network`, row types / pure helpers of `xrpl` (`xrpl::sanitize_text` / `xrp_to_drops` / `hex_to_ascii` / `txspec`), pure validation/encoding functions of `signing` (`validate_*` / `parse_*` / `encode_fxrp_*`), and types and pure helpers of `flare` (`format_token_units` / `classify_mint` / `check_operator`, etc.). It does not import `xrpl/client`, `xrpl/poll`, `xrpl/submit`, or `app` (it holds no `RpcClient`; I/O goes through `Action`).
- `xrpl/` may import the internal modules `action`, `network`, `signing`, `config`, `cli`, `flare`, and `coverage` (from `cli_exec.rs` / `poll.rs`). It does not import `components/` or `app`. `xrpl/types.rs` is a leaf with no dependency on UI / poll.
- `coverage.rs` has a reverse dependency: it imports `components::shared::tx_detail` (parser registration) and `xrpl::txspec` (registry). Because `xrpl/cli_exec.rs` calls `coverage`, a path `xrpl/` → `coverage` → `components` exists (read-only registration info only).
- `config.rs` imports `Action` / `Mode` from `action` (for keybinding deserialization). `Mode` lives in `action.rs`.
- `signing.rs` depends on `network`, `secrecy`, the XRPL SDK crate, and pure helpers of `xrpl` (`xrp_to_drops` / `resolve_payment_destination` / `GeneratedWalletKeys`); it does not import `components/` or `app`.
- A write for a new TX type reuses the shared pipeline (resolve / simulate / sign / submit in `xrpl/submit.rs`, handlers in `poll.rs`); do not write per-type simulate or signing. Declarative types go in `xrpl/txspec/` (architecture.md §2.1).
- When adding a panel, update `app::TAB_TITLES` in the same change (keep tab index and panel order consistent, security.md I-9).
- Tightly coupled spots: `action.rs` (hub for all send/receive), `submit_*` in `poll.rs` (per-type validation + shared simulate/sign/submit), `tx_detail/parsers.rs` (registered TX types, architecture.md "TX detail overlay").

## 4. Responsibilities under `src/components/panels/`

- `server.rs`: server status panel.
- `account.rs`: account info panel.
- `book.rs`: order book panel.
- `path_find.rs`: `ripple_path_find` route list (send amount, hops, path; cheapest first).
- `amm.rs`: AMM pool detail panel.
- `oracle.rs` / `flare_ftso.rs` / `fxrp_direct_mint.rs` / `flare_wallet.rs` / `combined_oracle.rs`: XRPL oracle aggregation, Flare FTSOv2, FXRP Direct Mint read (+ Smart Account row), and the combined Overview view.

- `trust_lines.rs`: TrustLine list panel (Table + Scrollbar, colored by balance).
- `tx_history.rs`: TX history panel (Table + Scrollbar, colored by tesSUCCESS).
- `wallet.rs`: summary of the seed-derived account + composer (transaction list is in `tx_history.rs`).
- `ledger_objects.rs`: `account_objects` list (each panel filters by type).

## 5. Responsibilities under `src/components/tabs/`

- `overview.rs`: Tab 0 — Server + Combined Oracle/FTSO/FXRP + Flare wallet read panel.
- `account_wallet.rs`: Tab 1 — Wallet (top) + TxHistory (bottom). Account panel when no seed is set.
- `market_oracle.rs`: Tab 2 — Book / Path-Find / AMM / Trust lines / Flare FTSO / XRPL Oracle.
- `assets.rs`: Tab 3 — NFT + ledger objects (including PayChannel / Escrow).
- `nft.rs`: NFT subview used by `AssetsTab`.

## 6. Responsibilities under `src/components/shared/`

- `fmt.rs`: number formatting utilities (`group_digits` / `group_digits_u64`, `fmt_xrp` / `fmt_drops`, time conversion).
- `fps.rs`: frame-rate display component.
- `help_overlay.rs`: keybinding help overlay toggled with the `?` key.
- `selectable_table.rs`: common helpers for table row selection and scrolling.
- `splash.rs`: startup splash component.
- `status_bar.rs`: one-line status bar at the bottom of the screen.
- `theme.rs`: common theme and color definitions (`SECONDARY` in addition to `ACCENT`, used to distinguish e.g. hash columns).
- `tx_detail/`: transaction detail overlay (`TxDetailState` + `render_tx_detail`) — parses every XRPL transaction type and shows it as a popup.
  - `mod.rs`: state management (`TxDetailState`) and rendering (`render_tx_detail`, `detail_lines_for`).
  - `format.rs`: common formatting functions (`fmt_xrpl_amount`, `push_common_lines`, `format_value`, `fmt_currency`). URI hex uses `xrpl::hex_to_ascii`.
  - `parsers.rs`: the `*_detail_lines` functions that parse 29 XRPL transaction types.
- `widgets.rs`: common UI helpers (`titled_block`, `titled_block_with_count`, `tx_table`, `spinner`). The history table is built on update and reused via `render_selectable_table`.

## 7. Responsibilities under `docs/`

The index of documents (the question each doc answers) has [`AGENTS.md`](../AGENTS.md) as its SSOT. It is not duplicated here.

## 8. Build / test commands and entry points

| Command | Purpose |
|---------|---------|
| `cargo check` | Minimum verification after code changes |
| `cargo fmt` | Format code |
| `cargo test` | Run all tests |
| `cargo build --release` | Release build |
| `./install.sh` or `mise run install` | Install binary |
| `mise run bench` | Full benchmark suite (~10 min) |
| `mise run bench-fast` | Quick benchmarks |
| `mise run bench-ci` | CI-equivalent benchmark (`benchmark.sh --json --fast --perf-only`) |

| Entry | File | Description |
|-------|------|-------------|
| `main()` | `src/main.rs` | Thin `lazyxrp` binary → `lazyxrp::run()`. |
| `main()` | `src/bin/rp.rs` | Thin `rp` binary → `lazyxrp::run_rp()` (lookup-only). |
| `run()` / `run_rp()` | `src/lib.rs` | Shared entry: TUI/CLI vs tx/account lookup. |
| `Cli` / `RpCli` | `src/cli.rs` | Clap-derived CLIs. `Cli`: TUI flags + `-x` script mode. |
| `App::run()` | `src/app.rs` | TUI main loop: event handling → action processing → dirty-flagged render (`needs_draw`). |
| `execute_cli_command()` / `execute_rp_lookup()` | `src/xrpl/cli_exec.rs` | Non-TUI CLI dispatchers. |
| `Config::new()` | `src/config.rs` | Config loading: built-in defaults → user config.toml → env vars. |
| `build.rs` | `build.rs` | Build-time metadata (vergen-gix for commit hash, date). |
