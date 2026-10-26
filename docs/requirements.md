# requirements.md

[日本語](requirements-ja.md)

**Role:** requirements definition and goal design (SSOT for FR/NFR).

Product **requirements** (what). Behavioral design: [`architecture.md`](architecture.md). Interaction & visuals: [`DESIGN.md`](../DESIGN.md). Plan: [`ROADMAP.md`](../ROADMAP.md).

## 1. Purpose

- Continuously monitor XRPL state in a TUI so the information needed for trading decisions can be checked quickly.
- Be practical for personal operation while keeping a structure that is easy to reuse as a public project.

## 2. Target users

- Primary: the developer (daily use in their own operations)
- Secondary: external users of the public repository

## 3. Functional Requirements

### FR-01: Monitor mode

- `lazyxrp` (no arguments) starts the TUI and keeps displaying XRPL information.
- By default it monitors the account in the config file; `--account` overrides it.

### FR-02: Server info retrieval

- Periodically fetch `server_info` / `fee` via RPC and reflect them on screen.

### FR-03: Account info retrieval

- Fetch `account_info` of the monitored account and show balance and status.
- A manual refresh action re-fetches immediately.

### FR-04: Order book retrieval

- Fetch `book_offers` for the specified currency pair and reflect it on screen.
- A manual refresh action re-fetches immediately.

### FR-05: WebSocket subscribe

- Subscribe to the target account's stream over a WebSocket connection and reflect received events on screen.

### FR-06: CLI command execution

- Support the `info` / `account` / `book` / `summary` commands via **`lazyxrp -x`** (or alias `rc`), so results can be printed without starting the TUI.

### FR-07: Config loading

- Settings are loaded from a file; defaults are used when unspecified.
- The config file is looked up at `$XDG_CONFIG_HOME/lazyxrp/config.toml` first, and at `~/.config/lazyxrp/config.toml` when that is unset.
- Key bindings (global defaults in `config.json5`, details in [`DESIGN.md`](../DESIGN.md)) and XRPL connection settings (RPC/WS, currency, issuer, poll interval) are supported.

## 4. Non-Functional Requirements

### NFR-01: Responsiveness

- With default settings, rendering and input do not stall, and the app never becomes unresponsive while monitoring.

### NFR-02: Reliability

- A transient RPC/WS failure does not abort the whole process immediately.
- On exit, background tasks are cancelled and the TUI terminates cleanly.

### NFR-03: Observability

- Major events and errors can be logged.

### NFR-04: Portability

- Buildable on the development environment (macOS arm64).

## 5. Constraints

- Implementation language is Rust (Edition 2024).
- Async runtime is Tokio.
- UI libraries are Ratatui + Crossterm.
- XRPL communication uses `xrpl-rust`.

### FR-08: NFT info retrieval

- Fetch and show the monitored account's NFT list with `account_nfts`.
- In the TUI Assets tab, fetch the image URI (HTTP(S) / IPFS / Arweave) of the selected NFT and show a preview using a supported terminal protocol or the halfblocks fallback.
- Also available via the CLI command `nfts <address>`.

### FR-09: TrustLine info retrieval

- Fetch and show the monitored account's TrustLine list with `account_lines`.
- Also available via the CLI command `lines <address>`.

### FR-10: AMM pool info retrieval

- Fetch and show the AMM pool state of the specified currency pair with `amm_info`.
- Also available via the CLI command `amm --asset1 <currency> --asset2 <currency> [--issuer1 <r-addr>] [--issuer2 <r-addr>]`.

### FR-11: Tx history retrieval

- Fetch and show the latest N transactions of the monitored account with `account_tx`.
- Also available via the CLI command `tx-history <address> [--limit N]`.

### FR-14: Account ledger objects (read)

- Fetch objects tied to the monitored account (Check / Ticket / MPT / DID / Payment Channel / Escrow, etc.) with `account_objects` and show them by type in the TUI **Assets** tab (upper pane: misc, lower pane: channel/escrow).
- A manual refresh (default key `o`) re-fetches immediately.

### FR-12: Network selection

- The target can be switched with `--network mainnet|testnet|devnet|xahau|xahau-test`.
- `mainnet` is used when unspecified.
- It can also be set by the environment variable `XRPL_NETWORK` and by `[xrpl] network` in `config.toml`.
- Precedence: CLI flag > environment variable > config.toml > default (mainnet).
- Within a TUI session, `<Ctrl-n>` cycles networks (not persisted to the config file). A custom RPC endpoint is kept when specified.
- `xahau` is treated as a production chain, so the mainnet write guard (`--yes`) applies.

