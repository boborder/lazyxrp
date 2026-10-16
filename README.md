# lazyxrp

[![crates.io](https://img.shields.io/crates/v/lazyxrp.svg)](https://crates.io/crates/lazyxrp)
[![CI](https://github.com/boborder/lazyxrp/actions/workflows/ci.yml/badge.svg)](https://github.com/boborder/lazyxrp/actions/workflows/ci.yml)
[![MSRV](https://img.shields.io/crates/msrv/lazyxrp.svg)](Cargo.toml)
[![License: MIT](https://img.shields.io/crates/l/lazyxrp.svg)](LICENSE)

Terminal UI and script CLI for the XRP Ledger — watch accounts, order books, NFTs, trust lines and AMM pools; sign payments and DEX offers (simulate-first, mainnet guarded).

Repository: https://github.com/boborder/lazyxrp · Changes: [`CHANGELOG.md`](CHANGELOG.md) · Security: [`SECURITY.md`](SECURITY.md)

## Features

Watch mode uses **four top-level tabs** (jump with `1`–`4`):

| Tab | Content |
|-----|---------|
| **Overview** | Server summary (left) + XRPL oracle + Flare FTSOv2 + FXRP Direct Mint read + Flare Smart Account read (right) |
| **Account** | Wallet composer (incl. Flare Smart Account FXRP transfer / redeem) + account summary + recent transaction history |
| **Market** | DEX book + Path-Find routes + AMM + trust lines + XRPL oracle + Flare FTSOv2 |
| **Assets** | NFTs + selected-NFT image preview + account_objects (Objects / Pay channels / Escrows) |

**Write coverage:** **50 / 68** XRPL transaction types can be signed from the TUI and/or `-x` (hand-written: Payment, AccountSet, TrustSet, OfferCreate / Cancel, SetRegularKey, Escrow ×3, AMMDeposit / Withdraw, AccountDelete; declarative specs: NFToken ×5, Check ×3, TicketCreate, DepositPreauth, SignerListSet, Clawback, PaymentChannel ×3, AMMCreate / Vote / Bid / Delete, MPToken ×4, Credential ×3, DID ×2, Oracle ×2, PermissionedDomain ×2, XChain ×6 via the Wallet composer **Transaction** (`m`) and `-x tx`). The full per-type matrix (read / TUI / CLI / planned / unsupported) is the "XRPL coverage" table in [`ROADMAP.md`](ROADMAP.md); `lazyxrp -x coverage` prints it.

## Requirements

- Rust **1.94.1+** (MSRV from `Cargo.toml` `rust-version`)
- Daily builds use the **`stable`** channel from [`rust-toolchain.toml`](rust-toolchain.toml)
- Edition 2024
- Release artifacts (`.github/workflows/cd.yml`): macOS (`x86_64`, `aarch64`), Linux (`x86_64`, `aarch64`, `i686`), Windows (`x86_64`). Verified on macOS arm64.

## Install

```bash
# Latest release (default: ~/.local/bin)
curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/boborder/lazyxrp/main/install.sh | bash

# Authenticated install if GitHub API rate-limits anonymous calls
# curl -fsSL ... | env GITHUB_TOKEN=ghp_... bash

# Custom install directory
curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/boborder/lazyxrp/main/install.sh | INSTALL_DIR=/usr/local/bin bash

# crates.io
cargo install lazyxrp
# Prefer reproducible builds when Cargo.lock is published:
# cargo install lazyxrp --locked

# From this repo (clone first; run from repo root)
cargo install --path .
```

### Uninstall

`./install.sh` does not remove files for you. Prefer the binary’s self-uninstall when available:

```bash
lazyxrp --self-uninstall          # prompts for confirmation
lazyxrp --self-uninstall --yes    # skip confirmation
```

That removes the current binary, `{name}.bak`, and the resolved config/data directories (same path rules as `Config::new`, including `LAZYXRP_*` overrides).

Manual alternatives:

```bash
rm -f ~/.local/bin/lazyxrp ~/.local/bin/lazyxrp.bak ~/.local/bin/rp
cargo uninstall lazyxrp --root "$HOME/.local"   # install.sh → ~/.local
cargo uninstall lazyxrp                         # default cargo prefix / --path .

# Config / data only (when the binary is already gone; no LAZYXRP_* overrides)
rm -rf ~/.config/lazyxrp ~/.local/share/com.kdheepak.lazyxrp   # Linux / typical XDG
# macOS:
# rm -rf "$HOME/Library/Application Support/lazyxrp" \
#        "$HOME/Library/Application Support/com.kdheepak.lazyxrp"
```

`cargo uninstall` also clears Cargo’s install metadata for that prefix; `--self-uninstall` only deletes files and directories. See `./install.sh --uninstall-help` for the same reference.

## Quick Start

**TUI (default):** `lazyxrp` launches the dashboard. No subcommand required.

```bash
lazyxrp --account <r-address>
lazyxrp --network testnet --account <r-address>
```

TUI flags: `--tick-rate`, `--account`, `--server`, `--ws-server`, `--network`, `--yes`, `--seed`, `--mnemonic`, `--allow-insecure-rpc`, `--watch`.

`--watch` is read-only: no signing key is loaded (env, config, `--seed` / `--mnemonic`, nor `FLARE_EVM_KEY`). The TUI shows the account panel for the key's address (or `--account`) without the composer; `-x` writes are refused.

## Script CLI (`-x` / `rc`)

Non-interactive subcommands use **`-x`** (or shell alias **`rc`**):

```bash
# Recommended
lazyxrp -x info
lazyxrp --network testnet -x account <r-address>
lazyxrp -x send <r-destination> --amount 10   # requires XRPL_SEED
lazyxrp --network testnet -x faucet           # fresh testnet wallet (seed printed once, not saved)

# Optional shell alias (not installed by install.sh)
alias rc='lazyxrp -x'
rc info
rc book --base XRP --quote USD --issuer <r-issuer>
```

| Subcommand | Purpose |
|------------|---------|
| `info` | Server info |
| `account <ADDR>` | Account info |
| `account-status <ADDR>` | Activated if reserve met (≥ 10 XRP) |
| `book --base … --quote …` | Order book |
| `summary [--account ADDR]` | Combined summary |
| `nfts <ADDR>` | NFT list |
| `lines <ADDR>` | Trust lines |
| `amm --asset1 … --asset2 …` | AMM pool info |
| `tx-history <ADDR> [--limit N]` | Transaction history |
| `send <DEST> [--amount AMT]` | Send XRP (requires `XRPL_SEED` or `XRPL_MNEMONIC`) |
| `offers <ADDR>` | Open DEX offers (`seq`, TakerGets, TakerPays; all pages) |
| `offer-cancel <SEQ>` | Cancel your offer `SEQ` (requires signing key; simulate first; `--yes` on mainnet/xahau) |
| `offer-create <GETS> <PAYS>` | Place a DEX offer; specs `XRP:drops` or `CUR:issuer:value` (requires signing key; simulate first; `--yes` on mainnet/xahau) |
| `amm-deposit <AMOUNT> <AMOUNT2>` | Two-asset AMM deposit; both specs are maxima and name the pool (requires signing key; simulate first; `--yes` on mainnet/xahau) |
| `amm-withdraw <ASSET> <ASSET2> [--lp N]` | AMM withdraw; assets `XRP` / `CUR:issuer`; without `--lp` withdraws all (requires signing key; simulate first; `--yes` on mainnet/xahau) |
| `account-delete <DEST>` | Delete the signing account and send the remaining XRP to `DEST` (X-address for a tag); irreversible, fee = owner reserve; the account must be ≥ 256 ledgers old and own no objects (requires signing key; simulate first; `--yes` on mainnet/xahau) |
| `tx <TYPE> [Field=value …]` | Any spec'd transaction type (see `-x coverage`, Status `Spec`), e.g. `tx NFTokenBurn NFTokenID=<64 hex>`; field names are case-insensitive (requires signing key; simulate first; `--yes` on mainnet/xahau) |
| `nft-mint [--uri TEXT] [--taxon N] [--flags burnable,onlyXRP,transferable] [--transfer-fee 0..50000] [--issuer ADDR]` | NFTokenMint; `--uri` is plain text (stored hex, ≤ 256 bytes); a non-zero fee needs `transferable` (requires signing key; simulate first; `--yes` on mainnet/xahau) |
| `nft-burn <NFTOKENID> [--owner ADDR]` | NFTokenBurn; irreversible (requires signing key; simulate first; `--yes` on mainnet/xahau) |
| `coverage` | Print the XRPL transaction coverage table (Markdown; offline) |
| `faucet [ADDR]` | testnet/devnet only: fund `ADDR`, or generate a wallet locally and fund it (seed printed once; only the address is sent) |

Writes (`send`, `offer-create`, `offer-cancel`, `amm-deposit`, `amm-withdraw`, `account-delete`, `tx`, `nft-mint`, `nft-burn`) wait until the transaction is validated and exit non-zero unless the final result is `tes…` (`--no-wait` returns right after submit). Before signing, the endpoint's `network_id` must match `--network`: a mainnet `XRPL_RPC_SERVER` under `--network testnet` is refused.

**Migrating to 0.3.0:** bare `lazyxrp info` (without `-x`) and `lazyxrp watch` were removed; the bare form exits with an error pointing at `lazyxrp -x info` (use `lazyxrp` with no arguments for the TUI). `send` simulates before signing (same as the TUI), needs `--yes` on mainnet/xahau or an interactive confirmation, and exits non-zero when signing or submission fails.

### Short command: `rp`

`rp` is a second binary (same crate). `cargo install lazyxrp`, release archives, and `install.sh` all provide it. Lookup-only CLI (not the TUI):

```bash
rp -t <txid|r-address>
rp <txid|r-address>
rp --network testnet -t <txid|r-address>
```

`install.sh` falls back to a `rp` → `lazyxrp` symlink if an older archive has no `rp` binary. When `INSTALL_DIR` is missing from `PATH`, the installer can append it to your shell profile (interactive prompt; auto under `CI=1` / `-q`).

## Key Bindings

Global defaults: [`config.json5`](config.json5) (merged into `~/.config/lazyxrp/config.toml`). **Full keymap** (global + panel-local, composer, TX overlay): [`DESIGN.md`](DESIGN.md).

Highlights: `Tab` / `1`–`4` tabs · `h`/`l` focus · `j`/`k` rows · `Enter` TX detail · `t` composer · `Ctrl-n` cycle network (session) · `?` help · `q` quit.

## Network Selection

```
Priority: --network flag > XRPL_NETWORK env > config.toml > mainnet (default)
TUI session: Ctrl-n cycles network (not persisted); custom --server / XRPL_RPC_SERVER kept
```

| Network | RPC | WS |
|---------|-----|----|
| mainnet | `https://xrplcluster.com` | `wss://xrplcluster.com` |
| testnet | `https://s.altnet.rippletest.net:51234` | `wss://s.altnet.rippletest.net:51233` |
| devnet  | `https://s.devnet.rippletest.net:51234` | `wss://s.devnet.rippletest.net:51233` |
| xahau | `https://xahau.network` | `wss://xahau.network` |
| xahau-test | `https://xahau-test.net` | `wss://xahau-test.net` |

Flare panels: `[flare] display = full | compact | off` in config (see [`docs/architecture.md`](docs/architecture.md) §6.7).

Flare Smart Accounts: the Overview FXRP panel shows your XRPL address's Flare personal account (FXRP balance, nonce, operator, instruction fees); Wallet composer `a` sends an FXRP transfer / redeem instruction as an XRPL Payment to the operator. Writes are signed only when `getXrplProviderWallets()[0]` equals the operator you pin out of band in `[flare.smart_accounts] expected_operator`.

## Configuration

Config lookup follows the XDG Base Directory spec:

- `$XDG_CONFIG_HOME/lazyxrp/config.toml`
- `~/.config/lazyxrp/config.toml` (fallback)

```bash
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/lazyxrp"
# Optional sample overrides from the repo:
cp .config/lazyxrp/config.toml "${XDG_CONFIG_HOME:-$HOME/.config}/lazyxrp/config.toml"
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `XRPL_NETWORK` | Network preset (`mainnet` / `testnet` / `devnet` / `xahau` / `xahau-test`) |
| `XRPL_RPC_SERVER` | Custom RPC endpoint (overrides network preset) |
| `XRPL_WS_SERVER` | Custom WS endpoint (overrides network preset) |
| `XRPL_SEED` | Signing seed for write TX (prefer over config file) |
| `XRPL_MNEMONIC` | BIP39 mnemonic (secp256k1 index 0; mutually exclusive with `XRPL_SEED`) |
| `FLARE_RPC_URL` | Flare FTSOv2 RPC (default: Flare mainnet). `https://` only unless `--allow-insecure-rpc`; never used for signing over `http://` |
| `FLARE_FEEDS` | Flare feeds for Overview / Market (e.g. `FXRP/USD,FLR/USD`) |
| `FLARE_FEED` | Legacy single-feed override |
| `FLARE_EVM_KEY` | Flare executor key for FXRP C3 (`[flare.fassets] execute=true` only) |
| `LAZYXRP_CONFIG` | Override config directory (`..` rejected) |
| `LAZYXRP_DATA` | Override data directory |
| `LAZYXRP_LOG_LEVEL` | Default file log filter (`tracing` `EnvFilter`) |

> **Security:** `--seed` / `--mnemonic` appear in process listings (`ps`) and shell history. Prefer `XRPL_SEED` / `XRPL_MNEMONIC`. Copy `.env.example` to a local `.env` only — **never commit** `.env` (gitignored).

## Development

From the repo root, use [`mise`](https://mise.jdx.dev/) (`mise run install` runs `./install.sh -q`) or plain Cargo:

```bash
cargo fmt
cargo check
cargo test
cargo run --bin lazyxrp -- --account <r-address>
cargo run --bin lazyxrp -- --network testnet -x info

# Optional Flare FTSOv2 overrides (Overview / Market)
# FLARE_FEEDS=FXRP/USD,FLR/USD,BTC/USD,ETH/USD
# FLARE_RPC_URL=https://flare-api.flare.network/ext/C/rpc
```

CI (`.github/workflows/ci.yml`):

```bash
cargo test --locked --all-features --workspace
cargo fmt --all --check
cargo clippy --locked --all-targets --all-features --workspace -- -D warnings
cargo doc --locked --no-deps --document-private-items --all-features --workspace --examples
cargo audit --ignore RUSTSEC-2026-0235
cargo deny check
```


## Contributing

See [`AGENTS.md`](./AGENTS.md).

## Documentation

Product docs live under [`docs/`](./docs/). Reading order: [`AGENTS.md`](./AGENTS.md).

Planning: [`ROADMAP.md`](./ROADMAP.md).

## References

- [XRPL Rust SDK](https://github.com/XRPLF/xrpl-rust)
- [XRPL Documentation](https://xrpl.org/docs)

## License

MIT — see [`LICENSE`](./LICENSE).