# tech.md

[日本語](tech-ja.md)

**Role:** Tech stack and API integration (dependency crates, environment variables, XRPL/Flare RPC surface).

## 1. Language / runtime

- Language: Rust
- Edition: 2024
- Toolchain: `channel = "stable"` in `rust-toolchain.toml` (components: `rustfmt`, `clippy`). The MSRV equivalent is `rust-version = "1.94.1"` in `Cargo.toml` (tracks the MSRV of alloy 2.4.2; a reference value, since real builds force stable).
- Async runtime: Tokio (`1` in `Cargo.toml`, `features = ["macros", "rt-multi-thread", "sync", "time", "net", "io-util"]`; `full` is not used)
- Target OS: release targets are in [`.github/workflows/cd.yml`](../.github/workflows/cd.yml) (cited by the README Requirements). Currently confirmed on macOS arm64.

## 2. Main libraries

### UI / CLI

- `ratatui` (`0.30` in `Cargo.toml`, `0.30.2` in `Cargo.lock`; `serde`, `macros`)
- `crossterm` (`0.29` in `Cargo.toml`, `0.29.0` in `Cargo.lock`; `serde`, `event-stream`)
- `clap` (`4` in `Cargo.toml`, `4.6.7` in `Cargo.lock`; `derive`, `cargo`, `wrap_help`, `unicode`, `string`)
- `ratatui-image` (`11` in `Cargo.toml`, `default-features = false`; `crossterm`. `image-defaults` is not used because it pulls in `image/default` (the AVIF encoder `ravif`, rayon, bmp/tiff, etc.) and slows the build) — Kitty / Sixel / iTerm2 / halfblocks image preview.
- `image` (`0.25` in `Cargo.toml`, `default-features = false`; PNG/JPEG/GIF/WebP only) — NFT image decode.

### XRPL / networking
- `reqwest` (`0.13` in `Cargo.toml`, direct dependency `0.13.5` in `Cargo.lock`; `default-features = false`, `features = ["json", "stream", "native-tls", "charset", "http2", "system-proxy"]` — JSON-RPC and bounded NFT metadata/image streaming). TLS is **native-tls**, not the default rustls/`aws-lc-rs` (the C build of `aws-lc-sys` alone dominated a clean build; `reqwest 0.12.x` on the `xrpl-rust` path is also native-tls). On the `xrpl-rust` path, **`reqwest 0.12.x` may coexist in the lock** (`Cargo.lock` is authoritative for resolution).
- `xrpl-rust` (`1.1` in `Cargo.toml`, `1.3.0` in `Cargo.lock`)
- `kobe-xrpl = 3.4` / `kobe-primitives = 3.4` — BIP39 secp256k1 mnemonic → BIP44 `m/44'/144'/0'/0/0` account derivation.
- `tokio-tungstenite` — dev-dependency only (local WS server for `ws.rs` tests). The WS in the main binary goes through xrpl-rust
- URL parsing uses `reqwest::Url` (a re-export of the `url` crate; there is no direct dependency)
- `base64 = 0.23` — XRPLF dUNL blob / validator manifest decode (`base64::engine::general_purpose::STANDARD`)
- `hex = 0.4` — hex encode/decode of seed / domain / validator key
- The calendar conversion from Ripple epoch → `format_ripple_time_utc` is `civil_from_days` in `xrpl/format.rs` (no dependency; the old `time` crate was removed)
- Payment signing uses the crate's public API `wallet::Wallet` + `transaction::sign` (`models::transactions::payment::Payment`), and at submit time converts to a `tx_blob` via `binarycodec::encode`. An existing XRPL family seed and a BIP39 mnemonic (secp256k1 only) are unified into `SigningCredential`.
### Flare / EVM

