# Security Policy

lazyxrp signs XRP Ledger transactions, so security reports are welcome and handled first.

## Supported versions

Only the latest release on [crates.io](https://crates.io/crates/lazyxrp) / [GitHub Releases](https://github.com/boborder/lazyxrp/releases) receives fixes.

## Reporting a vulnerability

Report privately via **GitHub → Security → [Report a vulnerability](https://github.com/boborder/lazyxrp/security/advisories/new)**. Please do not open a public issue for anything that could lead to loss of funds or key exposure.

Include the version (`lazyxrp --version`), network, and steps to reproduce. **Never include a seed or mnemonic** — use a fresh testnet account (`lazyxrp --network testnet -x faucet`).

## Safety model

- Every write is simulated before signing; mainnet and Xahau writes require `--yes` or an interactive confirmation.
- Before signing, the endpoint's `network_id` must match the selected network.
- Signing over `http://` / `ws://` is refused.
- Flare writes verify the chain id and enforce out-of-band pins for the Core Vault, Smart Account operator and payable minting-tag manager; direct-mint pause/limit reads fail closed. FDC proofs must match the requested transaction, owner and voting round.
- Prefer `XRPL_SEED` / `XRPL_MNEMONIC` over `--seed` / `--mnemonic` (visible in `ps` and shell history).
