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

- Rust **1.91+** (MSRV from `Cargo.toml` `rust-version`)
- Daily builds use the **`stable`** channel from [`rust-toolchain.toml`](rust-toolchain.toml)
- Edition 2024
- macOS / Linux (verified on macOS arm64)

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

Global flags (before subcommands): `--network`, `--yes` (skip confirmations for **mainnet writes** and **`--self-uninstall`**), `--tick-rate` / `--frame-rate`, `--server` / `--ws-server`, `--seed`.

```bash
lazyxrp watch --account <r-address>
lazyxrp --network testnet watch --account <r-address>
```

## CLI Commands

```bash
lazyxrp info                                                    # server info
lazyxrp account <r-address>                                     # account info
lazyxrp account-status <r-address>                              # activated if reserve met (>= 10 XRP)
lazyxrp book --base XRP --quote USD --issuer <r-issuer>         # order book
lazyxrp summary --account <r-address>                           # combined summary
lazyxrp nfts <r-address>                                        # NFT list
lazyxrp lines <r-address>                                       # trust lines
lazyxrp amm --asset1 XRP --asset2 USD --issuer2 <r-issuer>      # AMM pool info
lazyxrp txhistory <r-address> --limit 20                        # tx history
lazyxrp send <r-destination> --amount 10                        # decimal string amount; requires XRPL_SEED
```

### Short command: `rp`

`rp` is a second binary (same crate). `cargo install lazyxrp`, release archives, and `install.sh` all provide it. Lookup-only CLI (not the TUI):

```bash
rp -t <txid|r-address>
rp <txid|r-address>
rp --network testnet -t <txid|r-address>
```

`install.sh` falls back to a `rp` → `lazyxrp` symlink if an older archive has no `rp` binary. When `INSTALL_DIR` is missing from `PATH`, the installer can append it to your shell profile (interactive prompt; auto under `CI=1` / `-q`).

## Key Bindings

Defaults ship in the embedded repo-root `config.json5` and merge with your user config:

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Next / previous tab |
| `1`–`4` | Jump to tab by index |
| `h` / `l` or `←` / `→` | Move focus between panels in the current tab |
| `j` / `k` or `↑` / `↓` | Move selection in the focused panel |
| `Enter` | Open detail overlay (tables / dUNL row) |
| `t` | Account tab: TX composer (Payment / AccountSet / SetRegularKey / OfferCreate / TrustSet / FXRP Mint / Execute) |
| `g` | Account tab: local keygen overlay |
| `f` | Account tab (tx history): filter mode |
| `r` | Refresh account |
| `b` | Refresh order book |
| `o` | Refresh ledger objects (`account_objects`) |
| `?` | Toggle help overlay |
| `q` / `Ctrl-c` / `Ctrl-d` | Quit |
| `Ctrl-z` | Suspend (when the terminal handles SIGTSTP) |

## Network Selection

```
Priority: --network flag > XRPL_NETWORK env > config.toml > mainnet (default)
```

| Network | RPC | WS |
|---------|-----|----|
| mainnet | `https://xrplcluster.com` | `wss://xrplcluster.com` |
| testnet | `https://s.altnet.rippletest.net:51234` | `wss://s.altnet.rippletest.net:51233` |
| devnet  | `https://s.devnet.rippletest.net:51234` | `wss://s.devnet.rippletest.net:51233` |

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
| `XRPL_NETWORK` | Network preset (`mainnet` / `testnet` / `devnet`) |
| `XRPL_RPC_SERVER` | Custom RPC endpoint (overrides network preset) |
| `XRPL_WS_SERVER` | Custom WS endpoint (overrides network preset) |
| `XRPL_SEED` | Signing seed for write TX (prefer over config file) |
| `FLARE_RPC_URL` | Flare FTSOv2 RPC (default: Flare mainnet) |
| `FLARE_FEEDS` | Flare feeds for Overview / Market (e.g. `FXRP/USD,FLR/USD`) |
| `FLARE_FEED` | Legacy single-feed override |
| `FLARE_EVM_KEY` | Flare executor key for FXRP C3 (`[flare.fassets] execute=true` only) |
| `LAZYXRP_CONFIG` | Override config directory (`..` rejected) |
| `LAZYXRP_DATA` | Override data directory |
| `LAZYXRP_LOG_LEVEL` | Default file log filter (`tracing` `EnvFilter`) |

> **Security:** `--seed` appears in process listings (`ps`) and shell history. Prefer `XRPL_SEED`. Copy `.env.example` to a local `.env` only — **never commit** `.env` (gitignored).

## Development

From the repo root, use [`mise`](https://mise.jdx.dev/) (`mise run install` runs `./install.sh -q`) or plain Cargo:

```bash
cargo fmt
cargo check
cargo test
cargo run --bin lazyxrp -- watch --account <r-address>
cargo run --bin lazyxrp -- --network testnet info

# Optional Flare FTSOv2 overrides (Overview / Market)
# FLARE_FEEDS=FXRP/USD,FLR/USD,BTC/USD,ETH/USD
# FLARE_RPC_URL=https://flare-api.flare.network/ext/C/rpc
```

## Contributing

Start with [`AGENTS.md`](./AGENTS.md). Minimum bar: `cargo fmt` / `cargo check`, and keep `docs/` aligned when behavior or documented workflows change. Agent-oriented invariants and change rules live under [`docs/agent/`](./docs/agent/).

## Documentation

Index: [`docs/README.md`](./docs/README.md).

| Doc | Topic |
|-----|--------|
| [`docs/requirements.md`](./docs/requirements.md) | Functional & non-functional requirements |
| [`docs/design.md`](./docs/design.md) | Architecture & data flow |
| [`docs/tech.md`](./docs/tech.md) | Tech stack & versions |
| [`docs/test.md`](./docs/test.md) | Test catalog & expectations |
| [`docs/tasks.md`](./docs/tasks.md) | Task status |
| [`docs/directory.md`](./docs/directory.md) | Directory structure |
| [`docs/tx-detail.md`](./docs/tx-detail.md) | Transaction detail overlay |
| [`docs/references.md`](./docs/references.md) | Supplementary reference |
| [`docs/problems.md`](./docs/problems.md) | Known issues |
| [`docs/security.md`](./docs/security.md) | Threat model & hardening |
| [`docs/architecture/`](./docs/architecture/) | C4 context & containers |
| [`docs/RELEASE.md`](./docs/RELEASE.md) | Release / auto-tag checklist |

## References

- [XRPL Rust SDK](https://github.com/XRPLF/xrpl-rust)
- [XRPL Documentation](https://xrpl.org/docs)

## License

MIT — see [`LICENSE`](./LICENSE).
