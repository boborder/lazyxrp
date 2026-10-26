# security.md

[日本語](security-ja.md)

**Role:** Security design and secret handling (review records for S-xxx / R-xxx and I-1 to I-11).

Generated: 2026-05-01 | Scope: Full codebase (`src/`, `install.sh`, `docs/`)

This file tracks **security review findings (S-xxx)** and **implementation risks (R-xxx)** (table below). Invariant IDs (I-1 to I-11) are referenced across `docs/` and the code:

| ID | Invariant | Enforced in |
|----|-----------|-------------|
| I-1 | Plaintext `seed` / `mnemonic` are cleared to `None` in `Config::new()`, leaving only `SecretString`. `lib.rs::run` moves them via `Config::take_secrets()` into `Secrets` (seed / mnemonic / Flare EVM key) before sharing the `Arc<Config>` (the shared config holds no secrets) | `config.rs` / `lib.rs` |
| I-2 | A write on mainnet (`Network::is_production()`) requires `--yes` or `skip_mainnet_prompt: true` | each submit in `poll.rs` / `submit.rs` |
| I-3 | A write always goes simulate → `tesSUCCESS` → fetch `Sequence`/`Fee` → sign → submit. Never sign without simulating | `poll.rs` / `submit.rs` |
| I-4 | `XRPL_SEED` / `XRPL_MNEMONIC` are taken out once by `Config::new()` before the poll / UI tasks are spawned, and removed from the env (`take_secret_env`). `remove_var` never runs afterwards. On Linux the startup `/proc/self/environ` block is not rewritten and stays visible to readers with the same uid (known limitation) | `config.rs` |
| I-5 | Config precedence: CLI > env > user `config.toml` > built-in `config.json5`. For credentials, once a higher source sets either seed or mnemonic, both from lower sources are discarded (setting both at the same level is an error in `credential_from_secrets`) | resolve functions in `lib.rs` + `config.rs` (`layer_credentials`, `apply_cli_credentials`) |
| I-6 | The periodic poll never goes below `MIN_POLL_INTERVAL` (10 s) | `poll.rs` `drive_poll_loop` |
| I-7 | `account_tx` not-found is an empty list (not an error) | `poll.rs` |
| I-8 | `Tui::drop` restores the terminal and does not panic during drop | `tui.rs` |
| I-9 | `TAB_TITLES` matches the panels in count and order (TC-060). When adding a panel, update it in the same change | `app.rs` |
| I-10 | Never `Arc::make_mut` the `ArcValue` JSON of a `TxRow` | `xrpl/types.rs` (convention) |
| I-11 | Every RPC is wrapped in `tokio::time::timeout(RPC_TIMEOUT, …)` | `poll.rs` / `client.rs` |
| — | Signing uses a family seed **or** a BIP39 mnemonic (`credential_from_secrets`). A mnemonic is never converted into a seed | `signing.rs` |

## S-xxx ↔ R-xxx cross-reference

| S-ID | Topic | Related R-ID | Notes |
|------|-------|--------------|-------|
| S-001 | Seed or mnemonic in `Debug` output | R-001 | Plaintext logging; secrets live in `Secrets` (via `Config::take_secrets()`), never in the shared `Arc<Config>` |
| S-002 | Signing env var not cleared | R-001, I-4 | `Config::new()` removes `XRPL_SEED` and `XRPL_MNEMONIC` |
| S-003 | Config file credential permissions | R-001 | Unix warn on group/world read |
| S-010 | Plaintext credential retained on `Config` | R-001 | Plaintext cleared in `Config::new()`; secrets moved out by `take_secrets()` before `Arc<Config>` |
| S-011 | CLI credentials visible in `ps` | R-001 | Prefer env/file; startup warning |
| S-006 | `Tui::drop` panic / raw mode | R-005, I-8 | `eprintln!` on `exit()` failure |
| S-009 | `--self-uninstall` data deletion | — | User-driven; not an R entry |
| S-004, S-005, S-007, S-008 | R-007 (merge), — | See table below |
| S-012, S-013 | R-012 | NFT DNS resolution and metadata bounds |
| S-014 | R-013 | Wallet submit serialization |
| S-015 | R-014 | Plaintext RPC with signing seed |
| S-016 | R-015 | Issuer and financial input validation |
| S-017 | R-016 | Endpoint network ≠ selected network at signing time |
| S-018 | R-017 | Smart Account operator (payment destination) and fee come from one Flare RPC answer |
| S-019 | R-019 | FXRP Mint recipient prefilled from a Flare RPC answer (`getPersonalAccount`) |