### FR-15: Flare display level

- `[flare] display` selects `full` / `compact` / `off` (default `full`).
- When `off`, the Flare panels on Overview / Market are omitted and FTSO/FXRP read polling is skipped (the RPC URL for C3 execute is kept).
- When `compact`, the Oracle table is kept and FTSO/FXRP are shown as a one-line summary.

### FR-16: Flare wallet read panel

- When `[flare.wallet] address` is set to a valid EVM address, Overview shows a read-only panel (native FLR balance, FXRP ERC20 balance, `[flare.fassets] execute` state, whether the executor env key is set).
- When the address is unset, setup guidance is shown (nothing is written to Flare).
- Polling happens only while the Overview tab is shown.

### FR-17: Flare Smart Account (proof-based instruction flow)

- The Flare personal account of an XRPL address (the signing account, or the watch account if none) can be inspected and operated from the TUI alone.
- Read (Flare RPC, read-only): `MasterAccountController` (resolved via FlareContractRegistry) `getPersonalAccount` / `getNonce` / `getXrplProviderWallets` / `getInstructionFee(1)` (FXRP transfer) / `getInstructionFee(2)` (FXRP redeem; both in XRP drops), the personal account's FXRP `balanceOf` (in the token's smallest unit), the FXRP token's `decimals()` (values above 18 are rejected), and whether it is deployed (code present or not). Shown in the FXRP / Flare panel (FXRP Direct Mint) (balance formatted using `decimals()`); `[flare] display = off` and Overview-tab-only polling are treated the same as FTSO / FXRP.
- Write: from Wallet composer `a`, send `0x01` FXRP transfer (value = FXRP smallest unit; the composer converts a decimal FXRP amount to the smallest unit with integer arithmetic only, using the fetched `decimals()`; excess fractional digits or unfetched `decimals` are rejected; recipient = 0x EVM address) and `0x02` FXRP redeem (value = lots) as an XRPL `Payment` (to the operator, no `DestinationTag`, `Amount` = `getInstructionFee` (XRP drops), 32-byte Memo). Execution (FDC proof + `executeInstruction`) is done by the operator on Flare.
- Because the operator comes from a single Flare RPC response, signing is refused unless it matches `[flare.smart_accounts] expected_operator` (which the operator verifies through another channel); unset is also refused. Mainnet requires `--yes`; nothing is signed or submitted before simulate.
- The recipient of C2 (FXRP Mint composer) defaults to the signing account's own personal account (only when the read belongs to the signing account and is within 300 seconds; another account's or a stale read leaves it blank). The composer shows the recipient in full, states that the personal account is a Flare RPC response, and prompts a check against an explorer. "XRP → FXRP in my own personal account" can be completed with an XRPL signature alone (no other Flare address is required).
- Implemented (CLI only): vault instruction (`-x sa-vault`), memo instructions `0xE0` / `0xE1` / `0xE2` / `0xD0` / `0xD1` (`-x sa-memo`), custom instruction `0xFE` (`-x sa-call`), and FDC proof retrieval + `executeDirectMinting[WithData]` (`-x fxrp-execute`, requires a Flare EVM key). After verification, `sa-memo` waits up to 5 minutes for a public executor to finalize (skip with `--no-wait`) and, if not finished, shows how to finalize it yourself. The wait happens after the payment is validated, so a Flare read failure does not fail the command (it prints how to re-check with `sa-status` to stderr, exit 0). When an executor is pinned, `0xE0` / `0xE1` / `0xE2` do not wait and point to finalizing from the pinned executor. `-x sa-status <tx>` only reads `isTransactionIdUsed` (plain recipient memo mints are out of scope; confirmed on Coston2). `sa-call` (`0xFE`) can only be finalized by an executor holding the userOp bytes, so it does not wait for a public executor and states this together with the procedure.
- Finalize depends on a third party on Flare (the executor). That a public executor is running on mainnet (read-only survey on 2026-10-24) does not guarantee that the small, low-fee payment lazyxrp sends will be picked up (the executor's minimum fee is unmeasured). If it is not picked up, the XRP stays in the Core Vault, and finalizing yourself needs an EVM key and FLR (about 20 FLR for mainnet attestation + gas).
- Smart Account state is not persisted. Nothing about the Smart Account is stored. Everything shown (personal account, nonce, operator wallets, instruction fees, FXRP balance, vault positions) is read from Flare on each Overview poll and kept in memory only (`SmartAccountInfo`; a snapshot older than 300 s or from another network is refused, and `verify_*_terms` re-reads the views right before signing — TC-363). `getPersonalAccount(xrplAddress)` is deterministic, so there is nothing to remember, and a stored copy would only go stale (balance, nonce, operator and fees change) while bypassing those guards. The one datum worth persisting would be a user-verified `[flare.smart_accounts] expected_personal_account` pin (like `expected_operator`) to close S-019; it is not implemented (ROADMAP F5-b, P3). lazyxrp persists only the user-written config file and logs; it keeps no wallet, Smart Account or history cache.
- Out of scope (ROADMAP): `0xFF` (userOp inside the memo; the only custom instruction a public executor can relay, so its priority rises for XRPL-signature-only use), and a TUI for `sa-memo` / `sa-call`.

### FR-18: Write via declarative TX spec, and coverage tracking

- XRPL transaction types can be added declaratively as a `TxSpec` (field spec, validation, unsigned `tx_json`, typed signing) and written through one common path (TUI composer "Transaction" `m`, `-x tx <TYPE> Field=value…`, `-x nft-mint` / `-x nft-burn`). The common path reuses the existing write guards (network check, mainnet `--yes`, simulate `tesSUCCESS` required and bound to the request, Fee cap, submit serialization, `--watch` rejection) instead of reimplementing them.
- Targets the 5 NFT types (NFTokenMint / Burn / CreateOffer / AcceptOffer / CancelOffer). URI is plain text ≤ 256 bytes, hex-encoded; TransferFee is 0..=50000 (above 0 requires transferable); id is 64 hex; amounts are `XRP:drops` / `CUR:issuer:value`. Irreversible operations (Burn, AcceptOffer) require typed confirmation in the composer (`BURN` / `ACCEPT`). Assets tab `x` opens the Burn composer for the selected NFT with the NFTokenID pre-filled.
- Support status is managed with `src/coverage.rs` as the SSOT, tracking per type read / TUI write / CLI write / status (Existing, Spec, Planned, Unsupported), and automated tests enforce that it matches the ROADMAP.md "XRPL coverage" table and the counts in the README (`lazyxrp -x coverage` prints the table).

### FR-13: Secret management (Phase 3 groundwork)

- Signing credentials can be loaded with the following precedence:
  1. CLI `--seed` / `--mnemonic`
  2. The matching environment variables `XRPL_SEED` / `XRPL_MNEMONIC`
  3. `[xrpl.signing] seed` / `mnemonic` in the config file `~/.config/lazyxrp/config.toml`
- BIP39 mnemonics support secp256k1, BIP44 account index 0. Existing family seed behavior is preserved.
- The risk of storing credentials in plain text in the config file is documented explicitly.
- Running a write command with no credentials configured returns an explicit error message.
- Write operations using credentials on mainnet show a confirmation prompt before execution.
- The account info of the address derived from the credentials is shown in the TUI Account tab (guidance when unset, an error when invalid).

## 6. Development status

- Phase 1 (read expansion) + Phase 2 (network abstraction + Xahau / Flare multichain): **implemented**
- Phase 3 (write TX): **implemented**: supports Payment / AccountSet / SetRegularKey / OfferCreate / OfferCancel / TrustSet / FXRP Mint / FXRP Execute / AMMDeposit (two-asset) / AMMWithdraw (full or LP amount) / AccountDelete (the TUI requires typing `DELETE` to confirm) in the TUI composer + CLI. EscrowCreate / EscrowFinish / EscrowCancel are TUI composer only (time conditions only, no crypto-conditions). What remains is production verification and edge-case handling.
- Phase 3b (declarative TX spec, FR-18): **5 NFT types implemented**, common path (composer `m` / `-x tx`). Later families (Check / Ticket / DepositPreauth / SignerList / PaymentChannel / AMM) are added with one file under `txspec/` plus one row in `FAMILIES` (`architecture.md` §2.1).

## 7. Scope notes: not yet supported / planned

Pointers only; no new requirements. The ROADMAP is authoritative.

- XRPL transaction types that are not writable: Planned: the Vault* family; Unsupported: types with no `xrpl-rust` typed model, XLS-66 Loan*, witness attestations, ConfidentialMPT*. See ROADMAP "XRPL coverage" (the table SSOT is `ROADMAP.md`, generated by `lazyxrp -x coverage`).
- Flare items deferred: `0xFF`, a TUI for `sa-memo` / `sa-call`, FCC/PMW, OFT, and an own executor bot. See the Flare section of [`ROADMAP.md`](../ROADMAP.md).
