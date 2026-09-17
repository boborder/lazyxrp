# lazyxrp

Terminal UI for the XRP Ledger — monitor accounts, order books, NFTs, trust lines, and AMM pools.

Repository: https://github.com/boborder/lazyxrp

## Features

Watch mode uses **four top-level tabs** (jump with `1`–`4`):

| Tab | Content |
|-----|---------|
| **Overview** | Server summary (left) + XRPL oracle + Flare FTSOv2 + FXRP Direct Mint read (right) |
| **Account** | Wallet composer + account summary + recent transaction history |
| **Market** | DEX book + Path-Find routes + AMM + trust lines + XRPL oracle + Flare FTSOv2 |
| **Assets** | NFTs + selected-NFT image preview + account_objects (Objects / Pay channels / Escrows) |

## Requirements

- Rust **1.94.1+** (MSRV from `Cargo.toml` `rust-version`)
- Daily builds use the **`stable`** channel from [`rust-toolchain.toml`](rust-toolchain.toml)
- Edition 2024
- Release artifacts (`.github/workflows/cd.yml`): macOS (`x86_64`, `aarch64`), Linux (`x86_64`, `aarch64`, `i686`), Windows (`x86_64`). Verified on macOS arm64.

## Install

```bash
# Latest release (default: ~/.local/bin)
curl -fsSL https://raw.githubusercontent.com/boborder/lazyxrp/main/install.sh | bash

# Authenticated install if GitHub API rate-limits anonymous calls
# curl -fsSL ... | env GITHUB_TOKEN=ghp_... bash

# Custom install directory
curl -fsSL https://raw.githubusercontent.com/boborder/lazyxrp/main/install.sh | INSTALL_DIR=/usr/local/bin bash

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

TUI flags: `--tick-rate`, `--account`, `--server`, `--ws-server`, `--network`, `--yes`, `--seed`, `--mnemonic`, `--allow-insecure-rpc`.

Legacy `lazyxrp watch` still works but prints a deprecation warning — prefer bare `lazyxrp`.

## Script CLI (`-x` / `rc`)

Non-interactive subcommands use **`-x`** (or shell alias **`rc`**):

```bash
# Recommended
lazyxrp -x info
lazyxrp --network testnet -x account <r-address>
lazyxrp -x send <r-destination> --amount 10   # requires XRPL_SEED

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

Bare `lazyxrp info` (without `-x`) still works temporarily but prints a deprecation warning.

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
| `FLARE_RPC_URL` | Flare FTSOv2 RPC (default: Flare mainnet) |
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