Risks without a matching S entry (e.g. R-006 mainnet guard bypass, R-008 RPC 429) are tracked in the R-xxx table below and [`test.md`](test.md).

## R-xxx implementation risks

| R-ID | S-ID (if any) | One-line |
|------|---------------|----------|
| R-001 | S-001, S-002, S-010, S-011 | Seed resolution / `Secrets` vs cleared `seed` |
| R-002 | — | Submit errors dropped on closed `action_tx` |
| R-003 | — | `ArcValue` shared JSON mutation |
| R-004 | — | Unbounded channel growth |
| R-005 | S-006 | TUI Drop / terminal raw mode |
| R-006 | — | Mainnet / **Xahau mainnet** `--yes` guard bypass |
| R-007 | — | Config merge precedence per-key |
| R-008 | — | RPC 429 / backoff |
| R-009 | — | Submit hash not verified |
| R-010 | — | Duplicate poll on ledger close |
| R-011 | — | Poll `RpcClient::connect` “instant death” (**accepted / non-issue**) |
| R-012 | S-012, S-013 | NFT URI DNS SSRF / metadata recursion (**mitigated**) |
| R-013 | S-014 | Concurrent wallet submissions (**mitigated in poll task**) |
| R-014 | S-015 | Insecure RPC with signing seed (**mitigated**) |
| R-015 | S-016 | Unvalidated issuer and financial numeric inputs (**mitigated**) |
| R-016 | S-017 | Custom endpoint on another network bypasses the `--yes` guard (**mitigated**, TC-203) |
| R-017 | S-018 | Smart Account instruction Payment to an RPC-supplied operator / fee (**mitigated**, TC-240 / TC-243) |
| R-019 | S-019 | C2 recipient default = RPC-supplied personal account (**accepted**: shown in full, editable, user checks it; TC-367) |
| R-018 | — | Spec'd (generic) write path: guards live only in shared helpers; irreversible types must set `confirm` (**mitigated**, TC-088 table / TC-252–255) |

## 2026-10-16 Declarative TX spec write surface (NFT family, R-018)

