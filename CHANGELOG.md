# Changelog

All notable changes to this project are documented here. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [SemVer](https://semver.org/) (pre-1.0: a minor bump may break the CLI).

## [Unreleased]

### Added

- `-x offer-create <GETS> <PAYS>` — place a DEX offer from the CLI (specs `XRP:drops` / `CUR:issuer:value`, checked before any RPC).
- `-x faucet [ADDR]` — testnet/devnet only. Without `ADDR`, keys are generated locally and only the address is sent to the faucet; the seed is printed once and never saved.
- `mise run e2e-testnet` — opt-in live testnet run of every `-x` write path (not in CI).

### Changed

- **CLI writes (`send`, `offer-create`, `offer-cancel`) now wait until the transaction is validated** and exit non-zero unless the final result is `tes…` (or the transaction expires, or 5 minutes pass). `--no-wait` restores the old return-after-submit behavior.

### Security

- Before signing, the endpoint's `server_info.network_id` must match the selected network (TUI and CLI). Previously a custom `XRPL_RPC_SERVER` / `rpc_server` / `--server` on mainnet combined with `--network testnet` skipped the `--yes` guard and still signed and submitted to mainnet.
- **Breaking (FXRP Direct Mint):** the Core Vault address returned by the Flare RPC is signed only when it equals the new `[flare.fassets] expected_core_vault` (verify it out of band and set it); unset or mismatched → refused.
- Signing refuses a `simulate` answer whose `Sequence` differs from the requested one or whose `LastLedgerSequence` is more than 100 ledgers past its `ledger_index`.
- The Payment quote (`p`) only uses path alternatives in the requested Send max currency/issuer; overflowing amounts are an error instead of wrapping.
- The transaction hash shown and waited on is computed from the signed blob, not taken from the node.
- `simulate` result strings are stripped of control characters before they reach error output.
- `-x faucet` no longer follows redirects and caps the response body.
- The JSON-RPC client no longer follows redirects (a node could otherwise forward the signed `submit` body to another URL).
- The Flare EVM executor key is read and removed from the environment at startup (it was removed lazily on first use, while other tasks ran, and lost after one failed attempt); env secret copies are zeroized.
- The TUI submit result shows the signed fee (`<hash> · fee N drops`).
- `XRP:<drops>:<extra>` amount specs are rejected; `-x send` validates the amount (> 0) before any RPC.
- The key-generation popup is cleared on network switch and its seed is zeroized on drop; `-x faucet` keeps the generated seed in a secret type.
- CI/CD actions pinned to commit SHAs; `cross` pinned.

### Fixed

- CLI wait: a transaction validated between the `tx` check and the ledger read is no longer reported as expired.

## [0.3.0] - 2026-10-10

### Added

- Wallet composer: OfferCancel (`8`), Payment **Send max** (cross-currency via default paths) and `p` quote via `ripple_path_find` + `[xrpl] slippage_bps`.
- `-x offers <ADDR>` and `-x offer-cancel <SEQ>`.
- Global keybindings can be overridden from the user config.

### Changed

- **Breaking:** bare subcommands (`lazyxrp info`) and `lazyxrp watch` were removed — use `lazyxrp -x <cmd>`; plain `lazyxrp` starts the TUI.
- `-x send` simulates before signing and exits non-zero when signing or submission fails.
- Payment validation is shared by the composer preview, submit, quote and the poll task.

### Fixed

- Every write failed against live rippled 3.x: `simulate` does not return `LastLedgerSequence`; it is now filled from `ledger_index + 20`.

### Security

- Audit hardening: GitHub token kept out of `argv` in `install.sh`, guarded `xrp-ledger.toml` fetch, no panics on oversized remote numbers or multibyte remote strings.

## Earlier versions

See [GitHub Releases](https://github.com/boborder/lazyxrp/releases) for 0.2.11 and older.

[Unreleased]: https://github.com/boborder/lazyxrp/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/boborder/lazyxrp/compare/v0.2.11...v0.3.0
