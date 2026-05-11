# lazyxrp

Terminal UI for the XRP Ledger — monitor accounts, order books, NFTs, trust lines, and AMM pools.

Repository: https://github.com/boborder/lazyxrp

## Features

Watch mode uses **five top-level tabs** (jump with number keys `1`–`5`):

| Tab | Content |
|-----|---------|
| **Overview** | Server panel + wallet / seed-derived account summary |
| **Account** | Account summary + recent transaction history |
| **Market** | Order book, AMM pool, trust lines |
| **NFTs** | NFT list with taxon / serial / URI |
| **Objects** | `account_objects` for the watched account: upper — Checks, Tickets, MPT, DID, DepositPreauth, SignerList, …; lower — Payment channels + Escrow |

## Requirements

- Rust **1.91+** stable (`Cargo.toml` の `rust-version`; repo は [`rust-toolchain.toml`](rust-toolchain.toml) で **1.91.0** にピン)
- Edition 2024
- macOS / Linux (verified on macOS arm64)

## Install

```bash
# Latest release via install script (default: ~/.local/bin)
curl -fsSL https://raw.githubusercontent.com/boborder/lazyxrp/main/install.sh | bash

# If GitHub REST rate-limits anonymous API calls (busy NAT/VPN/etc.), authenticate:
# curl -fsSL ... | env GITHUB_TOKEN=ghp_... bash

# Custom install path
curl -fsSL https://raw.githubusercontent.com/boborder/lazyxrp/main/install.sh | INSTALL_DIR=/usr/local/bin bash

# From crates.io (Rust toolchain)
cargo install lazyxrp
# Prefer reproducible deps when the publish includes Cargo.lock:
# cargo install lazyxrp --locked

# From source (this repo) — clone first, run from repo root (`rust-toolchain.toml` /
# `Cargo.toml` next to `./install.sh` if you let the installer build)
cargo install --path .
```

**Uninstall** — `./install.sh` does not execute removal commands for you.

```bash
# From an installed lazyxrp on PATH — removes current binary + `{name}.bak` + resolved config/data dirs,
# loads paths like `cargo run`/`Config::new` would (respects overrides in config.toml).
# Confirmation: type yes when prompted, or use --yes to skip.
lazyxrp --self-uninstall
lazyxrp --self-uninstall --yes

# Prebuilt or release binary (adjust path if needed)
rm -f ~/.local/bin/lazyxrp ~/.local/bin/lazyxrp.bak

# install.sh → cargo into ~/.local/bin
cargo uninstall lazyxrp --root "$HOME/.local"

# Default cargo prefix or `cargo install --path .`
cargo uninstall lazyxrp

# Optional — same dirs as --self-uninstall when you have no binary (see src/config.rs; no LAZYXRP_*)
rm -rf ~/.config/lazyxrp ~/.local/share/com.kdheepak.lazyxrp   # Linux / typical XDG
# macOS: rm -rf "$HOME/Library/Application Support/lazyxrp" \
#           "$HOME/Library/Application Support/com.kdheepak.lazyxrp"
# Custom data_dir/config_dir or LAZYXRP_CONFIG / LAZYXRP_DATA → remove those paths instead.
```

`cargo uninstall` is still needed when you want Cargo’s metadata cleaned up for a given prefix; `--self-uninstall` only deletes files and directories.

`./install.sh --uninstall-help` prints the same reference.

## Quick Start

Global flags (before subcommands): `--network`, `--yes` (skip confirmations for **mainnet writes** and **`--self-uninstall`**), `--tick-rate` / `--frame-rate`, `--server` / `--ws-server`, `--seed`.

```bash
# TUI watch mode
lazyxrp watch --account <r-address>

# Use testnet
lazyxrp --network testnet watch --account <r-address>
```

## CLI Commands

```bash
lazyxrp info                                                    # server info
lazyxrp account <r-address>                                     # account info
lazyxrp account-status <r-address>                              # check if activated (>= 10 XRP)
lazyxrp book --base XRP --quote USD --issuer <r-issuer>        # order book
lazyxrp summary --account <r-address>                           # combined summary
lazyxrp nfts <r-address>                                        # NFT list
lazyxrp lines <r-address>                                       # trust lines
lazyxrp amm --asset1 XRP --asset2 USD --issuer2 <r-issuer>     # AMM pool info
lazyxrp txhistory <r-address> --limit 20                        # tx history
lazyxrp send <r-destination> --amount 10                        # amount is a decimal string (avoids float rounding); requires XRPL_SEED
```

## Key Bindings

Default bindings ship in the embedded repo-root `config.json5` and merge with your user config. Common keys:

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Next / previous tab |
| `1`–`5` | Jump to tab by index |
| `h` / `l` or `←` / `→` | Move focus between panels in the current tab |
| `j` / `k` or `↑` / `↓` | Move selection in the focused panel |
| `r` | Refresh account |
| `b` | Refresh order book |
| `o` | Refresh ledger objects (`account_objects` — Checks / MPT / DID / PayChan / Escrow, etc.) |
| `?` | Toggle help overlay |
| `q` / `Ctrl-c` / `Ctrl-d` | Quit |
| `Ctrl-z` | Suspend（端末が SIGTSTP を扱う場合） |

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

Config file lookup follows XDG Base Directory:

- `$XDG_CONFIG_HOME/lazyxrp/config.toml`
- `~/.config/lazyxrp/config.toml` (fallback)

```bash
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/lazyxrp"
# Sample overrides (optional): repo ships `.config/lazyxrp/config.toml` as a starting point.
cp .config/lazyxrp/config.toml "${XDG_CONFIG_HOME:-$HOME/.config}/lazyxrp/config.toml"
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `XRPL_NETWORK` | Network preset (`mainnet` / `testnet` / `devnet`) |
| `XRPL_RPC_SERVER` | Custom RPC endpoint (overrides network preset) |
| `XRPL_WS_SERVER` | Custom WS endpoint (overrides network preset) |
| `XRPL_SEED` | Signing seed (write TX — prefer over config file) |
| `LAZYXRP_CONFIG` | Override config directory (`..` rejected) |
| `LAZYXRP_DATA` | Override data directory |
| `LAZYXRP_LOG_LEVEL` | Default file log filter (`tracing` `EnvFilter`) |

## Development

From the repo root you can use [`mise`](https://mise.jdx.dev/) (`mise run install` runs `./install.sh -q`) or plain Cargo.

```bash
cargo fmt
cargo check
cargo test
cargo run --bin lazyxrp -- watch --account <r-address>
cargo run --bin lazyxrp -- --network testnet info
```

## Contributing

Start with [`AGENTS.md`](./AGENTS.md) — minimal bar is `cargo fmt` / `cargo check`, and keep `docs/` aligned when you change behavior or documented workflows. Policy detail: [`docs/agents/README.md`](./docs/agents/README.md).

## Documentation

- `docs/requirements.md` — functional & non-functional requirements
- `docs/design.md` — architecture & data flow
- `docs/tech.md` — tech stack & versions
- `docs/test.md` — testing notes & expectations
- `docs/tasks.md` — task status
- `docs/directory.md` — directory structure
- `docs/reference.md` — supplementary reference
- `docs/problems.md` — known issues
- `docs/security.md` — threat model & hardening notes
- `docs/architecture/` — C4 context & containers

## References

- [XRPL Rust SDK](https://github.com/XRPLF/xrpl-rust)
- [XRPL Documentation](https://xrpl.org/docs)

## License

MIT — see `LICENSE` in this repository.