- **New write surface**: `Action::GenericTxSubmit` → `PollCommand::GenericTx` → `submit_generic_transaction` (`-x tx` / `nft-mint` / `nft-burn` go through `cli_exec` to the same `simulate_confirm_submit`). Every spec'd type goes through this single path, so the guards live only in shared helpers instead of being duplicated per type: `TxSpec::prepare` (field validation, rejection of unknown fields, `Time` fixed once to absolute seconds) → mainnet `--yes` (`run_submit` / `resolve_submit_wallet`) → `ensure_network` → `account_info` → simulate `tesSUCCESS` and binding to the request (`check_simulate_bound_to_request`) → Fee cap → sign → submit, all under the poll's `submit_lock`. `--watch` is rejected by `Cmd::signs`.
- **Confirmation rule for irreversible operations**: irreversible types (NFTokenBurn = destroys the NFT, NFTokenAcceptOffer = asset transfer and payment) carry a confirmation phrase (`BURN` / `ACCEPT`) in `TxSpec::confirm`. The TUI composer does not queue directly on `s`; it always goes through the same type-to-confirm (`DangerConfirm`) as AccountDelete (partial input is rejected, Esc returns to the form). The CLI uses the mainnet confirmation prompt / `--yes` and shows the simulate result. **When adding an irreversible type in a future family, always set `confirm`** (a type without it goes to signing with one key press).
- **Input trust boundary**: a URI is hex-encoded from plaintext, so it cannot be arbitrary bytes (≤ 256 bytes). ids are 64 hex, addresses are checksummed, amounts are validated by the existing `parse_offer_amount` (0 and non-finite are rejected), and the composer and poll use the same `TxSpec::validate`. The Assets-tab `x` prefill is the `nft_id` of the displayed NFT row (node-derived), but it always passes the form confirmation and simulate.
- **Residual**: the NFT list belongs to the watched account, which is not necessarily the signing account (if the owner differs, simulate rejects with `tec*`). `tfMutable` mint and 0-price offers are unsupported (crate / shared parser limitation).
- **Account objects family (Check / Ticket / DepositPreauth / SignerListSet / Clawback)**: CheckCash (`CASH`), which can transfer assets, SignerListSet (`SIGNERS`: a wrong list plus a disabled master key locks the account) and Clawback (`CLAWBACK`), which are irreversible or risk lockout, require a typed confirmation phrase. SignerEntries validates `addr:weight` (≤ 8 entries, weight 1..65535, no duplicates, quorum ≤ total weight). DepositPreauth credentials and Clawback MPT are unsupported.
- **XChain family (XChainCreateBridge / Commit / Claim / AccountCreateCommit)**: creating a bridge (there is no delete tx) and Commit / Claim / AccountCreateCommit, which lock or burn assets, require a typed confirmation (`BRIDGE` / `COMMIT` / `CLAIM`). `XChainBridge` strictly validates a flat 4-element string (checksum, distinct doors, XRP/IOU mix rejected), and the Commit / Claim Amount must match the bridge's issue. Witness attestation is unsupported because the form cannot produce the signature.
- **Channel / AMM family (PaymentChannelCreate / Fund / Claim, AMMCreate / Bid / Delete)**: types that lock XRP, transfer assets, consume LP tokens or are irreversible require a typed confirmation (`LOCK` / `FUND` / `CLAIM` / `CREATE` / `BID` / `DELETE`); only AMMVote has no confirmation. Claim's `Signature` / `PublicKey` are only hex-validated pasted input and no signature is generated locally (`CredentialIDs` is unsupported). drops is an integer in 1..=1e17, AuthAccounts is ≤ 4 entries with no duplicates and the signer itself is rejected. AMMCreate burns a special Fee equal to the owner reserve, so the Fee cap guard (shared path) applies.
- **MPT / Identity family (MPTokenIssuanceDestroy, CredentialDelete, DIDDelete, OracleDelete, PermissionedDomainDelete)**: irreversible deletions require a typed confirmation (`DESTROY` / `DELETE`). Credential / DID / Oracle blobs are hex-encoded from plaintext (arbitrary bytes cannot be entered directly), prices are decimal u64 only, and a zero-hash Domain ID is rejected. MPT Set (lock / unlock), which needs no confirmation, and Authorize are reversible. Vault* and MPT mutation / confidential types are unsupported (reasons in `coverage.rs`).

## 2026-10-24 C2 recipient default and waiting for finalize (S-019 / R-019)