- `alloy` (`2` in `Cargo.toml`; `default-features = false`, `features = ["std", "reqwest", "reqwest-native-tls", "essentials"]` — HTTP provider + local signer. `full` is not used. The default `reqwest-rustls-tls` is avoided because it pulls in `aws-lc-sys`)
  - Resolves the `FtsoV2` / `AssetManagerFXRP` addresses via `ContractRegistry`.
  - FTSOv2: `getFeedsById(bytes21[])` fetches all feeds in one round trip (`fetch_from_rpc`). On a batch revert, retry with the `calculateFeeByIds` fee (eth_call only, no fund movement) → fall back to sequential `getFeedById`. The `FXRP/USD` setting is normalized to the `XRP/USD` feed.
  - FXRP Direct Mint: the `directMintingPaymentAddress()` / `getDirectMinting*Fee*` views are read in parallel with `try_join!` (`fetch_fxrp_direct_mint_info`).
  - Flare wallet read: `eth_getBalance` + `IAssetManager.fAsset()` → `IERC20.balanceOf` / `decimals` (`fetch_flare_wallet_balance`).
  - RPC resolution: `FLARE_RPC_URL` env > `[flare] network` preset (`flare` / `songbird` / `coston2`). Implementation: `app.rs::resolve_flare_rpc_url`. https enforced and http signing rejected: `lib.rs::check_flare_endpoint` (same rule as XRPL endpoints).

### Config / serialization

- `toml` (the user `config.toml` is read with `std::fs` + `toml::from_str`; the old `config` crate was removed in 0.6.1)
- `directories` (`6` in `Cargo.toml`, `6.0.0` in `Cargo.lock` — `ProjectDirs`)
- `serde` (`1` in `Cargo.toml`, `1.0.229` in `Cargo.lock`, `derive`)
- `serde_json` (`1` in `Cargo.toml`, `1.0.151` in `Cargo.lock`)
- `json5` (`1` in `Cargo.toml`, direct dependency `1.3.1` in `Cargo.lock` — parses the embedded `config.json5` at the repository root)

### Observability / failure handling

- `tracing` (`0.1` in `Cargo.toml`, `0.1.44` in `Cargo.lock`)
- `tracing-subscriber` (`0.3` in `Cargo.toml`, `0.3.23` in `Cargo.lock`; `env-filter`)
- `color-eyre` (`0.6` in `Cargo.toml`, `0.6.5` in `Cargo.lock`)

### Security

- `secrecy = 0.10.x` (`features = ["serde"]`)
  - Wraps signing credentials (family seed / BIP39 mnemonic) in `SecretString` (= `SecretBox<str>`), masks the secret in `Debug`/`Display`, and zeroizes on drop.
  - Load priority: CLI (`--seed` / `--mnemonic`) > the matching env var (`XRPL_SEED` / `XRPL_MNEMONIC`) > `seed` / `mnemonic` in `config.toml [xrpl.signing]`.
  - Writing a secret into `config.toml` risks plaintext storage on disk, so an env var is recommended.

### Dependency resolution notes (link stability)

- `critical-section = 1.2.0` (`features = ["std"]`)
  - Provides, via `std`, the critical-section implementation that becomes necessary through the `embassy-sync` family.

## 3. Build settings

- Build script: `build.rs`
- build-dependencies:
  - `anyhow` (`1` in `Cargo.toml`, `1.0.104` in `Cargo.lock`)
  - `vergen-gix` (`9` in `Cargo.toml`, `9.1.0` in `Cargo.lock`; `build`)
