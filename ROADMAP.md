# Roadmap — lazyxrp

[日本語](ROADMAP-ja.md)

**Role:** progress tracking (SSOT for the final goal, priorities and milestones). Planning cautions and pivot log → [`docs/roadmap-log.md`](docs/roadmap-log.md).

**Planning view.** Requirements → [`requirements.md`](docs/requirements.md) · System behavior → [`architecture.md`](docs/architecture.md) · UI/UX → [`DESIGN.md`](DESIGN.md) · Pivot log → [`roadmap-log.md`](docs/roadmap-log.md).

**Current version:** `Cargo.toml` → 0.9.1 (published 2026-10-26)
**Last updated:** 2026-10-28

**Priority:** P0 = release blocker · P1 = main goal of the next milestone · P2 = planned · P3 = spare capacity / demand-driven · "Later" = unscheduled. Within a horizon, work from the top.

## Final goal

Become a popular CLI/TUI tool in the XRPL ecosystem, for developers and users alike. **Keeping security is a non-negotiable condition**: no feature ships that weakens the guards ([`docs/security.md`](docs/security.md)).

Direction (long-term): deepen and spread the XRPL / Xahau / Flare multichain support (install path, onboarding). Work starts after the mid-term items are consumed.

## At a glance

Open work only. Shipped work is in [Shipped history](#shipped-history); unverified shipped behavior is in [Verification debt](#verification-debt).

| Item | Area | Priority | Detail |
|------|------|----------|--------|
| F4-d: confirm on mainnet that a public executor picks up lazyxrp's payments (needs a user decision) | C | P1 | [C](#c-flare-and-fxrp) |
| `0xFF` custom instruction (F4-c; Smart Accounts follow-up) | C | P2 | [C](#c-flare-and-fxrp) |
| Live E2E for Smart Account `0x01` / `0x02` (verification debt) | C | P2 | [Verification debt](#verification-debt) |
| F2-d: FTSO feed validation | B | P3 | [B](#b-uiux-and-display) |
| F5-a: DA-layer `X-API-KEY` header (Proposed) | C | P3 | [C](#c-flare-and-fxrp) |
| F5-b: optional `expected_personal_account` pin to close S-019 (Proposed) | C | P3 | [C](#c-flare-and-fxrp) |
| F5-c: C2 finalization status from AssetManager `DirectMintingExecuted` events (Proposed, blocked) | B / C | P3 | [B](#b-uiux-and-display) |
| Personal account shown in full / copy (Proposed) | B | P3 | [B](#b-uiux-and-display) |
| Vault* family (VaultCreate / Set / Delete / Deposit / Withdraw / Clawback) | A | P3 | [A](#a-xrpl-transactions-and-coverage) |
| Xahau `NetworkID` simulate verification (problems P-007) | A | P3 | [A](#a-xrpl-transactions-and-coverage) |
| Action error type (`String` → typed) | D | Later | [D](#d-quality-security-and-infrastructure) |
| God-file split of `wallet.rs` / `wallet_composer.rs` | D | Later | [D](#d-quality-security-and-infrastructure) |
| Global row cache (A1) — render perf at scale | B | Later | [B](#b-uiux-and-display) |
| Per-panel key configuration (`j` / `k` · `t` · composer) | B | Later | [B](#b-uiux-and-display) |
| `vendored-openssl` feature | D | Later | [D](#d-quality-security-and-infrastructure) |
| Install attestation / signature (today SHA-256 checksum only) | D | Later | [D](#d-quality-security-and-infrastructure) |
| AccountSet GlobalFreeze / RequireAuth confirm step | D | Later | [D](#d-quality-security-and-infrastructure) |

## A. XRPL transactions and coverage

### Shipped

Writable transaction types are **50 / 77** (a write path in the TUI or CLI; the full per-type table is in [XRPL coverage](#xrpl-coverage)). Status **Existing** = 12 hand-written types (Payment, AccountSet, TrustSet, OfferCreate, OfferCancel, SetRegularKey, EscrowCreate / Finish / Cancel, AMMDeposit / AMMWithdraw, AccountDelete); status **Spec** = declarative specs in `src/xrpl/txspec/` for 38 types in the families NFT, XChain, Account objects (Check, Ticket, DepositPreauth, SignerListSet, Clawback), Channel / AMM, MPT and Identity (Credential, DID, Oracle, PermissionedDomain), all through the shared composer "Transaction" + `-x tx` (`-x nft-mint` / `-x nft-burn` are aliases). Adding a family = one file in `txspec/` + one row in `FAMILIES` (steps in `docs/architecture.md` §2.1). Milestones: F-01 AccountDelete (0.5.0, 2026-10-11), EscrowCreate / Finish / Cancel and AMM Deposit / Withdraw (0.4.0, 2026-10-11), review-batch XRPL fixes (2026-10-21). Details in [Shipped history](#shipped-history).

**Live-verification status:** on testnet only NFT mint / burn, AMM, escrow and AccountDelete (plus the earlier Payment / OfferCreate / OfferCancel paths) were validated; the other Spec families are offline-tested only. Xahau: `NetworkID` is injected when `network_id > 1024`, but **Xahau's acceptance of simulate with `NetworkID` is unverified** (problems P-007).

**Fields intentionally omitted from Spec families:** DepositPreauth credentials, Clawback of MPT, NFT `tfMutable`, zero-price offers, `CredentialIDs` on Claim, SignerEntry `WalletLocator`, clearing in DIDSet, MPT mutation flags.

### Open / planned

| Item | Priority | Needs |
|------|----------|-------|
| Vault* family (6 types, status Planned: xrpl-rust 1.3.0 has typed models, not wired) | P3 | Extend the flat Amount grammar so Asset / Amount can be an MPT, then add VaultCreate / Set / Delete / Deposit / Withdraw / Clawback as one family |
| Xahau `NetworkID` simulate verification (P-007) | P3 | A live check against a Xahau endpoint (see [Verification debt](#verification-debt)) |
| Omitted Spec fields (above) | Later | Per-field decision; none is scheduled |

### Not doing (and why)

Unsupported types (no signing possible), grouped by reason:

| Reason | Types | What unblocks it |
|--------|-------|------------------|
| No typed model in xrpl-rust 1.3.0 | NFTokenModify (dynamic NFT), AMMClawback, Batch, DelegateSet, LedgerStateFix | xrpl-rust upgrade that adds the models |
| No typed model in xrpl-rust 1.3.0 (XLS-66 lending) | LoanBrokerSet, LoanBrokerDelete, LoanBrokerCoverDeposit, LoanBrokerCoverWithdraw, LoanBrokerCoverClawback, LoanSet, LoanDelete, LoanManage, LoanPay | xrpl-rust upgrade that adds the models |
| Witness-server signed attestation; cannot be authored in a form | XChainAddClaimAttestation, XChainAddAccountCreateAttestation | Not doable |
| Needs zero-knowledge proofs; no typed model in xrpl-rust | ConfidentialMPTConvert, ConfidentialMPTConvertBack, ConfidentialMPTSend, ConfidentialMPTMergeInbox, ConfidentialMPTClawback | Not doable in a form; also needs an xrpl-rust model |
| Planned, but Asset / Amount may be an MPT, which the flat Amount grammar cannot express | VaultCreate, VaultSet, VaultDelete, VaultDeposit, VaultWithdraw, VaultClawback | Grammar extension (see Open / planned) |

Pseudo-transactions (`EnableAmendment` / `SetFee` / `UNLModify`) are out of scope.

## B. UI/UX and display

### Shipped

Shipped panels and composers (Account, FXRP, FTSO, wallet composers for Payment / Offer / Escrow / AMM / AccountDelete / Transaction / Smart Account, the Watch view) are described in [`DESIGN.md`](DESIGN.md); keys and layout live there, behavior in `docs/architecture.md`. Display work shipped here: Watch / Operate split (`--watch`, 0.6.0), AMM no-pool display (0.6.1), FTSO `Age` staleness column (F2-d, 2026-10-20), FXRP panel `Max now` / `Cap used` / pause badge / vault positions (F1-c, F1-d, F2-c), personal-account prefill in the C2 composer (F4-b, 2026-10-24), review-batch UI fixes (2026-10-21); see [Shipped history](#shipped-history).

### Open / planned

| Item | Priority | Notes |
|------|----------|-------|
| F2-d: FTSO feed validation | P3 | Validate / suggest `FLARE_FEEDS` from the `FastUpdatesConfiguration` feed list (ABI unverified). Feed stability is not on-chain (docs web page only), so out of scope |
| Per-panel key configuration (`j` / `k` · `t` · composer) | Later | Global keys are configurable (DESIGN §6). Move to mid-term if demand appears |
| Global row cache (A1) — render perf at scale | Later | Only if measurement shows a problem |
| God-file split of `wallet.rs` / `wallet_composer.rs` | Later | See [D](#d-quality-security-and-infrastructure) |
| Personal account shown in full / copy (**Proposed**) | P3 | Needs a decision |
| F5-c: finalization status for plain C2 mints in the TUI (**Proposed**, blocked) | P3 | Blocked: the public RPC's `eth_getLogs` caps below 30 blocks, so AssetManager `DirectMintingExecuted` cannot be queried, and `isTransactionIdUsed` does not cover plain mints (F4-a). Needs an explorer API or an archive RPC = a new external dependency → needs a decision |

### Not doing (and why)

- TUI exposure of `sa-memo` / `sa-call`: decided not to (2026-10-24): `sa-memo` is for recovery / pinning, and `sa-call` needs an EVM key or an executor that has the userOp bytes. They stay CLI-only (policy in [C](#c-flare-and-fxrp), reasons in [`docs/roadmap-log.md`](docs/roadmap-log.md)).
- FTSO feed stability display — not on-chain.

## C. Flare and FXRP

### Shipped

Smart Accounts (2026-10-16, 0.7.0): reads (personal account / FXRP balance + `decimals()` / nonce / operator / fee, FXRP panel) + composer `a` (`0x01` transfer · `0x02` redeem as a Payment to the operator with a 32-byte memo; the transfer amount is converted to base units as an integer using the `decimals()` read). Signing only when the operator matches `[flare.smart_accounts] expected_operator`. TC-239–247. Reads were verified live on Coston2 / Flare mainnet (FXRP `decimals()` = 6). **The `0x01` / `0x02` writes are offline-tested only (local JSON-RPC), never run live.** The vault instructions on the same instruction pipeline (`0x11` / `0x12`, `-x sa-vault`) were verified live on Coston2 (F3-c). The rest of the Flare feature set is the adoption plan below (F1–F4, all shipped except the open rows).

### Flare adoption plan (survey of 2026-10-20)

Sources: Flare Developer Hub (`dev.flare.network/llms.txt` index), flare.network news, the vendored `.agents/skills/flare-*`, and live measurements of the Flare mainnet RPC. lazyxrp's state at survey time: FTSOv2 reads (`getFeedsById`) / AssetManagerFXRP Direct Mint reads + C2 (XRPL Payment to the Core Vault + 32-byte memo) + C3 (`executeDirectMinting`, proof pasted by hand) / Smart Account `0x01` transfer and `0x02` redeem. The items below were **not yet introduced** then. Stages follow dependency order; F1 can start without new design decisions.

**Background (2026 changes):** FAssets v1.3 (Songbird first → mainnet, 2026-04-28) changed minting to "Payment straight to the Core Vault + an executor relays the FDC proof", independent of agents, and introduced destination tags (MintingTagManager), executor restrictions, proof binding, time / daily / large-mint limits (delay mode) and `DirectMintingPaymentTooSmallForFee`. Smart Accounts v1.3 (2026-07-28) merges mint + vault deposit into one XRPL signature (Firelight stXRP / Upshift vaults, memo opcodes `0xFE` / `0xFF` / `0xE0`–`0xE2` / `0xD0`–`0xD1`). Firelight went live on 2026-10-06.

| # | Item | Priority | Rationale / done criteria |
|---|------|----------|---------------------------|
| F1-a | ~~Minimum-amount guard for C2~~ **Done** (0.8.0, 2026-10-20) | — | `flare::check_direct_mint_amount`: rejects before simulate unless the amount exceeds `max(min fee, fee_bips share) + executor fee`. TC-330. Confirmed against upstream `DirectMintingFacet._computeFees` (`fee = min(max(received·bips, min), received)`, `executor = min(execFee, received − fee)`, recipient = the remainder → the recipient receives something only when `received > fee + executorFee`). This threshold is the contract's own condition |
| F1-b | ~~Outcome detection for C3~~ **Done** (0.8.0, 2026-10-20) | — | `flare::decode_direct_minting_outcome`: distinguishes minted / delayed (with the re-execution time) / fee forfeited using only events emitted by the AssetManager. Fee forfeited is an Err; delayed is Ok but explicitly "not minted". TC-331. A live delayed case is not reproduced (an amount that hits the mainnet limits cannot be sent) |
| F1-c | ~~Read FXRP pause state + refuse C2~~ **Done** (0.8.0, 2026-10-20) | — | `mintingPaused()` / `emergencyPauseLevel()` (declared in upstream `IAssetManager.sol`; `EmergencyPause.Level` = uint8 NONE / START_OPERATIONS / FULL / FULL_AND_TRANSFER) go into `FxrpDirectMintInfo` (unreadable → `None` = unknown, does not block), a badge in the FXRP panel, and the composer refuses C2 while paused. Mainnet measurement: neither is paused. TC-332–333 |
| F1-d | ~~Pre-check of mint limits (read-only)~~ **Done** (0.8.0, 2026-10-20) | — | In one batch: hourly / daily limit + limiter state (recomputed up to the latest block time by `flare::window_max_now`; checked against `MintingRateLimiter._processElapsedWindows` / `recordMinting` (`>=` delays)), large threshold / delay, granularity, `totalSupply`, `mintingCapAMG` (`getSettings()` has 58 fields, over alloy's tuple limit → read word 22 of the raw bytes). `flare::classify_mint`; the panel shows `Max now` / `Cap used` (est.); the composer rejects only cap overruns (delay is display-only). Mainnet measurement cap 170M XRP / large 4M XRP / 2h matches the docs; a Coston2 smoke receipt (4.8 FXRP) decodes to `Minted` with the declared event ABI. **Known ceiling**: the window width (1 h / 1 d) can be changed by governance via `setWindowSizeSeconds` and has no getter, so fixed values are assumed. TC-334–336 |
| F2-a | ~~MintingTagManager reads~~ **Done** (0.8.0, 2026-10-20) | — | `-x fxrp [--tag N]`: `getMintingTagManager()` → `reservationFee` / `nextAvailableTag` / `executorChangeAfterSeconds`; with `--tag`, `ownerOf` / `mintingRecipient` / `allowedExecutor` / `pendingAllowedExecutorChange` (ABI checked against upstream `IMintingTagManager.sol`; an unreserved tag is `reserved: false` only on revert). Mainnet measurement: next tag 812, cooldown 600 s, tag 30 reserved. No real example of a pending change observed (only `null` confirmed). TC-337 |
| F2-b | ~~Redemption queue and agent list (read-only)~~ **Done** (0.8.0, 2026-10-20) | — | `-x fxrp` reads `redemptionQueue(next, maxRedeemedTickets)` until `next == 0` (up to 100 pages) and shows ticket count, total XRP / lots, the top 10 agents, `getAllAgents` (6 vaults) and the Core Vault (`coreVaultAvailableAmount`). **Consistency check**: Core Vault 141,910,803 XRP + queue 1,541,120 XRP = 143,451,923 XRP ≈ `totalSupply` 143,451,916 XRP (difference about 7 XRP = timing gap between separate RPC calls). Direct mint creates no ticket (`DirectMintingFacet` has no ticket creation), and a Core Vault transfer removes the agent's ticket. `agentRedemptionQueue` has the same content as the overall queue filtered by agent, so it is not implemented separately, and `getAgentInfo` is a struct with 50+ fields of little display value, so it is excluded |
| F2-c | ~~Vault position reads~~ **Done** (0.8.0, 2026-10-20) | — | Vault addresses come from on-chain `MasterAccountController.getVaults()` (id / address / type: 1 = Firelight, 2 = Upshift), so no fixed value or pin is needed. Mainnet measurement: 3 vaults (Firelight ×1, Upshift ×2). The FXRP panel shows the personal account's holdings (`balanceOf` + ERC-4626 `convertToAssets`, or the share count if absent). Unreadable vaults are skipped silently. No real example with a position observed (only confirmed that a zero-balance address gives an empty result). Whether `convertToAssets` exists on Upshift is unverified. TC-339 |
| F2-d | FTSO staleness display **Done** (0.8.0, 2026-10-20). Feed validation not done | P3 | The FTSO panel's `Time` (raw unix seconds) is replaced by `Age` (`2s` / `5m` / `3h`); over 60 s or timestamp 0 is warning-colored (`feed_age`, TC-338). That timestamps are unix seconds and fresh is confirmed on mainnet (all feeds within ±120 s of now, live test). Not done: validating / suggesting `FLARE_FEEDS` from the `FastUpdatesConfiguration` feed list (ABI unverified). Feed stability is not on-chain (docs web page only), so out of scope |
| F3-a | ~~Flare writes for tag-style mint~~ **Done** (0.8.0, 2026-10-20) | — | `-x fxrp-tag-reserve` (`reserve()`, pays `reservationFee` in FLR, shows the tag number from `MintingTagReserved`) / `fxrp-tag-set <tag> [--recipient] [--executor]` / `fxrp-tag-transfer <tag> <to>`. All pre-check the owner, reject the zero address, and send after `estimate_gas` (= simulate); `execute = true` + EVM key + Flare mainnet needs `--yes`. **Never actually sent** (only the read-side refusal paths were confirmed on mainnet: unreserved, FLR balance 0, owner mismatch). TC-341 |
| F3-b | ~~EVM-side redeem~~ **Done** (0.8.0, 2026-10-20) | — | `-x fxrp-redeem <XRP> <r-address> [--tag N]`: `redeemAmount` / `redeemWithTag` (tag is 32 bit). Pre-checks `minimumRedeemAmountUBA` (mainnet measured 5 XRP) and the FXRP balance, sends after `estimate_gas`. **Real send succeeded on Coston2 (2026-10-20)**: `redeemAmount` goes through without approve (FXRP burn Transfer + `RedemptionRequested`, no `RedemptionAmountIncomplete`), settling live that approve is not needed. `redeemWithTag --tag 12345` also succeeded, and the receipt shows `RedemptionWithTagRequested` (topic0 `0x69d3fbff…`, destinationTag = 0x3039 = 12345). The partial-redemption warning path (topic0 of `RedemptionAmountIncomplete`) could not be reproduced live and is unverified. TC-341 |
| F3-c | Smart Account extensions **Done** (only `0xFF` not supported; 0.8.0, 2026-10-20) | — | Vault instructions `0x11`–`0x13` / `0x21`–`0x23` via `-x sa-vault` (existing instruction pipeline; the vault id is registered via `getVaults()` + type check, the fee is `getInstructionFee(id)`; value units confirmed in upstream `Instructions.sol` / `PaymentReferenceParser.sol`: deposit = FXRP base units, redeem = shares, claim = period / `YYYYMMDD`. TC-342), `executeDirectMintingWithData`, the memo recovery instructions `0xE0` / `0xE1` / `0xE2` / `0xD0` / `0xD1` via `-x sa-memo` (TC-344), and the custom instruction `0xFE` via `-x sa-call` (TC-345). **Coston2 live (2026-10-20)**: `0xD1` (public executor finalizes, `isTransactionIdUsed(tx hash)` = true), `0xD0` (pinned my own key, confirmed with `getExecutor`), `0xE2`, `0xE1` (sent after confirming the `--target` mint was done, nonce 0 → 3), `0xE0` (sent against a not-yet-finalized `0xFE` → finalizing `0xE0` first, then the original `0xFE`, mints the FXRP but does not execute the userOp: the nonce stays at 4 and the +1 FXRP from `FXRP.transfer` does not appear at the recipient; the recipient's 1.0 → 1.1 FXRP, +0.1, is the Transfer of the executor fee of 100000 UBA in the original `0xFE`'s finalize receipt), `0xFE` (`FXRP.transfer` executed from the personal account, recipient 5 → 6 FXRP, nonce 3 → 4), `sa-vault firelight deposit 1 1000000` (vault 1 shares 999260, value = FXRP base units), `sa-vault firelight request-redeem 1 500000` (`0x12`. `isTransactionIdUsed` of the sent tx `84EBBD8C…` = true confirmed with the argument. The operator executed it a few minutes later, shares 999260 → 499260, and the burn of 500000 shares and the withdrawal-request event were confirmed in the vault log (tx `0xc1a818f4…`). The balance did not change for about 3 minutes after sending, so the operator's execution lags). The pin of operator `rEyj8nsH…` is **a cross-check, not a match against an official list**: `getXrplProviderWallets()` read through 3 independent Coston2 RPCs agreed, and the account also transacts with many other accounts on XRPL testnet (acceptable on testnet; on mainnet confirm out of band). **Unverified**: `sa-vault` claim (`0x13` waits for the period to pass; the first data word of the withdrawal-request event is `0x214` = 532, a candidate for the period: `sa-vault firelight claim 1 532`; the event's meaning is inferred because the vault ABI is unchecked). **Not supported**: `0xFF` (userOp inside the memo; if demand appears) |
| F3-d | ~~Automatic FDC proof retrieval~~ **Done** (0.8.0, 2026-10-20, user-approved) | — | `-x fxrp-execute <XRPL tx hash> [--data HEX] [--round N]`: verifier `prepareRequest` → `FdcHub.requestAttestation` (mainnet measured **20 FLR**, Coston2 1000 wei) → wait for `Relay.isFinalized(200, round)` (transient RPC disconnects tolerated up to 5 in a row) → DA layer proof → `executeDirectMinting`. The round is shown after attestation, and if it dies midway, `--round N` resumes without paying the fee again. Trust boundary: the verifier response's `abiEncodedRequest` is byte-checked against tx hash / proofOwner / type / source, https only, redirects disabled, response capped at 1 MiB, fee capped at 100 FLR, and the proof is Merkle-verified by the contract. **The whole chain was verified live on Coston2 (2026-10-20)**: a `sa-memo` Payment (XRPL testnet) → attestation → round finalized → DA proof → `executeDirectMintingWithData` from my own key succeeded ("minted 4.9 FXRP … routed to the personal account"). Where a public executor finalizes first, avoid it by pinning myself with `sa-memo set-executor` (after the pin only my key can finalize). `--round` resume was also verified live (no fee). TC-340 |
| F4-a | ~~`sa-memo` finalize wait and `sa-status`~~ **Done** (0.9.0, 2026-10-24) | — | `flare::smart_account_tx_used` (`isTransactionIdUsed`) is polled every 10 s up to 300 s to show whether a public executor finalized. If unfinished it points to `sa-status` and `fxrp-execute <tx> --data 0x`. The wait runs after the payment is validated, so it never fails the command (read errors go to stderr + exit 0). With a pinned executor, `0xE0` / `0xE1` / `0xE2` do not wait and point to the pinned executor. `sa-call` does not wait because an executor without the userOp bytes cannot finalize it. **This view does not work for the plain recipient-memo mint (C2) (on Coston2, a C2 tx finalized by a public executor gave `false`)**, so the TUI only points to checking the FXRP balance / C3 (querying AssetManager `DirectMintingExecuted` is unsuitable because the public RPC's `eth_getLogs` is capped below 30 blocks). `sa-status` also works for `sa-vault` operator instructions (`0x12` gave `true` in F3-c). TC-366. Mainnet `sa-status` was verified by reads (a finalized tx / an unused hash) |
| F4-b | ~~Default the C2 recipient to my own personal account~~ **Done** (0.9.0, 2026-10-24) | — | Prefilled only when the signing account's Smart Account read is within 300 s. Shown in full. **New trust boundary**: the personal account is a single Flare RPC response (no pin) → recorded as **accepted** in security.md S-019 / R-019. TC-367. No separate "side-effect-free mint" op is added (upstream has no pure-mint opcode, and a plain C2 memo + personal-account recipient is enough). That a plain mint to the personal account leaves the FXRP there and is usable via `0x01` / `0x02` is **unverified live on both Coston2 and mainnet** (a mint via `0xFE` was verified on Coston2) |
| F4-c | `0xFF` custom instruction (the follow-up row below) | P2 | A custom path with XRPL signing only, finishable through a public executor. Live Coston2 verification is the precondition |
| F4-d | **Confirm on mainnet that a public executor picks up lazyxrp's payments** | P1 (needs user decision) | Read survey of 2026-10-24 (`docs/test.md`): 905 normal / 10 executors, Smart Account 1000+ / 8 executors, 95–177 s from XRPL finality (8 cases). **Unverified**: the executors' fee floor (whether low-fee payments are picked up), who operates the executors, and whether `0xD1` (no pin) passes on mainnet. Confirming needs a small live send on mainnet. On failure the XRP stays in the Core Vault, and finalizing it myself needs an EVM key and about 20 FLR + gas. If tried on mainnet, use a small C2 (plain mint), which is verified working on Coston2. Avoid `sa-memo remove-executor` because it depends on the unverified `0xD1` |
| F5-a | DA-layer `X-API-KEY` header (the verifier already sends it) (**Proposed**) | P3 | Add when mainnet rate limits are hit (see Policy) |
| F5-b | Optional `[flare.smart_accounts] expected_personal_account` pin to close S-019 (**Proposed**) | P3 | Not implemented; see [Data persistence decision](#data-persistence-decision). Needs a decision |
| F5-c | C2 finalization status from AssetManager `DirectMintingExecuted` events (**Proposed**, blocked) | P3 | Blocked as described in [B](#b-uiux-and-display): needs an explorer API or archive RPC = a new external dependency → needs a decision |

### Open / planned

| Item | Priority | Notes |
|------|----------|-------|
| F4-d mainnet executor probe | P1 | Needs a user decision (see the row above) |
| `0xFF` / F4-c — custom instruction with the userOp inside the memo (1024-byte limit) | P2 | Not supported. **The only custom instruction a public executor can relay without the userOp bytes** (upstream skill: "if you don't want to coordinate with an executor, use `0xFF`"). `0xFE` needs the executor to have the userOp bytes, so an XRPL-signing-only user needs their own EVM key or a trusted executor. Implementation is small (the `-x sa-call` memo just becomes `[0xFF｜wallet｜fee｜userOp]`), but it is not shipped until the 1024-byte limit and the XRP staying in the Core Vault on revert are verified live on Coston2. `docs/architecture.md` §6.9, F4-c. Raised from Later to P2 on 2026-10-24 |
| Live E2E for Smart Account `0x01` / `0x02` | P2 | Verification debt: offline-tested only |
| F5-a / F5-b / F5-c | P3 | Proposed; see the plan table |

### Not doing (and why)

**Policy (2026-10-24; reasons in the same-day entry of [`docs/roadmap-log.md`](docs/roadmap-log.md)):** XRPL signing only is the default path (reads, Smart Account instructions, XRP transfers, mint target = my own personal account); the Flare EVM key stays an optional feature for "finalize it myself" and "EVM-side redeem / tag". Out of scope: FCC / PMW, FLR staking, OFT, a self-run executor bot, and `sa-memo` / `sa-call` in the TUI (`sa-memo` is for recovery / pinning, and `sa-call` needs an EVM key or an executor). Sending an API key to the DA layer (`X-API-KEY`, already sent to the verifier) is added only after mainnet rate limits are hit (F5-a).

**Out of scope (reasons):** FCC (a foundation for developers building TEE extensions, not a client feature) · FLR staking / delegation / FlareDrops / rFLR (off lazyxrp's XRP ↔ FXRP axis) · FXRP OFT bridge sends (the write surface of LayerZero fees, DVNs and per-endpoint rate limits is large; for reads only, `FAssetOFTAdapter` token fee / limit could be added to F2) · gasless FXRP (EIP-712 + relayer, x402) · Web2Json (no source on mainnet / Songbird) · FBTC / FDOGE (other than FXRP the mint path differs).

**Unresolved:** `fsa.flare.network` showed "The FAsset System is currently paused for XRP" as of 2026-10-20, but the same day mainnet AssetManagerFXRP had `mintingPaused()` / `emergencyPaused()` / `emergencyPauseLevel()` all false / 0 (selectors derived with `web3_sha3` and called with `eth_call`). The basis of the banner is unidentified (a separate FSA-side condition or a display lag; `terminated()` reverts and does not exist). The plan does not depend on the pause state, so only the observation is recorded, to be rechecked when F1-c is implemented.

### Data persistence decision

Decision: nothing about the Smart Account is stored; it is re-read from Flare on each Overview poll (rationale and guards: [`docs/architecture.md`](docs/architecture.md) §6.9 "Persistence"). The only datum worth persisting is a user-verified `[flare.smart_accounts] expected_personal_account` pin to close S-019 — not implemented (F5-b, P3).

## D. Quality, security and infrastructure

### Shipped

Audit run-2 (0.3.1, TC-204–214) and its remaining five items (https→http redirect, IPv6-embedded IPv4 / `fec0::/10` SSRF, bidi / format characters, tx-time overflow, `install.sh` https-only), fixed in 0.7.0 (TC-236–238); the review-fix batch (security and correctness, 2026-10-21, TC-346–362); the ponytail audits ×2 (0.6.1); release operations (auto-generated Release body, `mise run release <ver>`, weekly CI for advisory / yanked; procedure in `docs/maintenance.md`); OSS packaging, dependabot and pinned Actions SHAs (0.3.1). See [Shipped history](#shipped-history).

### Open / planned

The remaining items have no defined done criteria, so assess them before starting.

| Item | Priority | Notes |
|------|----------|-------|
| Action error type migration (`String` → typed) | Later | Structural issue in `docs/security.md`. Do it together with making UI wording an SSOT |
| God-file split of `wallet.rs` / `wallet_composer.rs` | Later | **Intentionally deferred** in the review-fix batch |
| `vendored-openssl` feature | Later | **Intentionally deferred** in the review-fix batch |
| Install attestation / signature | Later | **Intentionally deferred** in the review-fix batch (today only a SHA-256 checksum) |
| AccountSet GlobalFreeze / RequireAuth confirm step | Later | **Intentionally deferred** in the review-fix batch |

### Not doing (and why)

Nothing is decided against in this area; the items above are deferred, not rejected. Anything that weakens a guard is excluded by the Final goal.

## Verification debt

Shipped behavior that has not been verified live.

| Item | What is unverified | Needed to close |
|------|--------------------|-----------------|
| Flare mainnet writes | Never sent (C3 mainnet smoke not done; only testnet / Coston2 was validated) | A small live send on mainnet (user decision; needs an EVM key and FLR) |
| `fxrp-tag-*` sends (F3-a) | `-x fxrp-tag-reserve` / `fxrp-tag-set` / `fxrp-tag-transfer` were never actually sent; only the read-side refusal paths were confirmed on mainnet | A live send (Coston2 first) |
| Smart Account `0x01` / `0x02` live E2E | Offline-tested only (local JSON-RPC) | A live transfer / redeem on Coston2 |
| `sa-vault` claim `0x13` | Waits for the period to pass; candidate period 532 (`sa-vault firelight claim 1 532`) is inferred from the event's first data word `0x214`; the event meaning is unchecked against the vault ABI | Wait for the period, then run the claim; check the vault ABI |
| `fxrp-redeem` partial-redemption warning path (F3-b) | Not reproduced live (the topic0 of `RedemptionAmountIncomplete`) | A live partial redemption |
| Delayed mint (`DirectMintingDelayed`) live (F1-b) | Not reproduced (an amount that hits the mainnet limits cannot be sent) | A mint that hits a limit |
| Upshift `convertToAssets` (F2-c) | Whether it exists on Upshift is unverified | Read against an Upshift vault |
| Vault position with non-zero shares (F2-c) | Only an empty result for a zero-balance address was confirmed | A real position |
| Mainnet public-executor pickup of low-fee payments + `0xD1` with no pin (F4-d) | The executors' fee floor, who operates them, and whether `0xD1` (no pin) passes on mainnet | A small live send on mainnet (user decision) |
| Xahau simulate with `NetworkID` (P-007) | Xahau's acceptance of simulate with an injected `NetworkID` (`network_id > 1024`) | A live check against a Xahau endpoint |
| Pending executor change observed (F2-a) | No real example of a pending change observed (only `null`) | A tag with a pending executor change |
| Plain mint to the personal account usable via `0x01` / `0x02` (F4-b) | Unverified on both Coston2 and mainnet (a mint via `0xFE` was verified on Coston2) | A plain C2 mint to the personal account, then `0x01` / `0x02` |

## XRPL coverage

**SSOT is `src/coverage.rs`** (`lazyxrp -x coverage` prints the same table). Do not edit this table by hand; paste the output again; `roadmap_table_matches_registry` (TC-248) fails on any per-row mismatch (Type / Read / TUI / CLI / Status; the Note column is free text). Status: **Existing** = hand-written write path per type · **Spec** = declarative spec in `src/xrpl/txspec/` (the TUI composer "Transaction" + `-x tx` share one path; `-x nft-mint` / `-x nft-burn` are aliases) · **Planned** = a typed model exists in xrpl-rust 1.3.0, not wired · **Unsupported** = no typed model (cannot be signed). Read = a TX detail parser is registered. Pseudo-transactions (`EnableAmendment` / `SetFee` / `UNLModify`) are out of scope.

<!-- xrpl-coverage:start -->
| Type | Read | TUI write | CLI write | Status | Note |
|------|------|-----------|-----------|--------|------|
| Payment | ✓ | ✓ | ✓ | Existing |  |
| AccountSet | ✓ | ✓ | – | Existing |  |
| TrustSet | ✓ | ✓ | – | Existing |  |
| OfferCreate | ✓ | ✓ | ✓ | Existing |  |
| OfferCancel | ✓ | ✓ | ✓ | Existing |  |
| SetRegularKey | ✓ | ✓ | – | Existing |  |
| EscrowCreate | ✓ | ✓ | – | Existing |  |
| EscrowFinish | ✓ | ✓ | – | Existing |  |
| EscrowCancel | ✓ | ✓ | – | Existing |  |
| AMMDeposit | ✓ | ✓ | ✓ | Existing |  |
| AMMWithdraw | ✓ | ✓ | ✓ | Existing |  |
| AccountDelete | – | ✓ | ✓ | Existing |  |
| NFTokenMint | ✓ | ✓ | ✓ | Spec | NFT |
| NFTokenBurn | ✓ | ✓ | ✓ | Spec | NFT |
| NFTokenCreateOffer | ✓ | ✓ | ✓ | Spec | NFT |
| NFTokenAcceptOffer | ✓ | ✓ | ✓ | Spec | NFT |
| NFTokenCancelOffer | ✓ | ✓ | ✓ | Spec | NFT |
| XChainCreateBridge | ✓ | ✓ | ✓ | Spec | XChain |
| XChainModifyBridge | ✓ | ✓ | ✓ | Spec | XChain |
| XChainCreateClaimID | ✓ | ✓ | ✓ | Spec | XChain |
| XChainCommit | ✓ | ✓ | ✓ | Spec | XChain |
| XChainClaim | ✓ | ✓ | ✓ | Spec | XChain |
| XChainAccountCreateCommit | ✓ | ✓ | ✓ | Spec | XChain |
| CheckCreate | ✓ | ✓ | ✓ | Spec | Account objects |
| CheckCash | ✓ | ✓ | ✓ | Spec | Account objects |
| CheckCancel | ✓ | ✓ | ✓ | Spec | Account objects |
| TicketCreate | ✓ | ✓ | ✓ | Spec | Account objects |
| DepositPreauth | ✓ | ✓ | ✓ | Spec | Account objects |
| SignerListSet | ✓ | ✓ | ✓ | Spec | Account objects |
| Clawback | ✓ | ✓ | ✓ | Spec | Account objects |
| PaymentChannelCreate | ✓ | ✓ | ✓ | Spec | Channel / AMM |
| PaymentChannelFund | ✓ | ✓ | ✓ | Spec | Channel / AMM |
| PaymentChannelClaim | ✓ | ✓ | ✓ | Spec | Channel / AMM |
| AMMCreate | ✓ | ✓ | ✓ | Spec | Channel / AMM |
| AMMVote | ✓ | ✓ | ✓ | Spec | Channel / AMM |
| AMMBid | ✓ | ✓ | ✓ | Spec | Channel / AMM |
| AMMDelete | ✓ | ✓ | ✓ | Spec | Channel / AMM |
| MPTokenIssuanceCreate | ✓ | ✓ | ✓ | Spec | MPT |
| MPTokenIssuanceDestroy | ✓ | ✓ | ✓ | Spec | MPT |
| MPTokenIssuanceSet | ✓ | ✓ | ✓ | Spec | MPT |
| MPTokenAuthorize | ✓ | ✓ | ✓ | Spec | MPT |
| CredentialCreate | ✓ | ✓ | ✓ | Spec | Identity |
| CredentialAccept | ✓ | ✓ | ✓ | Spec | Identity |
| CredentialDelete | ✓ | ✓ | ✓ | Spec | Identity |
| DIDSet | ✓ | ✓ | ✓ | Spec | Identity |
| DIDDelete | ✓ | ✓ | ✓ | Spec | Identity |
| OracleSet | ✓ | ✓ | ✓ | Spec | Identity |
| OracleDelete | ✓ | ✓ | ✓ | Spec | Identity |
| PermissionedDomainSet | ✓ | ✓ | ✓ | Spec | Identity |
| PermissionedDomainDelete | ✓ | ✓ | ✓ | Spec | Identity |
| VaultCreate | – | – | – | Planned | Single-asset vault: Asset may be an MPT, which the flat Amount grammar cannot express |
| VaultSet | – | – | – | Planned | Single-asset vault: deferred as a family with VaultCreate (MPT-capable assets) |
| VaultDelete | – | – | – | Planned | Single-asset vault: deferred as a family with VaultCreate (MPT-capable assets) |
| VaultDeposit | – | – | – | Planned | Single-asset vault: Amount may be an MPT, which the flat Amount grammar cannot express |
| VaultWithdraw | – | – | – | Planned | Single-asset vault: Amount may be an MPT, which the flat Amount grammar cannot express |
| VaultClawback | – | – | – | Planned | Single-asset vault: Amount may be an MPT, which the flat Amount grammar cannot express |
| NFTokenModify | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 (dynamic NFT) |
| AMMClawback | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 |
| Batch | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 |
| DelegateSet | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 |
| LedgerStateFix | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 |
| LoanBrokerSet | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 (XLS-66 lending) |
| LoanBrokerDelete | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 (XLS-66 lending) |
| LoanBrokerCoverDeposit | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 (XLS-66 lending) |
| LoanBrokerCoverWithdraw | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 (XLS-66 lending) |
| LoanBrokerCoverClawback | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 (XLS-66 lending) |
| LoanSet | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 (XLS-66 lending) |
| LoanDelete | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 (XLS-66 lending) |
| LoanManage | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 (XLS-66 lending) |
| LoanPay | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 (XLS-66 lending) |
| XChainAddClaimAttestation | – | – | – | Unsupported | witness-server signed attestation; cannot be authored in a form |
| XChainAddAccountCreateAttestation | – | – | – | Unsupported | witness-server signed attestation; cannot be authored in a form |
| ConfidentialMPTConvert | – | – | – | Unsupported | needs zero-knowledge proofs; no typed model in xrpl-rust |
| ConfidentialMPTConvertBack | – | – | – | Unsupported | needs zero-knowledge proofs; no typed model in xrpl-rust |
| ConfidentialMPTSend | – | – | – | Unsupported | needs zero-knowledge proofs; no typed model in xrpl-rust |
| ConfidentialMPTMergeInbox | – | – | – | Unsupported | needs zero-knowledge proofs; no typed model in xrpl-rust |
| ConfidentialMPTClawback | – | – | – | Unsupported | needs zero-knowledge proofs; no typed model in xrpl-rust |
<!-- xrpl-coverage:end -->

Writable **50 / 77** (a write path in the TUI or CLI; the remaining Planned (Vault family) and Unsupported types are explained in the table's Note column and in [A](#a-xrpl-transactions-and-coverage)). Adding a new family: 1 family = 1 file in `txspec/` + 1 row in `FAMILIES`; steps in `docs/architecture.md` §2.1.

## Shipped history

Chronological. Mid-term (0.4.0), short-term and shipped items are merged here; the Flare adoption items F1–F4 keep their full rows in [C](#c-flare-and-fxrp). A strikethrough marks a finished roadmap item.

| Date | Version | Item | Notes |
|------|---------|------|-------|
| — | 0.1.23 | Quality pass | **Done** |
| — | — | Agent/docs hygiene | **Done** |
| 2026-09-12 | 0.2.0+ | Wallet TX + FXRP | **Done**. C3 mainnet smoke → Mid-term (P2) |
| — | — | Docs / agent hygiene | **Done**. Human doc index in README; `docs/` self-contained |
| — | — | Ratatui optimization | **Done**. dirty render + shared table shipped |
| 2026-09-12 | — | Docs hierarchy redesign | **Done**. keys → `DESIGN.md`; architecture = behavior; removed `docs/README` + `docs/tasks` |
| 2026-09-12 | 0.2.5+ | Network multichain | **Done**. Xahau + `<Ctrl-n>` session switch + Flare display + wallet read panel |
| 2026-09-16 | 0.2.9 | Test coverage / local integration pass | **Done**. Local RPC read-only CLI coverage, `lazyxrp`/`rp` binary smoke tests, Assets and panel state coverage |
| 2026-09-17 | 0.2.10 | Review batch #3: production guard + test coverage | **Done**. Xahau production write guard, local CLI RPC migration, nft_image SSRF hardening, binary smoke tests, alloy 2 / MSRV 1.94.1 |
| 2026-09-17 | 0.2.11 | Flare read-path stabilization | **Done**. Batched FTSOv2 `getFeedsById` (5→1 RPC/poll), fee-aware retry, `FXRP/USD`→`XRP/USD` premap, AssetManager/wallet reads via `try_join!`, flare-only failure cooldown (2→60s) decoupled from XRPL backoff |
| 2026-09-17 | 0.2.11 | XRPL poll efficiency pass | **Done**. dUNL excluded from XRPL backoff both ways + stale-on-error with 60 s retry cooldown; Account-tab panels (`account_tx`, wallet overview) gated like Market/Assets |
| 2026-09-17 | 0.2.11 | Status bar health tracks | **Done**. XRPL chip goes STALE during quiet backoff + OFFLINE on `server_info` errors; flare dot (`flr:`) with red-on-failure/orange-stale; `dunl:` data-age marker with `!` past cache TTL |
| 2026-10-10 | 0.3.0 | DEX trading + CLI redesign + keymap docs | **Done**. Wallet composer OfferCancel (`8`) + Payment Send max / `p` path quote (`slippage_bps`); CLI: bare subcommands + `watch` removed, `-x send` simulate-first + non-zero exit on failure, `-x offers` / `-x offer-cancel`; shared Payment validation (P-004); live `simulate` LLS fill (TC-200); `--seed`/`--mnemonic` kept with stderr warning (FR-13); global keybindings user-mergeable (DESIGN §6); `rc` alias stays README-only |
| 2026-10-11 | 0.3.1 | Live verification + testnet DX + audit run-2 | **Done**. `-x faucet` / `-x offer-create`, CLI write waits for validated, endpoint `network_id` check (S-017), opt-in testnet E2E, audit run-2 fixes (TC-204–214, the Core Vault pin is a breaking change), OSS packaging, dependabot + Actions SHA pinning |
| 2026-10-11 | 0.3.1 | Mid-term #2 — ~~Sequence / LastLedgerSequence consistency check~~ | **Done** (0.3.1, audit run-2). Source: the residual of the Fee cap section in `docs/security.md`. P2. TC-204 |
| 2026-10-11 | — | Short-term #1 — ~~Testnet E2E — TUI~~ | **Done** (tmux-driven). OfferCreate (immediate fill), a non-filling OfferCreate → OfferCancel, and `p` quote → cross-currency Payment (1 USD received) validated. Hashes in `docs/test.md` Testnet E2E |
| 2026-10-11 | — | Short-term #2 — ~~Release operations optimization~~ | **Done**. The Release body is auto-generated (commits since the previous tag + GitHub generated notes), `mise run release <ver>` (bump + lock sync + gates + one commit), weekly CI (advisory / yanked). Procedure in `docs/maintenance.md` |
| 2026-10-11 | — | Short-term #3 — ~~Backfill of the v0.3.0 Release body~~ | **Done**. Breaking changes, migration link and commit list filled in |
| 2026-10-11 | — | Mid-term #1 — ~~`poll.rs` split~~ | **Done**. Source: `docs/security.md` structural issue. P1. Write path (`submit_*`, `simulate_then_finalize`, quote and related tests) moved to `xrpl/submit.rs` (1,526 lines); `poll.rs` 2,633 → 1,120 lines. Behavior unchanged (260 tests green, no new TC) |
| 2026-10-11 | — | Mid-term #3 — ~~B-02 EscrowCreate composer UI~~ (F-03 Finish/Cancel pulled forward) | **Done**. Source: `docs/problems.md` P-003. P2. Signing was in fact unimplemented too (poll silently dropped it). Signing + `submit_escrow_*` + composer `9` Create (relative time `90s`–`7d`, CancelAfter optional) + Finish/Cancel from the `0` list, TUI only. TC-215–221; Create ×2 / Cancel / Finish validated on testnet |
| 2026-10-11 | — | Mid-term #4 — ~~F-04 AMM Deposit/Withdraw TX~~ | **Done**. Follow-up of DEX trading. P2. Two-asset deposit, withdraw all / by LP amount. Composer `d` / `w` + `-x amm-deposit` / `-x amm-withdraw`. TC-222–226; CLI ×2 / TUI ×2 validated on testnet (pools created by a throwaway script; AMMCreate was out of scope) |
| 2026-10-11 | — | Mid-term #5 — ~~Fake WebSocket integration fixtures~~ | **Done**. Source: `docs/test.md`. P2. TC-227. The fixture found a real bug: `xrpl_receive` dropped the `type` of stream messages, so ledger close / account tx WS events were never delivered (it only worked via interval poll). Fixed to read raw frames; ledger close reception confirmed on testnet. The panel-side reflection is covered by the existing TCs (account.rs) |
| 2026-10-11 | — | Mid-term #6 — ~~Double-submit guard (consecutive submits with identical content)~~ | **Done**. Source: unfixed list in `docs/security.md`. P3. TC-228. One exit point in WalletPanel rejects a write with identical params until its result (Ok/Err) arrives. CLI is out of scope because it waits for validated |
| 2026-10-11 | — | Mid-term #7 — ~~`account_offers` marker paging (> 200 entries)~~ | **Done**. Source: ponytail note in `client.rs`. P3. TC-229. Follows the marker at 200 entries/page (up to 50 pages). Both TUI OfferCancel and `-x offers` |
| 2026-10-11 | — | Mid-term #8 — ~~C3 execute smoke (`execute=true`)~~ | **Done** (testnet). P3. By user decision real funds were on testnet (XRPL testnet + Coston2). Verification found and fixed the C2 simulate rejection (memo null, TC-230) and a C3 ABI mismatch (nonexistent selector, TC-231); C2 → mint by a public executor and a 48-byte memo → lazyxrp C3 confirmed FXRP 9.8 → 14.7. Mainnet was not done |
| 2026-10-11 | 0.4.0 | write foundation cleanup + TX expansion | **Done**. `poll.rs` → `submit.rs` split, EscrowCreate / Finish / Cancel (TUI), AMM Deposit / Withdraw (TUI + `-x`), parse fix for the real `amm_info` response shape (TC-015). Escrow ×4 and AMM ×4 validated on testnet. Order of dependency was: `poll.rs` split → EscrowCreate / AMM (new submit kinds after the split) |
| 2026-10-11 | 0.4.1 | Mid-term #5–#7 | **Done**. WS event non-recognition fix (TC-227), double-submit guard (TC-228), `account_offers` paging (TC-229) |
| 2026-10-11 | 0.4.2 | C3 testnet smoke | **Done**. FXRP C2 simulate null-rejection fix (TC-230), C3 ABI fix (TC-231). Mint / execute confirmed on XRPL testnet + Coston2 |
| 2026-10-11 | 0.5.0 | AccountDelete (long-term F-01 — TX surface completion) | **Done**. Composer `x` + `-x account-delete`; the TUI requires typing `DELETE` to confirm (TC-232–233). The Fee (owner reserve) comes from simulate. TUI (with an X-address tag) / CLI validated on testnet |
| 2026-10-11 | 0.6.0 | Watch / Operate split | **Done**. `--watch` read-only launch (TC-234–235). User choice: the `--watch` flag. All keys are discarded at startup and only addresses are shown; the TUI shows the Account panel (no composer, no key generation) and signing `-x` commands are refused |
| 2026-10-11 | 0.6.1 | AMM no-pool display + ponytail audits ×2 | **Done**. For a pair without a pool, the panel is shown without an error in the status bar. 7 dependencies removed (`config`, `signal-hook`, `tracing-error`, `strum`, `url`, `time`, build-dep `anyhow`), behavior unchanged |
| 2026-10-16 | 0.7.0 | Flare Smart Accounts — proof-based FXRP transfer / redeem | **Done**. See [C](#c-flare-and-fxrp) Shipped (TC-239–247; `0x01` / `0x02` writes offline-tested only) |
| 2026-10-20 / 2026-10-24 | 0.8.0 / 0.9.0 | Flare adoption plan F1–F4 | **Done** except F4-c / F4-d. Full rows in [C](#c-flare-and-fxrp) (TC-330–345, TC-366–367) |
| 2026-10-21 | 0.8.0 | Review-fix batch (security and correctness) | **Done**. Flare: `eth_chainId` verification (`expected_chain_id` / `check_write_chain` / `verify_rpc_chain`; the `--yes` decision uses the reported chain id; once at the start of every Flare subcommand), the TUI C3 is also gated on the Flare network (`PollContext.flare_network`), `check_network_pair` moved into `flare.rs` and applied to TUI C2 / Smart Account / C3, 10 FLR fee cap via `send_capped` (`MAX_TX_FEE_WEI`), FDC pays only after the round parameter is confirmed (tx hash + `--round` resume hint), EIP-55 enforced, `tag_owner` treats only a revert as unreserved, cap checks for `sa-memo` / `sa-call`, unset oracle → `XrplOracleNotConfigured`. XRPL: write params carry the compose-time `network` (`stale_write_refusal`), `submit_signed_tx` is single-shot + `tx(hash)` lookup, `terQUEUED` = queued success, ask price 1e6/q fix, `NetworkID` injected when `network_id > 1024` (**Xahau's acceptance of simulate is unverified**, problems P-007), WS 30 s read timeout + reconnect on network switch, base58 / hex in toml validation, activation = `account_info` success, dUNL VL / field-id, currency display rules, strict `parse_drops`, IPv6 NFT URI. Config: keybinding allowlist (`is_bindable`), permission check of the file actually read, `~` expansion, credential precedence (CLI > env > file), zeroize in signing.rs, release `overflow-checks = true`, RUSTSEC-2026-0235 ignore removed (rkyv is not in Cargo.lock), `install.sh` does not install Rust by default for a binary install (`--install-rust`). UI: keys / paste only on the visible tab, no wallet in Overview, TrustSet starts empty, NoFreeze needs typing `NOFREEZE`, the Flare snapshot is cleared on NetworkChange and refused after 300 s, IOU currency field 40 characters, splash exits on a `server_info` error, fetch dedup (TC-346–362) |
| — | 0.7.0 | Remaining audit run-2 items (5) | **Done**. https→http redirect, IPv6-embedded IPv4 / `fec0::/10` SSRF, bidi / format characters, tx-time overflow, `install.sh` https-only (TC-236–238) |
| 2026-10-28 | Unreleased | Review + fixes + dependency update | **Done**. Repo-wide comment / test / doc review; TUI lost-keystroke race fixed (event stream starts after component `init`); CLI `book` / `amm` reads and `offer-create` / `amm-deposit` / `amm-withdraw` specs accept currency symbols such as `RLUSD` (TC-368–369); `kobe-xrpl` 4 / `kobe-core` 4, `vergen-gix` 10, MSRV 1.99. `mise run e2e-testnet` green |

Long-term items shipped: F-01 AccountDelete (0.5.0, 2026-10-11) with composer `x` + `-x account-delete`, TC-232–233; Watch / Operate split (0.6.0, 2026-10-11), TC-234–235.

## Task tracking

| Concern | SSOT |
|---------|------|
| Goal hierarchy + priorities + shipped history | This file |
| Planning cautions + pivot log | `docs/roadmap-log.md` |
| Doc index (doc map) | Root `AGENTS.md` § Progressive disclosure |

## How to add work

1. Pick the area (A–D) and add a row to this file, with its priority; add it to [At a glance](#at-a-glance) if it is open.
2. New requirement → `docs/requirements.md` (FR/NFR).
3. Shipped behavior / network / config rules → `docs/architecture.md` (+ TC in `docs/test.md`).
4. Keys, layout, colors, modals → root [`DESIGN.md`](DESIGN.md).
5. Pivots and planning cautions → [`docs/roadmap-log.md`](docs/roadmap-log.md).
6. New coverage rows come from `lazyxrp -x coverage` (paste the output between the markers; never hand-edit the table).
7. Bilingual rule: every `docs/*.md` and `ROADMAP.md` has a Japanese twin `*-ja.md`; the English file is canonical for agents; any change to one must be made to the other in the same change; ID sets (TC/FR/NFR/S/R/I/P/F) must match.