- **New trust boundary (S-019)**: to complete "XRP → FXRP in my own personal account" with only an XRPL signature, the FXRP Mint composer fills the recipient with the signing account's personal account (the `getPersonalAccount` answer). This value is a single Flare RPC answer with no pin like the Core Vault / operator has, so a malicious / compromised RPC can return an attacker's address and take over the mint destination (the Core Vault is pinned so the XRP destination does not change, but the FXRP recipient does). **Mitigation**: the value is prefilled only when the read belongs to the signing account and is within 300 seconds, all digits are shown with an explicit note "this is a Flare RPC answer; check it in an explorer", it is editable, and `verify_direct_mint_terms` right before sending is unchanged. **Residual (accepted)**: trusting the RPC itself is the same premise as S-015; there is no cross-check against a second independent source and no pin (`expected_personal_account`). If concerned, type the recipient by hand or use a trusted RPC (`FLARE_RPC_URL`). The only mitigation beyond display + user check that was considered is an optional `[flare.smart_accounts] expected_personal_account` pin, which is not implemented (ROADMAP F5-b, P3).
- **Smart Account data is not persisted**: nothing about the Smart Account is stored. Everything shown (personal account, nonce, operator wallets, instruction fees, FXRP balance, vault positions) is read from Flare on each Overview poll and kept in memory only (`SmartAccountInfo`; a snapshot older than 300 s or from another network is refused, and `verify_*_terms` re-reads the views right before signing — TC-363).
- **Finalize is a third party's transaction**: the Flare side of a mint (`executeDirectMinting[WithData]`) is performed by an executor. A mainnet read-only investigation (2026-10-24, `docs/test.md`) found public executors running (normally 905 txs / 10 parties, Smart Account 1000+ txs / 8 parties, 95–177 s after XRPL finality), but there is no guarantee that a low-fee payment will be picked up (the fee floor is unmeasured). If it is not picked up, the XRP stays in the Core Vault, and finalizing it yourself needs an EVM key and FLR (mainnet attestation about 20 FLR + gas). If the Smart Account's `executeDirectMintingWithData` reverts, FXRP is also not minted and the XRP does not return automatically (`0xE0` recovery procedure).
- **`sa-memo` waiting and `sa-status`**: `isTransactionIdUsed` is read-only and is not used to allow or refuse signing (display only). After the payment is validated, the command never fails whatever happens: a Flare read failure prints the re-check procedure (`sa-status`) to stderr and exits 0 (a non-zero exit would invite a re-run and a second payment to the Core Vault). When an executor is pinned, `0xE0` / `0xE1` / `0xE2` can only be finalized by the pinned executor, so it does not wait and says so (`0xD0` / `0xD1` bypass the pin). `sa-call` (`0xFE`) does not wait because public executors do not hold the userOp bytes. This view does not track a plain recipient-memo mint (C2) (on Coston2, a C2 tx finalized by a public executor was confirmed to be `false`). The guards (Core Vault pin, network pairing, simulate, `--yes`) are unchanged.
- **Unverified**: no write to Flare mainnet has ever actually been sent (see the 2026-10-20 section above). Passing `0xD1` (executor release without pin) on mainnet is also unverified.

## 2026-10-20 Flare EVM write surface expansion and FDC proof retrieval (user-approved policy reversal)

- **New write surface** (Flare EVM key): `-x fxrp-execute` (FDC fee + `executeDirectMinting`) / `fxrp-redeem` (burns FXRP) / `fxrp-tag-reserve` / `fxrp-tag-set` / `fxrp-tag-transfer`. CLI only, not exposed in the TUI. All guards are shared: `[flare.fassets] execute = true` (default false) → the EVM key taken in at startup → https Flare RPC → `--yes` on Flare mainnet. `--watch` is rejected because it drops the key. Each write reads its preconditions on-chain, then `estimate_gas` (a revert stops here), then sends.
- **The roadmap-log policy that FDC proof retrieval stays outside lazyxrp was reversed.** There are two new external HTTP surfaces: the verifier and the DA layer. Mitigations: https only (including overrides of `FDC_VERIFIER_URL` / `FDC_DA_URL`), redirects disabled, 1 MiB response cap, 30 s timeout, the verifier's `abiEncodedRequest` is byte-compared against the tx hash / `proofOwner` / type / source id before paying the fee (a fake request cannot make us pay), fee cap 100 FLR (measured 20 FLR), and the DA response's shape is checked by the proof parser before execution. The contract Merkle-verifies the proof, so all a forged DA / verifier can do is waste the fee. The proof is bound to the signing key's address (others cannot use it).
- **Remaining risks**: the fee (about 20 FLR) is not returned if the round does not finalize / the DA does not respond. `fxrp-tag-transfer` cannot be undone. `sa-vault`'s `value` is accepted raw (units confirmed in upstream `Instructions.sol`: deposit = FXRP smallest unit, redeem = shares, claim = period / `YYYYMMDD`), so mistaking the magnitude is the user's responsibility, and the memo hex is shown beforehand. `fxrp-execute` / `fxrp-redeem` / `sa-vault` enforce the XRPL–Flare network pairing (mainnet ↔ flare, testnet ↔ coston2) before any RPC. **Status of real sends**: on Coston2, `fxrp-redeem` (`redeemAmount` / `redeemWithTag`), `sa-vault`, `sa-memo`, `sa-call` and `fxrp-execute` were confirmed with real sends (ROADMAP F3-b to F3-d, TC-345). Writes to Flare mainnet and `fxrp-tag-reserve` / `fxrp-tag-set` / `fxrp-tag-transfer` have not been sent (only the rejection paths were measured, ROADMAP F3-a).

