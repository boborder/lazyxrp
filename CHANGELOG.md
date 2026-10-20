# Changelog

All notable changes to this project are documented here. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [SemVer](https://semver.org/) (pre-1.0: a minor bump may break the CLI).

## [Unreleased]

## [0.7.0] - 2026-10-16

### Added
- FXRP Direct Mint guards for FAssets v1.3: the C2 Payment is refused when the amount does not exceed the Direct Mint fees (a payment below the minimum fee is minted entirely to the fee receiver on Flare) or while FXRP minting is paused (`mintingPaused()` / `emergencyPauseLevel()`, shown as a badge in the FXRP panel). C3 now reports what the receipt actually did: minted, delayed by the mint rate limit (re-run with the same proof after `executionAllowedAt`), or payment lost to the fee receiver — previously any non-reverting call was shown as success.
- FXRP panel shows the Direct Mint limits (`Max now` = largest mint that is neither delayed nor over the cap; `Cap used`), read in the same Flare batch; the FXRP Mint composer refuses an amount above the global minting cap headroom (the mint would revert and the XRP would wait in the Core Vault).
- CLI `-x fxrp [--tag N]`: read-only Flare FXRP report (JSON) — Direct Mint fees / pause / limits, MintingTagManager (reservation fee, next tag, executor-change cooldown; with `--tag` the owner, minting recipient, allowed executor and any pending executor change), the Core Vault holdings, the registered agent vaults and the global redemption queue (tickets, total XRP and lots, 10 largest agents).
- FTSO panel shows each feed's age (`2s`, `5m`, `3h`) instead of the raw unix timestamp; feeds older than 60 s (or never updated) are highlighted as stale.
- Flare EVM write commands (CLI only; need `[flare.fassets] execute = true`, the Flare EVM key and `--yes` on Flare mainnet; every call estimates gas first): `-x fxrp-execute <XRPL tx hash> [--data HEX]` proves the Direct Mint Payment with the Flare Data Connector (verifier → `FdcHub` attestation, about 20 FLR on mainnet → voting round → DA layer proof) and finalizes it in one command; `-x fxrp-redeem <XRP> <r-address> [--tag N]` (`redeemAmount` / `redeemWithTag`); `-x fxrp-tag-reserve`, `fxrp-tag-set <tag> [--recipient] [--executor]`, `fxrp-tag-transfer <tag> <to>`. New env `FDC_VERIFIER_URL`, `FDC_DA_URL`, `FDC_VERIFIER_API_KEY` (https only).
- `-x sa-vault <firelight|upshift> <deposit|request-redeem|claim> <vault_id> <value>`: Smart Account vault instruction as an XRPL Payment to the operator, through the existing operator-pin / fee-bound / simulate pipeline (the vault id is checked against `getVaults()`).
- `-x sa-memo <ignore-memo|set-nonce|replace-fee|set-executor|remove-executor> <XRP>`: Smart Account memo instructions `0xE0` / `0xE1` / `0xE2` / `0xD0` / `0xD1` (layouts from upstream `MemoInstructions.sol`) as a Direct Mint Payment to the Core Vault through the existing C2 guards (Core Vault pin, fee check, simulate-first, `--yes` on mainnet). It reads the personal account first: `0xE0` / `0xE2` need `--target` not yet minted, `0xE1` needs `--target` already minted plus `newNonce > getNonce` with a jump of at most `u32::MAX`, and a pinned executor is shown (it alone can finalize; `0xD0` / `0xD1` bypass the pin). An executor finalizes with `fxrp-execute <tx> --data 0x`. Not sent live yet.
- `fxrp-redeem` warns when the receipt carries `RedemptionAmountIncomplete` (remaining UBA) or the older `RedemptionRequestIncomplete` (remaining lots): only part of the amount was allocated.
- FXRP panel shows the personal account's Firelight / Upshift vault positions (vault list from `MasterAccountController.getVaults()`; share balance converted with ERC-4626 `convertToAssets` when the vault has it).
- Flare Smart Accounts (proof-based instruction flow). Overview FXRP panel rows show the Flare personal account of your XRPL address (FXRP balance, nonce, deployed?, operator, transfer / redeem instruction fees; `MasterAccountController` resolved via the Flare contract registry, same Flare poll batch and `[flare] display` gating as FXRP). Wallet composer `a` sends an `0x01` FXRP transfer (decimal FXRP amount → the token's raw base units using the FXRP `decimals()` read from the chain, exact integer math; `0x` recipient) or `0x02` FXRP redeem (lots) as an XRPL Payment of exactly `getInstructionFee(id)` XRP drops to the operator, no `DestinationTag`, with the 32-byte instruction as the only Memo; after submit the operator executes it on Flare. The composer refuses to queue until the Smart Account read (including `decimals()`) is loaded. Not included yet: custom instructions `0xFE` / `0xFF`.
- **New config `[flare.smart_accounts] expected_operator`**: the operator address returned by the Flare RPC is paid only when it equals this pin (verify it out of band); unset or mismatched → refused before any RPC. The instruction fee must be 1 drop – 10 XRP. Mainnet needs `--yes`; simulate runs before signing.
- **Declarative transaction specs (`src/xrpl/txspec/`)** and the first family, NFT: NFTokenMint / NFTokenBurn / NFTokenCreateOffer / NFTokenAcceptOffer / NFTokenCancelOffer, written through one generic path (one `GenericTxSubmit` Action trio, one `PollCommand`, the existing mainnet `--yes` / simulate-first / Fee-cap / submit-lock guards). Wallet composer `m` "Transaction" generates the form from the field specs (inline validation, help, preview; Burn and AcceptOffer need a typed `BURN` / `ACCEPT`). CLI `-x tx <TYPE> Field=value…`, `-x nft-mint --uri --taxon [--flags burnable,onlyXRP,transferable] [--transfer-fee bps] [--issuer]`, `-x nft-burn <NFTokenID> [--owner]`; all refused under `--watch`. Assets tab `x` opens the burn composer for the selected NFT. `tfMutable` minting and zero-price offers are not supported.
- XRPL coverage tracking: `src/coverage.rs` is the single source of truth (status Existing / Spec / Planned / Unsupported per transaction type, plus read / TUI / CLI write capability). `lazyxrp -x coverage` prints the table; ROADMAP.md "XRPL coverage" and the README count are checked against it by a test.
- More declarative transaction families (all via Wallet composer `m` and `-x tx`): Check ×3, TicketCreate, DepositPreauth, SignerListSet, Clawback, PaymentChannel ×3 (pasted claim `Signature` / `PublicKey`, nothing signed locally), AMMCreate / Vote / Bid / Delete, MPTokenIssuance ×3 + MPTokenAuthorize, Credential ×3, DID ×2, Oracle ×2, PermissionedDomain ×2, XChain ×6 (compact `XChainBridge=lockDoor,lockIssue,issueDoor,issueIssue`). Fund-moving and irreversible types need a typed confirmation word (e.g. `CASH`, `CLAWBACK`, `DESTROY`, `DELETE`, `COMMIT`). Writable types: **50 / 68**; attestation, Vault (MPT amounts) and ConfidentialMPT types stay Planned / Unsupported with reasons in the coverage table. Tx detail shows Clawback / MPT / Credential / DID / Oracle / PermissionedDomain fields.
- Fixed: `-x`/TUI `xrp_to_drops` accepted `+`/`-`-prefixed digits (`1.+5` became 1.05 XRP); `drops_to_xrp` loses no digits above 2^53 drops.

### Security

- NFT image / `xrp-ledger.toml` fetches: a redirect from https to http is refused; the private-address guard now also rejects IPv4 hidden in IPv6 (IPv4-compatible `::a.b.c.d`, NAT64 `64:ff9b::/96`, 6to4 `2002::/16`), deprecated site-local `fec0::/10` and `0.0.0.0/8`.
- Text from RPC nodes (server host / version, CLI error and engine messages) has bidi-override and zero-width format characters stripped as well as control characters, so it cannot visually reorder or hide text.
- Transaction detail: an absurd `date` / time field no longer overflows (debug panic, wrapped date in release); it shows the `----` placeholder.
- `install.sh` and the README one-liner download over https only (`--proto '=https'`, also for every redirect hop).

## [0.6.1] - 2026-10-11

### Fixed

- Market tab AMM panel stayed on "loading" forever: nothing ever fetched `amm_info`. It is now polled for the configured book pair (mainnet XRP/RLUSD shows the pool); a pair without a pool shows "no AMM pool for this pair" in the panel, without a status-bar error.

### Changed

- Internal cleanup (no CLI/TUI behavior change): shared submit helpers for TUI and `-x` writes, dead actions/events and pass-through wrappers removed, dependencies `config`, `signal-hook`, `tracing-error`, `strum`, `url`, `time` and build-dep `anyhow` dropped. User `config.toml` is now read with `toml` directly: keys are matched as written (the old loader lowercased keys and coerced quoted numbers).

## [0.6.0] - 2026-10-11

### Added

- `--watch`: read-only start. No signing key is loaded (env, config, `--seed` / `--mnemonic`, `FLARE_EVM_KEY`); the TUI shows the account panel for the key's address (or `--account`) without the composer or key generation, and `-x` writes are refused.

## [0.5.0] - 2026-10-11

### Added

- AccountDelete: Wallet composer `x` and `-x account-delete <DEST>`. Deletes the signing account and sends the remaining XRP to the destination (X-address for a destination tag). The TUI always asks you to type `DELETE` first; the CLI follows the usual simulate → `--yes` on mainnet flow. The fee (owner reserve) comes from simulate.

### Changed

- Wallet panel: an address with no account on the connected network (unfunded or deleted) now says so instead of showing "None".

## [0.4.2] - 2026-10-11

### Fixed

- FXRP Mint (C2) payments were refused at `simulate` ("Field 'Memos.Memo.MemoFormat' has bad type"): unset memo fields were sent as `null`. Null fields are now dropped before `simulate`.
- FXRP Execute (C3) called an `executeDirectMinting` signature the AssetManager does not have and could never succeed. It now sends the deployed `IXRPPayment.Proof` and takes the DA layer answer as returned (`{"response": {…}, "proof": [...]}`); the old hex-`response` form is rejected. A reverted execution is now reported as an error instead of a success. Verified on XRPL testnet + Coston2.

## [0.4.1] - 2026-10-11

### Added

- Wallet: submitting the same write with identical parameters again is refused until the first one's result arrives (double `s` / repeated confirm).
- `-x offers` and the OfferCancel list now include every open offer (`account_offers` marker paging; previously the first 200).

### Fixed

- WebSocket ledger-close and account-transaction events were never recognized (the XRPL client re-wrapped stream messages and dropped their `type`), so the TUI refreshed only on the poll interval and the Server panel showed `Reserve: - XRP`. Raw frames are now read.

## [0.4.0] - 2026-10-11

### Added

- Wallet composer `9` **EscrowCreate** (XRP, FinishAfter / optional CancelAfter as `90s` / `30m` / `2h` / `7d` from now) and `0` **Escrows**: list escrows you own or receive, `s` EscrowFinish / `c` EscrowCancel. TUI only; crypto-condition escrows can only be cancelled.
- AMM deposit / withdraw: Wallet composer `d` **AMMDeposit** (two-asset, both amounts are maxima) and `w` **AMMWithdraw** (all, or an LP-token amount), plus `-x amm-deposit <AMOUNT> <AMOUNT2>` / `-x amm-withdraw <ASSET> <ASSET2> [--lp N]`.

### Changed

- Internal: the poll task's write paths moved to `src/xrpl/submit.rs` (no behavior change).

### Fixed

- Market AMM panel and `-x amm` showed `asset2 XRP`, empty pools and `0 ?` LP against live rippled: `amm_info` fields are snake_case (`amount`, `amount2`, `lp_token`, `trading_fee`), not ledger-object casing.

## [0.3.1] - 2026-10-11

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

[Unreleased]: https://github.com/boborder/lazyxrp/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/boborder/lazyxrp/compare/v0.6.1...v0.7.0
[0.6.1]: https://github.com/boborder/lazyxrp/compare/v0.4.1...v0.6.1
[0.6.0]: https://github.com/boborder/lazyxrp/compare/v0.4.1...v0.6.0
[0.5.0]: https://github.com/boborder/lazyxrp/compare/v0.4.1...v0.5.0
[0.4.2]: https://github.com/boborder/lazyxrp/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/boborder/lazyxrp/compare/v0.3.0...v0.4.1
[0.4.0]: https://github.com/boborder/lazyxrp/compare/v0.3.0...v0.4.0
[0.3.1]: https://github.com/boborder/lazyxrp/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/boborder/lazyxrp/compare/v0.2.11...v0.3.0
