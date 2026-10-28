# Changelog

All notable changes to this project are documented here. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [SemVer](https://semver.org/) (pre-1.0: a minor bump may break the CLI).

## [Unreleased]

### Fixed

- TUI: keystrokes were randomly lost (roughly 1 in 3 runs). The terminal event stream started before the NFT image picker queried the terminal, so the picker missed its replies and left a thread reading stdin that stole keys. The event stream now starts after every component's `init`.
- `-x book` / `-x amm` accept a currency symbol such as `RLUSD` (sent as its 160-bit code, same rule as `xrpl.currency`); before, anything that was not a 3-letter or hex code failed with `bad currency`.
- DESIGN.md: TX detail / validator overlays close with `Enter` (`Esc` was documented but is not bound).

### Changed

- Dependencies: `kobe-xrpl` 4 (with `kobe-core` 4, replacing `kobe-primitives`), `vergen-gix` 10, everything else updated within semver. MSRV (`rust-version`) is now **1.99**.

## [0.9.1] - 2026-10-26

### Changed

- `xrpl.currency_code` is now optional: unset, the order-book quote code is derived from `xrpl.currency` (3-letter code as is, otherwise zero-padded 160-bit hex), so changing `currency` no longer leaves the book on the old token. Set it only for a code that cannot be derived, e.g. an AMM LP token. An existing `currency_code` keeps taking precedence.

## [0.9.0] - 2026-10-26

### Added

- FXRP Mint composer (`6`) prefills `Flare recipient` with your own Smart Account personal account (full address shown; only when the Overview Flare read is for the signing account and under 300 s old), so an XRPL-key-only user needs no other Flare address. The personal account is a Flare RPC answer with no pin: the composer says so and asks you to check it on an explorer (security.md S-019 / R-019).
- `-x sa-status <XRPL tx hash>`: read-only; has a Smart Account payment (`sa-memo` / `sa-call` / `sa-vault`) been finalized on Flare (`isTransactionIdUsed`; a plain C2 mint is not tracked)? `-x sa-memo` now waits up to 5 minutes (`--no-wait` skips) for a public executor and, if none finalizes it, prints the `sa-status` / `fxrp-execute <tx> --data 0x` follow-up. The wait never fails the command (the Payment is already validated): a Flare read error is reported with the re-check hint and the exit stays 0. With an executor pinned, `0xE0` / `0xE1` / `0xE2` do not wait (only the pinned executor can finalize). `sa-call` does not wait: a public executor lacks the user-operation bytes.

### Changed

- The TUI message after an FXRP Mint payment no longer implies the next step is C3: it says a public executor usually finalizes within minutes (not guaranteed; no fee floor is known) and how to finalize it yourself (EVM key, FLR).
- Docs: every `docs/*.md` is now English with a Japanese twin `*-ja.md`; `ROADMAP.md` (English, holds the coverage table that TC-248 reads) and `ROADMAP-ja.md` are reorganised by area (XRPL / UI-UX / Flare / quality) with an at-a-glance board, grouped unsupported transaction types, a verification-debt table and explicit proposals (F5-a/b/c). Smart Account data is documented as never persisted (architecture §6.9). Rule in `AGENTS.md`: change both languages together.
- Docs: Flare requirements / README / architecture / DESIGN / security / test / ROADMAP updated for the XRPL-only vs EVM-key split. A read-only mainnet scan found public executors (905 plain + 1000+ Smart Account mints in about 50 days, 95–177 s after XRPL validation); whether one picks up a low-fee payment is unmeasured, and no Flare write has been sent on mainnet.

### Security

- `RUSTSEC-2026-0387` (`crypto-bigint` 0.5.5, timing variability) is ignored in `deny.toml` and the new `.cargo/audit.toml`: it is pinned by `k256` / `ecdsa` 0.16 (xrpl-rust, kobe-xrpl, alloy) and the fixed 0.7.6 is semver-incompatible. Remove both entries once upstream moves to `elliptic-curve` 0.14 (security.md).

## [0.8.0] - 2026-10-21

### Added

- FXRP Direct Mint guards for FAssets v1.3: the C2 Payment is refused when the amount does not exceed the Direct Mint fees (a payment below the minimum fee is minted entirely to the fee receiver on Flare) or while FXRP minting is paused (`mintingPaused()` / `emergencyPauseLevel()`, shown as a badge in the FXRP panel). C3 now reports what the receipt actually did: minted, delayed by the mint rate limit (re-run with the same proof after `executionAllowedAt`), or payment lost to the fee receiver — previously any non-reverting call was shown as success.
- FXRP panel shows the Direct Mint limits (`Max now` = largest mint that is neither delayed nor over the cap; `Cap used`), read in the same Flare batch; the FXRP Mint composer refuses an amount above the global minting cap headroom (the mint would revert and the XRP would wait in the Core Vault).
- CLI `-x fxrp [--tag N]`: read-only Flare FXRP report (JSON) — Direct Mint fees / pause / limits, MintingTagManager (reservation fee, next tag, executor-change cooldown; with `--tag` the owner, minting recipient, allowed executor and any pending executor change), the Core Vault holdings, the registered agent vaults and the global redemption queue (tickets, total XRP and lots, 10 largest agents).
- FTSO panel shows each feed's age (`2s`, `5m`, `3h`) instead of the raw unix timestamp; feeds older than 60 s (or never updated) are highlighted as stale.
- Flare EVM write commands (CLI only; need `[flare.fassets] execute = true`, the Flare EVM key and `--yes` on Flare mainnet; every call estimates gas first): `-x fxrp-execute <XRPL tx hash> [--data HEX]` proves the Direct Mint Payment with the Flare Data Connector (verifier → `FdcHub` attestation, about 20 FLR on mainnet → voting round → DA layer proof) and finalizes it in one command; `-x fxrp-redeem <XRP> <r-address> [--tag N]` (`redeemAmount` / `redeemWithTag`); `-x fxrp-tag-reserve`, `fxrp-tag-set <tag> [--recipient] [--executor]`, `fxrp-tag-transfer <tag> <to>`. New env `FDC_VERIFIER_URL`, `FDC_DA_URL`, `FDC_VERIFIER_API_KEY` (https only).
- `-x sa-vault <firelight|upshift> <deposit|request-redeem|claim> <vault_id> <value>`: Smart Account vault instruction as an XRPL Payment to the operator, through the existing operator-pin / fee-bound / simulate pipeline (the vault id is checked against `getVaults()`).
- `-x sa-memo <ignore-memo|set-nonce|replace-fee|set-executor|remove-executor> <XRP>`: Smart Account memo instructions `0xE0` / `0xE1` / `0xE2` / `0xD0` / `0xD1` (layouts from upstream `MemoInstructions.sol`) as a Direct Mint Payment to the Core Vault through the existing C2 guards (Core Vault pin, fee check, simulate-first, `--yes` on mainnet). It reads the personal account first: `0xE0` / `0xE2` need `--target` not yet minted, `0xE1` needs `--target` already minted plus `newNonce > getNonce` with a jump of at most `u32::MAX`, and a pinned executor is shown (it alone can finalize; `0xD0` / `0xD1` bypass the pin). An executor finalizes with `fxrp-execute <tx> --data 0x`. Verified live on Coston2 (`0xD0`, `0xD1`, `0xE1`, `0xE2`).
- `-x sa-call <XRP> --call TARGET:0xDATA …`: Smart Account custom instruction `0xFE` (calls run from the personal account, value 0): sends the 42-byte memo with the keccak256 of the `PackedUserOperation` and prints the user-operation bytes for `fxrp-execute <tx> --data 0x…`. Verified live on Coston2. `fxrp-execute --round N` resumes a run whose attestation fee was already paid.
- `fxrp-redeem` warns when the receipt carries `RedemptionAmountIncomplete` (remaining UBA) or the older `RedemptionRequestIncomplete` (remaining lots): only part of the amount was allocated.
- FXRP panel shows the personal account's Firelight / Upshift vault positions (vault list from `MasterAccountController.getVaults()`; share balance converted with ERC-4626 `convertToAssets` when the vault has it).

### Security

- Flare writes verify the RPC `eth_chainId` once up front (every Flare subcommand) and decide the mainnet `--yes` gate from the reported chain id; the TUI pairs the XRPL network with its Flare network (`check_network_pair`) for C2 / Smart Account / C3, and C3 gates on the Flare network.
- Flare EVM writes are capped at 10 FLR of gas cost (`MAX_TX_FEE_WEI`, via `send_capped`); FDC reads round parameters before paying the attestation fee and prints the tx hash and a `--round` resume hint; `sa-memo` / `sa-call` apply the same cap checks.
- TUI FXRP Direct Mint and Smart Account (`0x01`/`0x02`) payments re-read the Flare views (fees, Core Vault, pause / mint cap, operator, instruction fee) right before signing and refuse when they differ from the composer's snapshot (`flare::verify_direct_mint_terms` / `verify_smart_account_terms`); the 300 s snapshot age limit is now only a secondary guard. Vault instructions re-check the operator pin only.
- XRPL writes carry the compose-time network and are refused after a network switch; submit is a single attempt (no blind resubmit) and a lost reply is resolved with a `tx(hash)` lookup (makes S-014 true).
- EIP-55 checksums are enforced on mixed-case EVM addresses; `tag_owner` treats only reverts as "not reserved".
- Keybindings are restricted to an allowlist (rejected at load); config permissions are checked on the file actually read; `~` is expanded in `config_dir` / `data_dir` / `LAZYXRP_*`; credentials resolve CLI > env > file, a lower source's other kind is dropped (both kinds at one level is an error).
- More zeroization of key material in `signing.rs`; release profile builds with `overflow-checks = true`.
- `RUSTSEC-2026-0235` ignore removed (`rkyv` is not in `Cargo.lock`); `cargo audit` runs without ignores.
- `install.sh`: the `NO_VERIFY=1` MITM warning is shown at run time; binary installs no longer install Rust by default (`--install-rust` opts in).

### Changed

- NFT URIs: `ipfs://ipfs/<cid>`, upper-case `IPFS://`, `/ipfs/<cid>` and bare CIDs resolve (the doubled form used to 404); gateways are now `ipfs.io`, `gateway.pinata.cloud`, `dweb.link`; base64 `data:` URIs load; the NFT table and preview display ipfs URIs as `https://ipfs.io/ipfs/…` (openable in a browser); the URI column uses the available width and the preview shows the whole URI, sanitised of control / bidi characters.
- Key and bracketed-paste events go to the visible tab only; paste is edit-only (wallet fields and the Tx History filter) and never triggers hotkeys. Overview has no wallet; Tx History `f` / `m` are reachable; the TrustSet form starts empty; NoFreeze needs a typed `NOFREEZE`; the watch account defaults to the signing address when a credential is present; the IOU currency row accepts up to 40 characters; the startup splash ends on a `server_info` error; Flare snapshots are cleared on network change and refused when older than 300 s; in-flight fetches are de-duplicated; help overlay lists the real bindings.
- XRPL coverage is now 50 / 77 (the nine `Loan*` types are Unsupported).
- Currency display: non-standard codes are never shown as standard ones; a fake `XRP` code is shown as a hex prefix.

### Fixed

- `terQUEUED` is reported as queued, not failed.
- DEX ask prices are computed correctly (`1e6 / quality`).
- `NetworkID` is injected when the network id is > 1024 (simulate and signing). Whether Xahau accepts `simulate` with it is **not verified** (docs/problems.md P-007).
- WebSocket has a 30 s read timeout and reconnects after a drop or a network switch.
- Domain verification accepts base58 `nH…` and hex node keys; activation = `account_info` success; dUNL validator-list field ids / VL lengths are decoded per spec; `parse_drops` is strict; IPv6 NFT URIs work.
- The Market oracle panel reports "not configured" (`XrplOracleNotConfigured`) when no oracles are configured.

### Deferred

- Split of `wallet.rs` / `wallet_composer.rs`, a `vendored-openssl` feature, install attestation / signing, and a confirm step for GlobalFreeze / RequireAuth.

## [0.7.0] - 2026-10-16

### Added

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

[Unreleased]: https://github.com/boborder/lazyxrp/compare/v0.9.1...HEAD
[0.9.1]: https://github.com/boborder/lazyxrp/compare/v0.9.0...v0.9.1
[0.9.0]: https://github.com/boborder/lazyxrp/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/boborder/lazyxrp/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/boborder/lazyxrp/compare/v0.6.1...v0.7.0
[0.6.1]: https://github.com/boborder/lazyxrp/compare/v0.4.1...v0.6.1
[0.6.0]: https://github.com/boborder/lazyxrp/compare/v0.4.1...v0.6.0
[0.5.0]: https://github.com/boborder/lazyxrp/compare/v0.4.1...v0.5.0
[0.4.2]: https://github.com/boborder/lazyxrp/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/boborder/lazyxrp/compare/v0.3.0...v0.4.1
[0.4.0]: https://github.com/boborder/lazyxrp/compare/v0.3.0...v0.4.0
[0.3.1]: https://github.com/boborder/lazyxrp/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/boborder/lazyxrp/compare/v0.2.11...v0.3.0