## 2026-10-16 Smart Account operator pin (S-018)

- **New trust boundary**: the destination (`MasterAccountController.getXrplProviderWallets()[0]`) and `Amount` (`getInstructionFee(id)`) of a Flare Smart Accounts instruction Payment are decided by a single Flare RPC answer. A malicious / compromised RPC can return its own XRPL address as the operator and intercept the user's XRP and the FXRP operation intent (32-byte memo). Same structure as the Core Vault (C2).
- **Mitigation**: sign only on an exact match between `[flare.smart_accounts] expected_operator` (a value the operator verified through another channel) and `flare::check_operator`. Unset or mismatched is rejected before signing (before any RPC, TC-240 / TC-243). The fee is limited to `1..=10 XRP` by `signing::check_smart_account_fee` (a 0 or order-of-magnitude-off answer is rejected). The Payment has a classic destination and no `DestinationTag` (with a tag, direct minting credits the tag holder), and operator ≠ signing account. Mainnet needs `--yes` (added to the TC-088 table), and signing happens only after simulate `tesSUCCESS`.
- **Residual**: the pin is the operator's responsibility (setting a wrong value sends to the wrong destination). Operator execution (FDC proof + `executeInstruction`) happens outside XRPL and lazyxrp does not verify the result (the TUI says "check the personal account's FXRP balance"). `getPersonalAccount` / balance are for display and are not used to allow Smart Account instruction signing (only a transfer exceeding the balance is rejected in the composer). **Exception**: the default recipient of the FXRP Mint composer (C2) (S-019 / R-019, 2026-10-24 above).
- **Units (FXRP `decimals()`)**: the transfer value is FXRP's smallest unit itself (the contract does not convert and passes it to `transfer(to, value)`). The composer converts a decimal amount with integer arithmetic only, using `decimals()` read from the same Flare RPC (excess digits, `decimals > 18` and not-fetched are rejected, TC-247). **Residual**: `decimals()` and the balance are answers from the same single RPC as the operator, so a malicious RPC can make the user send an amount shifted by orders of magnitude. However, the amount sent is limited to FXRP in the user's own personal account, the destination is the recipient the user typed, and only the operator is fixed by the pin (RPC `decimals()` / balance are outside the pin; trusting the RPC you choose is the same premise as S-015).

## 2026-10-11 Endpoint network check (S-017)