- Release profile:
  - `codegen-units = 1`
  - `lto = "fat"`
  - `opt-level = "s"`
  - `strip = true`
  - `panic` is unwind (for color-eyre's panic report)

## 4. Environment variables

| Variable | Description |
|---|---|
| `XRPL_NETWORK` | Network preset (`mainnet` / `testnet` / `devnet` / `xahau` / `xahau-test`) |
| `XRPL_RPC_SERVER` | Custom RPC endpoint (overrides the network preset) |
| `XRPL_WS_SERVER` | Custom WS endpoint (overrides the network preset) |
| `XRPL_SEED` | XRPL family seed (for signing) |
| `XRPL_MNEMONIC` | BIP39 mnemonic (secp256k1, for signing) |
| `FLARE_RPC_URL` | Flare RPC URL (takes precedence over the `[flare] network` preset) |
| `FLARE_FEEDS` | Flare feed list for Overview / Market (comma-separated) |
| `FLARE_FEED` | Legacy single-feed setting (only when `FLARE_FEEDS` is unset) |
| `FLARE_EVM_KEY` | Flare EVM key (only when `[flare.fassets] execute=true`; the env name is `[flare.fassets] evm_key_env`): C3 `executeDirectMinting` (composer `7`, `-x fxrp-execute`), `fxrp-redeem`, `fxrp-tag-*` |
| `LAZYXRP_CONFIG` | Explicit override of the config directory (`..` is rejected) |
| `LAZYXRP_DATA` | Explicit override of the data directory |
| `LAZYXRP_LOG_LEVEL` | Default filter for the file log (`EnvFilter` of `tracing-subscriber`) |

Startup order is `Config::new()` → `logging::init(config.resolved_data_dir())`. `Config::new()` reads `XRPL_SEED` / `XRPL_MNEMONIC`, then `XRPL_NETWORK` / `XRPL_RPC_SERVER` / `XRPL_WS_SERVER`, and applies them to `xrpl` with precedence over the same items in the config file. If both signing credentials are set, startup fails with an error.

```
--network CLI > XRPL_NETWORK > config.toml [xrpl] network > default (mainnet)
--server  CLI > XRPL_RPC_SERVER > config.toml rpc_server  > Network::rpc_url()
FLARE_RPC_URL env > config.toml [flare] network preset > Flare mainnet default
FLARE_FEEDS env > FLARE_FEED env > DEFAULT_FLARE_FEEDS (`FXRP/USD`, `FLR/USD`, `BTC/USD`, `ETH/USD`)
```

Flare keys in `config.toml` (details in [`architecture.md`](architecture.md) §6):

| Key | Values | Notes |
|-----|--------|-------|
| `[flare] network` | `flare` / `songbird` / `coston2` | RPC preset for FTSO / FXRP / wallet read |
| `[flare] display` | `full` / `compact` / `off` | Density of the Flare panels in Overview / Market |
| `[flare.wallet] address` | `0x…` EVM | Overview read-only panel (invalid hex fails startup) |
| `[flare.fassets] execute` | `true` / `false` | Gate for C3 `executeDirectMinting` (default `false`) |
| `[flare.fassets] expected_core_vault` | XRPL `r…` | Core Vault destination pin for C2 Direct Mint. Signing is refused if it does not match the value returned by the Flare RPC (unset is also refused). Set a value confirmed through another channel |
| `[flare.smart_accounts] expected_operator` | XRPL `r…` | Operator destination pin for the Smart Account instruction Payment. Signing is refused if it does not match `getXrplProviderWallets()[0]` from the Flare RPC (unset is also refused). Set a value confirmed through another channel |

`<Ctrl-n>` inside a TUI session cycles the XRPL network via `Network::next_network()` (not persisted to config). `xahau` is subject to the same write guard (`--yes`) as mainnet.

**Long-running operation:** the default `poll_interval_ms = 5000` is meant for interactive use. When hitting a public RPC (`xrplcluster.com` etc.) 24/7, `15000`–`30000` is recommended ([`architecture.md`](architecture.md) §3). The dUNL has a 10-minute TTL cache, and Flare registry resolution is cached in-process.

## 5. Development commands

- Resolve dependencies: `cargo build`
- Task runner: root `mise.toml` (`mise run <task>`). `check` / `fmt` / `lint` (fmt+clippy is the CI baseline) / `test` / `test-serial` / `e2e-testnet` (opt-in live writes, [`test.md`](test.md)) / `doc` / `audit` (cargo-audit + cargo-deny — same as CI; ignore lists: `deny.toml` for cargo-deny, `.cargo/audit.toml` for cargo-audit) / `update` (re-runs check + audit after `cargo update`) / `clean` (removes target/ and benchmark output) / `bench*` / `release` / `tag-push` ([`maintenance.md`](maintenance.md)). The tools themselves are under `[tools]` (rust uses the same channel `stable` as [`rust-toolchain.toml`](../rust-toolchain.toml), `hyperfine`, `cargo-audit`, `cargo-deny`, `cargo-bloat`).
- Compile check: `cargo check`
- Local install (optional): root `./install.sh` (**either `curl` or `wget` is required for HTTP fetches**). English prompts. See options with `--help` (`--method cargo|binary`, `--install-rust` / `--no-install-rust`, etc.). Source builds run from the root of a cloned tree (the same level as `Cargo.toml` / `rust-toolchain.toml`). With `curl | bash` alone, the default toolchain installed via rustup matches **the `channel` of the repo's `rust-toolchain.toml` whenever it can be read** (falls back to **`stable`** on the plain stdin path where it cannot). The GitHub Releases REST API can be **rate-limited depending on the environment when unauthenticated against the public API**, so optionally set **`GITHUB_TOKEN` / `GITHUB_API_TOKEN`**. Binary placement **copies to a `*.partial.*` file in INSTALL_DIR and then `mv`s (cleanup on failure or interruption is the EXIT `cleanup`).** **Manual uninstall** is `./install.sh --uninstall-help` (`lazyxrp --self-uninstall` / `--yes` **from the binary on PATH** also works. Besides the binary / `cargo uninstall`, it shows optional examples of deleting the user config and data directories, separately for Linux/macOS, and notes on `LAZYXRP_CONFIG` / `LAZYXRP_DATA` and the `data_dir` / `config_dir` overrides in `config.toml`, as in the README Uninstall). A TTY gets interactive mode + animation; `-q` or a non-TTY is non-interactive. Downloads prefer `curl` (falling back to `wget`) with retry/timeout. GitHub tag / commit SHA resolution **prefers `jq` if present** (otherwise the original grep/sed). The `BINARY_INSTALL=1 ./install.sh -q` prebuilt-first example is unchanged. Or **[mise](https://mise.jdx.dev/)** `mise run install` (via the task in `mise.toml`; see `README.md` for details)
- Run (TUI): `cargo run --bin lazyxrp -- --account <r-address>`
- Run with a seed / mnemonic (discouraged — exposed in argv/history; prefer `XRPL_SEED` / `XRPL_MNEMONIC` or `config.toml`): `cargo run --bin lazyxrp -- --account <r-address> --mnemonic "abandon ... about"`
- Scripted CLI (examples):
  - `cargo run --bin lazyxrp -- -x info`
  - `cargo run --bin lazyxrp -- -x account <r-address>`
  - `cargo run --bin lazyxrp -- -x summary --account <r-address>`
  - `cargo run --bin lazyxrp -- -x book --base XRP --quote USD --issuer <r-issuer> --limit 5`
  - `cargo run --bin lazyxrp -- -x nfts <r-address>`
  - `cargo run --bin lazyxrp -- -x lines <r-address>`
  - `cargo run --bin lazyxrp -- -x amm --asset1 XRP --asset2 USD --issuer2 <r-issuer>`
  - `cargo run --bin lazyxrp -- -x tx-history <r-address> --limit 20`
- Network selection examples:
  - `cargo run --bin lazyxrp -- --network testnet --account <r-address>`
  - `XRPL_NETWORK=testnet cargo run --bin lazyxrp -- -x info`

## 6. Technical constraints and notes

- Around `tokio::spawn`, there are cases that hit a known Rust limitation (issue #100013).
- The current implementation prioritizes build stability by adjusting how the XRPL background tasks are started.
- On macOS, some dependencies may produce a link warning about "object built for a newer macOS version"; triage any fatal error separately.
