# lazyxrp

Terminal UI for the XRP Ledger — monitor accounts, order books, NFTs, trust lines, and AMM pools.

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

- Rust stable (Edition 2024)
- macOS / Linux (verified on macOS arm64)

## Install

```bash
# Latest release via install script (default: ~/.local/bin)
curl -fsSL https://raw.githubusercontent.com/boborder/lazyxrp/main/install.sh | bash

# Custom install path
curl -fsSL https://raw.githubusercontent.com/boborder/lazyxrp/main/install.sh | INSTALL_DIR=/usr/local/bin bash

# From source
cargo install --path .
```

## Quick Start

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

Default bindings ship in the embedded `.config/config.json5` and merge with your user config. Common keys:

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
| `XRPL_SEED` | Signing seed (Phase 3 write TX — prefer over config file) |

## Development

```bash
cargo check
cargo test
cargo run --bin lazyxrp -- watch --account <r-address>
cargo run --bin lazyxrp -- --network testnet info
```

## Documentation

- `docs/requirements.md` — functional & non-functional requirements
- `docs/design.md` — architecture & data flow
- `docs/tech.md` — tech stack & versions
- `docs/tasks.md` — task status
- `docs/directory.md` — directory structure
- `docs/problems.md` — known issues

## References

- [XRPL Rust SDK](https://github.com/XRPLF/xrpl-rust)
- [XRPL Documentation](https://xrpl.org/docs)