- **Symptom**: `XRPL_RPC_SERVER` (`rpc_server` / `--server` too) takes precedence over the `--network` preset (architecture §6.2). Writing with `--network testnet` while pointing at a mainnet endpoint made the `--yes` guard treat it as testnet and drop, and the signing / submission reached mainnet. The TUI `<Ctrl-n>` also keeps the custom endpoint, so the same applies. Found during testnet E2E in a shell with mainnet `XRPL_RPC_SERVER` and `XRPL_SEED` set (that run's writes had the env removed, so nothing was sent to mainnet).
- **Mitigation**: before signing, `RpcClient::ensure_network` checks `server_info.network_id` and rejects a mismatch (measured: mainnet omits `network_id` = 0, testnet 1, devnet 2, xahau 21337, xahau-test 21338). It covers all TUI XRPL submits (`fetch_account_summary_for_submit`) and CLI `send` / `offer-create` / `offer-cancel`. This adds one RPC per write.
- **Residual**: the check is the node's self-report. A malicious endpoint can return a fake `network_id` (trusting the endpoint you choose is the same premise as S-015).

## 2026-08-06 Re-review fixes

- **S-012 NFT external URI DNS SSRF**: fixed. `src/xrpl/nft_image.rs` inspects every DNS answer for each URI and rejects private/loopback/link-local/unique-local plus unspecified (`0.0.0.0`), `0.0.0.0/8`, broadcast, site-local `fec0::/10`, and IPv4 embedded in IPv6 (mapped `::ffff:`, compatible `::a.b.c.d`, NAT64 `64:ff9b::/96`, 6to4 `2002::/16`) judged by the inner IPv4. Automatic redirects are disabled, and each redirect is resolved, inspected and connection-pinned (limit 6 hops, 4 MiB — TC-146/147). https → http redirects are rejected (TC-237).
- **S-013 NFT metadata recursion DoS**: fixed. Metadata traversal has a depth limit of 32 and a node limit of 10,000. The 4 MiB response cap is kept.
- **S-014 Concurrent wallet submission**: fixed. `account_info → simulate → sign → submit` in the poll task is serialized by `PollContext.submit_lock`. No blind retry was added. After a submit failure, the next operation re-fetches account_info.
- **S-015 Signing over insecure RPC**: fixed. Using an `http://` or `ws://` endpoint while holding a signing seed refuses startup. Read-only use is allowed only when no seed is set.
- **S-016 Issuer and numeric validation of transaction input**: fixed. TrustSet, RegularKey and OfferCreate validate the classic address checksum, IOU currency format, finite positive IOU values and positive XRP drops.

The open structural issues (splitting `poll.rs`, migrating the Action error type, fake RPC/WebSocket integration tests, separating the Watch/Operate UI) are outside the scope of this safety fix. In the next change, treat live tests as auxiliary and add local fixtures.

## 2026-10-04 Security audit response (source audit → local reproduction → fix)

14 leads from the source audit were checked in order and the following were fixed (tests in `docs/test.md` TC-175 to TC-182).

- **Flare RPC https enforcement and signing refusal (S-015 extension, R-014)**: `FLARE_RPC_URL` / the preset are checked by `lib.rs::check_flare_endpoint`. `http://` is rejected without `--allow-insecure-rpc`, and if signing credentials exist (XRPL seed/mnemonic or the Flare EVM key env) it is rejected even with `--allow-insecure-rpc`. Because the Flare RPC answer decides the Core Vault destination, since 2026-10-11 nothing is signed unless it matches `[flare.fassets] expected_core_vault` (a value the operator verified through another channel) (`flare::check_core_vault`; unset is also rejected).
- **Cap on node-decided Fee**: a Fee above `signing::MAX_NODE_FEE_DROPS` (2 XRP = 2,000,000 drops) is rejected before signing (`sequence_fee_ledger_from_simulate` on the simulate path, for both the TUI and CLI `Send`). CLI `Send` also requires simulate (nothing is signed unless `tesSUCCESS`), and the balance check uses `checked_add` for `amount + fee`. Since 2026-10-11, `parse::check_simulate_bound_to_request` (inside `simulate_tx_requiring_tes_success`, common to all writes) also checks that the simulate `Sequence` equals the requested value and that `LastLedgerSequence` is at most `ledger_index + MAX_LLS_WINDOW` (100). Residual: the window is based on the node's own `ledger_index`.
- **`xrp-ledger.toml` fetching (S-012 extension)**: the manifest-derived domain is restricted to ASCII hostnames only, and the same guarded fetch as NFT images is used (`nft_image::fetch_via`/`client_for_url`: private/loopback rejected, DNS pinned, re-inspected on each redirect, 1 MiB cap).
- **DNS-pinning bypass via proxy**: `client_for_url` uses `.no_proxy()`. **In an environment that requires HTTP(S)_PROXY, fetching NFT images and toml fails (fail closed)**. The private check gained `100.64/10`, `192.0.0.0/24`, `198.18/15` and `240/4`.
- **`--watch` (view-only startup, 2026-10-11, TC-234–235)**: `lib::enter_watch_mode` empties `Secrets` (the seed / mnemonic / `flare_evm_key` moved from `Config` by `take_secrets()`) right after startup, leaving only the address for display. The TUI shows no Wallet (composer, key generation), and no credential is passed to the poll task. `-x` signing subcommands are rejected before any RPC.
- **Crash class**: byte slicing of remote strings (`short_hex` / tx hash / path address) was changed to per-character, Flare `U256::to::<T>()` to `saturating_to` / `try_into`, and the `ripple_path_find` sort to `f64::total_cmp` (non-finite last). Since the panic hook calls `process::exit(1)`, all of these could terminate the TUI via remote input.
- **RPC body cap**: 16 MiB, enforced during chunk reading in addition to the `Content-Length` pre-check.
- **Control / invisible characters in remote strings**: control characters, bidi overrides and zero-width format characters are stripped from `error_message` / `engine_result(_message)` / tx hash (`util::sanitize_remote`) and the Server panel host / version (`sanitize_text`) (TC-236).
- **S-003**: the warning is emitted in `Config::new()` (before `logging::init`), so it uses `eprintln!` rather than `tracing`.
- **`install.sh`**: the GitHub token goes through curl `-K -` (stdin) / wget `--config` (fd) and is removed from argv. A token containing unexpected characters is ignored. curl uses `--proto '=https' --proto-redir '=https'` (redirect targets are https only), and the README one-liner also uses `--proto '=https' --tlsv1.2`. The wget fallback has no scheme restriction (`--https-only` is effective only when recursive).
- **`cd.yml`**: verification build with `cargo package --locked` (no token), and the token-bearing step uses `cargo publish --locked --no-verify`.

Investigated but **not fixed** (same-operator actions / hardening): mismatch between the network display and the connection target after Ctrl-N, logging of a provider token placed in a URL, dUNL signature not verified (display only), NFT decode dimension cap, CD ref/environment constraints (covered by GitHub-side rulesets). Double submission was handled on 2026-10-11 (TUI: a write with the same params is rejected until the result Ok/Err arrives, TC-228). Binding Actions from config keybindings was fixed later: anything outside the `config::is_bindable` allowlist (only payload-less navigation/refresh actions) is a load-time error (`*Submit` etc. cannot be bound).

## 2026-10-11 Security audit run-2 response

The 4 needs_validation items (Core Vault pin, simulate Sequence/LLS check, quote currency check, simulate string sanitize) are covered by the items in the 2026-10-04 section above. The following are hardening (tests in `docs/test.md` TC-204 to 214).

- **Taking env secrets**: `XRPL_SEED` / `XRPL_MNEMONIC` / the Flare EVM key (`[flare.fassets] evm_key_env`) are taken out exactly once by `config::take_secret_env` in `Config::new()` and removed from the env, held as `SecretString`, and the raw `String` is zeroized. Right after applying CLI credentials, `lib.rs::run` moves the seed / mnemonic / EVM key from `Config` into `Secrets` via `Config::take_secrets()` (`None` on the `Config` side), so no secret reaches the panels / status bar / splash that share the `Arc<Config>`. `Secrets` is received only by the signing paths (`App` → poll task, `run_flare_command`), and the UI receives only the address derived once by `App::new` (`AccountWalletTab::new` → `WalletPanel`). The EVM key was previously `remove_var`ed inside the poll task at first execute (a mutation while running, during which other tasks may read the env, and if it failed the key was unusable afterwards). **On Linux `remove_var` does not rewrite the initial `/proc/self/environ`** (readable from the same uid): passing via env still assumes trust within the same user.
- **The JSON-RPC client does not follow redirects** (`client.rs`, `Policy::none()`). This closes the path where an https endpoint forwards `submit` (the signed blob) to http via 307. The system proxy is left as is (the operator's choice). The faucet also has no redirects plus a body cap.
- **The tx hash** is computed locally from the signed blob. The TUI submit result shows the signed Fee from simulate (`<hash> · fee N drops`). There is no confirmation step before signing (the Fee cap `MAX_NODE_FEE_DROPS` covers it).
- **Input**: extra separators such as `XRP:100:tail` are rejected. CLI `send` also runs the same `validate_payment_input` as the composer (amount > 0 etc.) before any RPC.
- **Key generation popup / faucet**: `GeneratedWalletKeys` zeroizes the seed on drop, and the popup is cleared on Network switch (shown as "Esc to dismiss · g new key"). The seed generated by `-x faucet` is a `SecretString`.
- **CI**: all GitHub Actions pinned to commit SHAs (tag in a comment), `cross@0.2.5` pinned.

## 2026-09-17 Dependency audit response (detected by setting up `mise run audit`)

- **RUSTSEC-2026-0253: lru 0.16.4 `LruCache::pop()` unsound (use-after-free)**: a runtime dependency via `alloy-provider` → `lru`. There was no fixed 0.16.x and alloy 1.8.3 (last 1.x) still used the old lru, so **alloy was bumped a major version, 1.8.3 → 2.4.2** (`Cargo.toml` `alloy = "2"`). The API surface used is only primitives + `ProviderBuilder` + `EthereumWallet`, so no change was needed. `rust-version` went 1.91 → 1.94.1 to match alloy 2.4.2's MSRV: **do not lower `rust-version` below 1.94.1, as lru 0.16.4 would return**. `cargo tree -i lru` = a single 0.18.4.
- **RUSTSEC-2026-0285: rustls 0.23.44 TLS 1.3 handshake encryption-level validation flaw (published 2026-09-14, Medium)**: updated to **0.23.45** with `cargo update -p rustls` (a patch within 0.23.x).
- Both were resolved by dependency updates, not by adding ignores. They were found on the first run in which `mise run audit` (cargo-audit + cargo-deny) became able to detect locally at the same standard as CI's security-advisories.
- **RUSTSEC-2026-0387: crypto-bigint 0.5.5 timing variability in constant-time operations**: pulled by `k256`/`ecdsa` 0.16 (`xrpl-rust`, `kobe-xrpl`, `alloy-signer`); the fix is 0.7.6, which is semver-incompatible, so `cargo update` cannot reach it. Ignored in `deny.toml` and `.cargo/audit.toml` until upstream moves to the `elliptic-curve` 0.14 line; remove both entries then (`unused-ignored-advisory = "deny"` will flag a stale one in deny).  Impact: `k256` signing runs locally on the user's machine (no remote timing oracle), but this is a residual risk on shared hosts.

## Open items and notes

### S-009: `--self-uninstall` (user-driven removal of the binary and config data)

- Deletes the `std::env::current_exe()` file and the `.bak`, and `remove_dir_all`s the config/data directories resolved by `Config`.
- By default it lists the targets and then requires `yes` on stdin. `--yes` skips the confirmation.
- **Cargo's install metadata is not removed**. In a `cargo install`ed environment, the user separately runs **`cargo uninstall lazyxrp`** if needed.

## Resolved (history)

| ID | Description | Fix |
| --- | --- | --- |
| S-001 | The `Debug` derive on `RawSigningConfig` logged the seed in plaintext | `impl fmt::Debug` masks it as `[REDACTED]` |
| S-002 | `Config::new()` did not delete the env var after reading it | removed with `unsafe { env::remove_var(SEED_ENV) }` |
| S-003 | No file permission check when the config file stores signing credentials (seed / mnemonic) | On Unix, warn **to stderr** when group/world readable (`warn_if_config_world_readable`; it runs before `logging::init`, so `tracing` is not possible) |
| S-004 | No path validation for paths from env vars | `validated_path` rejects `..`. `canonicalize()` + rejecting paths outside home is recommended before Phase 3 |
| S-006 | `Drop` in `tui.rs` used `unwrap()` | `if let Err(e) = self.exit() { eprintln!(...) }` |
| S-007 | Parsing of the built-in config | `expect` at build time for detection during development |
| S-008 | Default tracing log level was INFO | No problem after S-001 was addressed |
| S-005 | Insufficient warning for `NO_VERIFY=1` in `install.sh` | In addition to `usage()` / the header, a red MITM-risk warning banner is also shown when running with `NO_VERIFY=1` |
| S-010 | `Config` kept the plaintext `seed` inside an `Arc<Config>` | Added `secret_seed: Option<SecretString>` to `RawSigningConfig` and cleared the plaintext in `Config::new()`. Furthermore `lib.rs::run` moves the secrets into `Secrets` via `Config::take_secrets()` before sharing the `Arc<Config>` |
| S-011 | CLI `--seed` stays in process arguments and is visible via `ps` | `README.md` warning + startup `eprintln!` warning + a deprecation note in `--help` |
