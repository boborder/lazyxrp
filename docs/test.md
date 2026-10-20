# Test Strategy & Registry

**Role:** テスト設計・テストガイド（重要 TC カタログと FR/architecture へのトレース）。

> Last Updated: 2026-09-21 (contract-first consolidation ×3: same-contract tests merged into table-driven tests — lib suite 300 → 237 fns incl. added TC-049 env-seed test; TC-075 (library serde) and TC-138 (f64 artifact pin) removed; TC rows resynced)
> Target: lazyxrp (Rust TUI for XRPL)
> Catalog: important-case roster (not a 1:1 index of every `#[test]`)

## Policy

- This file is an **important-case roster**: a TC is added only for cross-module contracts, regression-prone logic, or user-visible behavior. Unannotated helper/unit tests are intentionally outside the catalog.
- Reference the `TC-xxx` ID in commit messages (e.g. `test(xrpl): add NFT parse case (TC-013)`).
- Inline `#[cfg(test)]` modules preferred; live-network cases are `#[ignore]` and run explicitly.
- Tests live in the lib (`src/lib.rs`) and `tests/cli_smoke.rs`. `[lib] doctest = false` and `test = false` on both `[[bin]]` targets (`Cargo.toml`) skip empty test targets to keep `cargo test` rebuilds fast — put unit tests in lib modules, not `main.rs` / `bin/rp.rs`, or drop `test = false` first.
- **Traceability:** User-visible TCs should map to `docs/requirements.md` FR/NFR and/or `docs/architecture.md` §. Keys/layout TCs trace to `DESIGN.md`. See [Traceability](#traceability) for network/Flare rows.

## Commands

```bash
cargo test                # all tests
cargo test <name>         # one case
# CI gates (.github/workflows/ci.yml):
cargo test --locked --all-features --workspace
cargo fmt --all --check
cargo clippy --locked --all-targets --all-features --workspace -- -D warnings
cargo doc --locked --no-deps --document-private-items --all-features --workspace --examples
cargo audit --ignore RUSTSEC-2026-0235
cargo deny check
```

## Registry

| TC | P | Target | Case |
|---|---|--------|------|
| TC-001-003 | P0 | `src/xrpl/parse.rs` | book_currency — XRP uppercase/case-insensitive vs issued with issuer |
| TC-004-005 | P1 | `src/xrpl/util.rs` | json_str — nested path returns value; missing path returns empty |
| TC-006-007 | P1 | `src/xrpl/util.rs` | json_u32 — number / string-number coerced; missing or non-numeric returns zero |
| TC-008-009 | P0 | `src/xrpl/format.rs` | drops_to_xrp — basic conversion; invalid string returns zero |
| TC-010-012 | P1 | `src/xrpl/format.rs` | format_amount — None dash / XRP drops string / issued currency object |
| TC-013, TC-074 | P1 | `src/xrpl/parse.rs` | account_nfts — parse response into NftRow vec; tfMutable flag (dNFT) |
| TC-014 | P1 | `src/xrpl/parse.rs` | account_lines — parse response into TrustLineRow vec |
| TC-015 | P2 | `src/xrpl/parse.rs` | amm_info — real rippled shape (`amount` / `amount2` / `lp_token` / `trading_fee`; assets implied by the amounts) into AmmSummary |
| TC-016 | P2 | `src/xrpl/parse.rs` | account_tx — parse response into TxRow vec (with direction) |
| TC-017 | P1 | `src/xrpl/parse.rs` | book_offers — parse response into OfferRow vec |
| TC-018 | P2 | `src/xrpl/parse.rs` | server_info/fee — parse response into summary structs |
| TC-019-022 | P2 | `src/config.rs` | parse_style — empty yields default, foreground/background mapping, modifiers combined, gray==grey |
| TC-023 | P2 | `src/config.rs` | extract_color_and_modifiers — extracts modifiers and color |
| TC-024-025 | P2 | `src/config.rs` | parse_color — RGB shorthand; unknown returns None; out-of-range `rgb`/`grayNN`/`bright colorNN` return None (no u8 overflow) |
| TC-026 | P0 | `src/config.rs` | Config::new — loads default keybindings |
| TC-027-029,031-032 | P1 | `src/config.rs` | parse_key_event — modifier composition, case insensitivity, invalid keys error |
| TC-033-035, TC-092 | P1 | `src/config.rs` | Config source precedence — file overrides default, `XRPL_RPC_SERVER` env overrides file, XDG resolution, `~/.config` fallback |
| TC-036 | P2 | `src/config.rs` | Config validation — invalid key sequence format |
| TC-037 | P0 | `src/network.rs` | Network::default — is Mainnet |
| TC-038-040 | P0 | `src/network.rs` | Network::from_str — roundtrip mainnet/testnet/devnet, case-insensitive, unknown errors |
| TC-041 | P1 | `src/network.rs` | Network::is_production — correct boolean |
| TC-042 | P0 | `src/config.rs` | `XRPL_NETWORK` overrides the file's network in `Config::new`; unparsable / `" testnet"` values are ignored |
| TC-043 | P0 | `src/lib.rs` | resolve_rpc_url/resolve_ws_url — precedence CLI > env > config file > network default |
| TC-044 | P1 | `src/lib.rs` | resolve_ws_url — CLI --ws-server is top priority |
| TC-045-046 | P1 | `src/signing.rs` | credential_from_secrets — none / seed / mnemonic / both-rejected outcome matrix |
| TC-047 | P1 | `src/signing.rs` | prompt_production_confirmation — testnet/devnet skips |
| TC-048 | P2 | `src/signing.rs` | prompt_production_confirmation — --yes skips on mainnet |
| TC-049 | P1 | `src/config.rs` | Config::new — env XRPL_SEED overrides file seed |
| TC-050 | P0 | `src/xrpl/cli_exec.rs` | CLI info — local JSON-RPC succeeds |
| TC-051 | P1 | `src/xrpl/cli_exec.rs` | CLI account — local JSON-RPC succeeds |
| TC-052 | P1 | `src/xrpl/cli_exec.rs` | CLI book — local JSON-RPC succeeds |
| TC-053 | P1 | `src/xrpl/cli_exec.rs` | CLI summary — local JSON-RPC succeeds |
| TC-054 | P1 | `src/xrpl/cli_exec.rs` | CLI nfts — local JSON-RPC succeeds |
| TC-055 | P1 | `src/xrpl/cli_exec.rs` | CLI lines — local JSON-RPC succeeds |
| TC-056 | P2 | `src/xrpl/cli_exec.rs` | CLI amm — local JSON-RPC succeeds |
| TC-057 | P2 | `src/xrpl/cli_exec.rs` | CLI tx-history — local JSON-RPC succeeds |
| TC-058 | P1 | `src/cli.rs` | CLI — invalid parameters error; removed `watch` subcommand no longer parses |
| TC-059 | P2 | `src/xrpl/cli_exec.rs` | CLI — invalid classic account address fails before RPC connect |
| TC-060 | P3 | `src/app.rs` | Watch mode — tab cycling/quit/refresh covered by TC-061–065 (former 4-tab construction test removed: asserted constructor defaults) |
| TC-061 | P1 | `src/app.rs` | Watch mode — `q` key sets `should_quit` |
| TC-062-063 | P2 | `src/app.rs` | Watch mode — `r`/`b` send Account/Book PollCommand; a second press within 500 ms is debounced |
| TC-064 | P2 | `src/app.rs` | Watch mode — TabNext/TabPrev wrap around panels |
| TC-065 | P3 | `src/app.rs` | Watch mode — HelpOverlay toggles on `?` and Esc |
| TC-066 | P1 | `src/xrpl/cli_exec.rs` | CLI — `account-status` against local JSON-RPC |
| TC-068 | P1 | `src/xrpl/client.rs` | XRPL RPC error — not found is not silently swallowed |
| TC-069 | P1 | `src/xrpl/parse.rs` | XRPL submit response — requires `tesSUCCESS` and hash |
| TC-070 | P1 | `src/xrpl/types.rs` | book_offers — issued quote uses `currency_code` |
| TC-071-072 | P1 | `src/xrpl/parse.rs` | account_objects — empty array parses; mixed Check / Ticket / MPT / PayChannel / Escrow |
| TC-076 | P1 | `src/cli.rs` | CLI — `--self-uninstall` flag parses; backup path helper |
| TC-077-079 | P2 | `src/components/panels/tx_history.rs` | TxHistoryPanel — filter by tx_type / hash partial; empty filter shows all |
| TC-080-082 | P2 | `src/xrpl/parse.rs` | ripple_path_find — alternatives, empty alternatives, `source_amount` as XRP drops string |
| TC-083 | P2 | `src/xrpl/format.rs` | summarize_paths_computed — hop chain abbreviation |
| TC-084 | P2 | `src/xrpl/format.rs` | path_find_rows_from — display rows; cheapest-first ordering |
| TC-085-086 | P2 | `src/xrpl/types.rs` | asset_display_name — known hex codes; unknown passthrough |
| TC-087 | P2 | `src/xrpl/poll.rs` | Poll trigger — coalesces rapid bursts |
| TC-088 | P0 | `src/xrpl/submit.rs` | Mainnet guard: all submit paths reject without `--yes` (R-006); table includes Smart Account instruction |
| TC-089/TC-144 | P0 | `src/xrpl/parse.rs`, `src/xrpl/poll.rs` | account_tx result mapping — not-found → empty history (I-7); action follows append flag (Ok/Err × append matrix) |
| TC-093 | P1 | `src/components/shared/tx_detail/mod.rs` | TxDetail render cache — invalidated on open |
| TC-094 | P2 | `src/components/shared/tx_detail/parsers.rs` | TxDetail parser registry — required types, no duplicates |
| TC-096-097 | P2 | `src/xrpl/parse.rs` | get_aggregate_price — full response; trimmed_set omitted |
| TC-098 | P1 | `src/signing.rs` | IOU validation — invalid issuer / non-finite / zero amount rejected |
| TC-099 | P1 | `src/signing.rs` | IOU validation — XRP / overlong currency code rejected, 3-letter ok |
| TC-100 | P2 | `src/xrpl/nft_image.rs` | NFT metadata traversal stops at depth limit |
| TC-101 | P1 | `src/lib.rs` | Signing seed refuses plaintext RPC/WS endpoints |
| TC-102 | P1 | `src/xrpl/client.rs` | Rate-limited RPC errors retryable; permanent XRPL errors not |
| TC-103 | P2 | `src/xrpl/ws.rs`, `src/components/shared/fps.rs` | Duplicate ledger-close emits one trigger; FPS counts draws not ticks |
| TC-227 | P1 | `src/xrpl/ws.rs` | Local WebSocket fixture (real rippled frame shapes): subscribe has account + `ledger` stream; ledger close → `XrplLedgerClose` + one poll trigger (duplicate dropped); account tx → `XrplAccountTx`; server close → reconnect + resubscribe; cancel stops the task |
| TC-228 | P1 | `src/components/panels/wallet.rs` | Double-submit guard: identical write params refused (flash) until that write's Ok/Err arrives; different params or a retry after the result go through |
| TC-229 | P2 | `src/xrpl/client.rs` | `account_offers` follows `marker` across pages (local JSON-RPC) until none is returned |
| TC-230 | P1 | `src/xrpl/client.rs` | `simulate` request carries no JSON nulls (Direct Mint memo built by xrpl-rust has `MemoFormat: null`; rippled rejects it) |
| TC-231 | P1 | `src/flare.rs` | C3 `executeDirectMinting` selector = deployed `0x78d0299e` (`IXRPPayment.Proof`); a real Coston2 DA `{response, proof}` answer decodes (and its `{data, merkleProof}` alias encodes identically); malformed / old hex-`response` shape rejected |
| TC-232 | P1 | `src/xrpl/submit.rs` | AccountDelete (local JSON-RPC): an X-address destination's tag reaches simulate and the signed blob; the blob carries the simulate-chosen fee (owner reserve), not a base fee. Mainnet without `--yes` refused (TC-088 table) |
| TC-233 | P1 | `src/components/panels/wallet.rs` | AccountDelete composer: `s` never queues directly — invalid destination flashes, valid one opens `DELETE` type-to-confirm; Esc keeps the destination; partial word refused; `DELETE` + Enter queues the trimmed destination |
| TC-234 | P1 | `src/lib.rs` | `--watch -x <write>` (send / offer-cancel / offer-create / amm-deposit / amm-withdraw / account-delete) refused before any RPC; reads, faucet and the TUI path still resolve |
| TC-235 | P1 | `src/lib.rs` | `enter_watch_mode` drops seed, mnemonic (plaintext + secret) and Flare EVM key from `Config`, returns the key's address (None without a key) |
| TC-236 | P1 | `src/xrpl/util.rs`, `src/components/panels/server_detail.rs` | `sanitize_text` (TUI server panel, validator detail popup — domain, TOML text, attestation, errors, Content-Type — and `sanitize_remote` CLI/error text) strips controls, bidi overrides (`U+202A–202E`, `U+2066–2069`, `U+061C`) and zero-width format chars; ordinary Unicode kept, cap counts chars |
| TC-237 | P1 | `src/xrpl/nft_image.rs` | Redirect hop https → http refused (NFT image + `xrp-ledger.toml`); scheme-relative / relative hops stay https; http → https allowed. SSRF table adds `::a.b.c.d`, NAT64, 6to4, `fec0::/10`, `0.0.0.0/8` (public NAT64 / 6to4 still allowed) |
| TC-238 | P2 | `src/components/shared/tx_detail/mod.rs` | Ripple-epoch time from a tx field saturates instead of overflowing; `u64::MAX` renders the `----` placeholder |
| TC-239 | P0 | `src/signing.rs` | Smart Account payment reference (32 bytes): id at byte 0, walletId at 1, value at 2..12 (10 bytes big-endian), recipient at 12..32; pinned to the reference-encoder vectors (transfer value 10 `0x01 00 …0a f548…131d`, value 1234567 `…12d687 f548…131d`, redeem 3 `0x02 00 …03` + zero tail) and distinct-byte offsets; value 0 / ≥ 2^80 refused (2^80−1 fits), recipient malformed / zero address refused, case + whitespace normalized; `check_smart_account_fee` bounds 1..=10 XRP; `parse_token_amount`: exact integer decimal → base units for decimals 0 / 6 / 8 / 18, too many fraction digits / decimals > 18 / `.5` / `1.` / sign / exponent / u128 overflow refused |
| TC-240 | P0 | `src/flare.rs` | `check_operator`: RPC operator signed only on exact match with `[flare.smart_accounts] expected_operator`; mismatch / unset / blank pin → refused (R-017) |
| TC-241 | P1 | `src/flare.rs` | `fetch_smart_account_info` against a local JSON-RPC mock: registry → controller / AssetManager, personal account, nonce, operators, both fees, FXRP balance (base units) + `decimals()` (6 / 8 / 0 carried through; 19 → error), `eth_getCode` → deployed flag; undeployed (`0x`); out-of-range uint256 → error (no panic); malformed XRPL owner never reaches the RPC; `format_token_units` exact for any decimals |
| TC-242 | P0 | `src/xrpl/submit.rs` | Smart Account transfer + redeem vs local JSON-RPC: `server_info → account_info → simulate → submit` order; simulate and signed blob = Payment to the pinned operator, `Amount` = instruction fee (exact), no `DestinationTag`, one Memo whose `MemoData` decodes to the 32-byte instruction (id, wallet 0, value, recipient / zero) |
| TC-243 | P0 | `src/xrpl/submit.rs` | Smart Account refusals happen before any RPC: pin unset / blank / mismatched, fee 0 or > 10 XRP, zero / malformed recipient, value 0 / over 10 bytes, non-address operator, X-address operator (tag), operator = signer; exactly one error action each (R-017) |
| TC-244 | P1 | `src/components/panels/wallet.rs` | Smart Account composer: bad / zero recipient, zero / empty / over-balance amount, non-numeric / zero lots, read for another account → flash only; valid transfer / redeem queue exact params (FXRP decimal → base units via the read `decimals()`, operator + fee from the read); picker `a`, `i` flips transfer ↔ redeem, typing + `s`, Esc → picker row 13; composer refuses to open without a read; render shows operator, pin status, fee, balance |
| TC-245 | P1 | `src/components/panels/fxrp_direct_mint.rs` | FXRP panel renders `SmartAccountInfo` values (personal account, FXRP balance formatted with `fxrp_decimals`, nonce + deployed / not deployed, operator, both fees); control characters in remote strings are stripped; no rows before the first `SmartAccountInfo` |
| TC-246 | P2 | `src/config.rs` | `[flare.smart_accounts] expected_operator` parses; unset by default |
| TC-247 | P0 | `src/components/panels/wallet.rs` | Smart Account composer follows the read `decimals()` (8 and 0, not an assumed 6): `12.5` → 1_250_000_000 units at 8 decimals; exact balance accepted, balance + 1 unit refused; more fraction digits than decimals refused; decimals > 18 and no read → never queues |
| TC-104 | P1 | `src/xrpl/parse.rs`, `src/components/shared/fps.rs` | account_tx marker survives parsing; tick-rate label refreshes after 1s |
| TC-105 | P2 | `src/xrpl/client.rs` | Local HTTP 429 retried before success |
| TC-248 | P0 | `src/coverage.rs` | XRPL coverage drift: the ROADMAP.md table between the `xrpl-coverage` markers equals `rows()` (type, family, read, TUI, CLI, status) and README states `**N / M**` writable / total; registry consistent (no type listed twice, every Spec type has a tx_detail parser, Existing rows are wired) |
| TC-249 | P0 | `src/xrpl/txspec/{mod,nft}.rs` | Field validation tables per NFT type (accept / reject; TransferFee 50000 ok / 50001 reject and needs transferable; URI 256 / 257 bytes incl. multibyte; ids 63 / 64 / 65 hex; taxon u32 max; 500 / 501 offers; sell / buy Owner rules; BrokerFee needs both offers; unknown field / flag); registry well-formed; CLI `Field=value` parsing; relative `Time` resolved once in `prepare` |
| TC-250 | P0 | `src/xrpl/txspec/nft.rs` | Unsigned `tx_json` shape per NFT type (hex URI, flags bits, Amount drops / IOU object, Account / Sequence) |
| TC-251 | P0 | `src/xrpl/txspec/nft.rs` | Signed blob per NFT type decodes with distinct Sequence 41 / Fee 13 / LastLedgerSequence 987654 (swapped args fail); builder applies the crate's model rules |
| TC-252 | P0 | `src/xrpl/submit.rs` | Generic submit vs local RPC mock: method order `server_info` → `account_info` → `simulate` → `submit`; signed Sequence 7 / Fee 15 / LLS 80,000,020 from simulate; relative Expiration fixed once |
| TC-253 | P0 | `src/xrpl/submit.rs` | Generic submit stops at a `tec*` simulate: no `submit` call, Err action (fails when the `tesSUCCESS` guard is removed) |
| TC-254 | P0 | `src/xrpl/cli_exec.rs` | `-x tx` / `nft-mint` / `nft-burn`: simulate → confirm → submit, `--no-wait`, refusals (bad field, unknown type, mainnet without `--yes`) never reach RPC, simulate stop |
| TC-255 | P0 | `src/components/panels/wallet.rs` | Transaction composer: form generated from field specs, inline validation, single queued `GenericTxSubmit`; burn needs typed `BURN` (partial refused, Esc restores the form) and opens prefilled; help / inline errors rendered |
| TC-256 | P1 | `src/app.rs`, `src/components/tabs/nft.rs` | Assets `x` opens the burn composer with the selected NFTokenID (not on other tabs; none selected → no-op) |
| TC-260 | P0 | `src/xrpl/txspec/checks_misc.rs` | Field validation tables for CheckCreate / CheckCash / CheckCancel / TicketCreate / DepositPreauth / SignerListSet / Clawback: TicketCount 0 / 1 / 250 / 251, id 63 / 64 / 65, exactly-one rules, signers 8 / 9, weight 0 / 65535 / 65536, quorum vs total weight, delete form, XRP clawback; typed confirm phrases only on CheckCash / SignerListSet / Clawback |
| TC-261 | P0 | `src/xrpl/txspec/checks_misc.rs` | Unsigned `tx_json` shape per type (ids upper-cased, nested `SignerEntry` objects, delete form omits `SignerEntries`, no `Fee` / `LastLedgerSequence`) |
| TC-262 | P0 | `src/xrpl/txspec/checks_misc.rs` | Signed blob per type decodes with distinct Sequence 41 / Fee 13 / LastLedgerSequence 987654; builder applies model rules (signer list containing the signer, self-clawback) |
| TC-263 | P0 | `src/xrpl/txspec/checks_misc.rs` | Generic submit of TicketCreate vs local RPC mock: invalid form never reaches RPC; `server_info` → `account_info` → `simulate` → `submit` with simulate-chosen Sequence / Fee / LLS in the blob |
| TC-280 | P0 | `src/xrpl/txspec/channel_amm.rs` | PaymentChannelCreate / Fund validation: drops 1 / 1e17 / 1e17+1 / 0 / fractional / `XRP:` prefix, SettleDelay u32 max / +1, PublicKey shape (66 hex, `ED` / `02` / `03` only), channel id 63 / 64, `CancelAfter` Time, required fields, unknown field |
| TC-281 | P0 | `src/xrpl/txspec/channel_amm.rs` | PaymentChannelClaim cross-field rules: Signature needs Amount / Balance / PublicKey, Balance ≤ Amount, renew + close exclusive, Balance required unless close / renew, signature length 144 / 146, odd / non-hex refused |
| TC-282 | P0 | `src/xrpl/txspec/channel_amm.rs` | AMMCreate / Vote / Bid / Delete validation: TradingFee 1000 / 1001, identical amounts / assets refused, asset grammar (`XRP` / `CUR:issuer`), BidMin ≤ BidMax and same LP token, XRP bid refused, AuthAccounts 4 / 5 / duplicates / bad address |
| TC-283 | P0 | `src/xrpl/txspec/channel_amm.rs` | Unsigned `tx_json` per type (drops strings, id / key / signature upper-cased, claim flag bits, `Asset` objects, nested `AuthAccounts`, optional fields omitted, no `Fee` / `LastLedgerSequence`) |
| TC-284 | P0 | `src/xrpl/txspec/channel_amm.rs` | Signed blob per type decodes with distinct Sequence 41 / Fee 13 / LastLedgerSequence 987654 and the typed fields; sender as its own AuthAccount refused at build |
| TC-285 | P0 | `src/xrpl/txspec/channel_amm.rs` | Typed confirm phrases `LOCK` / `FUND` / `CLAIM` / `CREATE` / `BID` / `DELETE`; none on AMMVote |
| TC-286 | P0 | `src/xrpl/txspec/channel_amm.rs` | Generic submit of PaymentChannelCreate + AMMDelete vs local RPC mock: invalid form never reaches RPC; `server_info` → `account_info` → `simulate` → `submit`; relative `CancelAfter` fixed once (simulate = blob), simulate-chosen Sequence / Fee / LLS in the blob |
| TC-320 | P0 | `src/xrpl/txspec/xchain.rs` | Flat `XChainBridge` grammar (`lockDoor,lockIssue,issueDoor,issueIssue`): 4 parts, checksummed distinct doors, `XRP` / `CUR:issuer` issues, cross-currency pairing refused |
| TC-321 | P0 | `src/xrpl/txspec/xchain.rs` | Field validation tables for the 6 XChain types: drops 0 / max / max+1, claim id hex 1..=16 digits and non-zero, required fields, XRP-only minimum, modify needs a change, Amount must be a bridge issue, account-create needs XRP-XRP |
| TC-322 | P0 | `src/xrpl/txspec/xchain.rs` | Unsigned `tx_json` per XChain type (bridge object, drops strings, modify flag `0x10000`, claim id upper-cased, no `Fee` / `LastLedgerSequence`; IOU bridge issues as `{currency, issuer}`) |
| TC-323 | P0 | `src/xrpl/txspec/xchain.rs` | Signed blob per XChain type decodes with distinct Sequence 41 / Fee 13 / LastLedgerSequence 987654 and the bridge object |
| TC-324 | P1 | `src/xrpl/txspec/xchain.rs` | Builder applies the crate model rules: CreateBridge / ModifyBridge signer must be a door account |
| TC-325 | P0 | `src/xrpl/txspec/xchain.rs` | Typed confirm phrases: `BRIDGE` (CreateBridge), `COMMIT` (XChainCommit / AccountCreateCommit), `CLAIM` (XChainClaim); none on Modify / CreateClaimID |
| TC-329 | P1 | `src/components/shared/tx_detail/parsers.rs`, `src/xrpl/txspec/xchain.rs` | Tx History detail parser for all 6 XChain types (coverage "read"); bridge rendered in the flat `lockDoor,lockIssue,issueDoor,issueIssue` form with claim id |
| TC-326 | P2 | `src/xrpl/txspec/xchain.rs` | `-x tx` `Field=value` parsing resolves `XChainBridge` / `XChainClaimID` case-insensitively |
| TC-327 | P0 | `src/xrpl/txspec/xchain.rs` | Generic submit of XChainCommit vs local RPC mock: invalid bridge never reaches RPC; `server_info` → `account_info` → `simulate` → `submit`, bridge + normalised claim id in simulate request and signed blob |
| TC-328 | P1 | `src/xrpl/txspec/xchain.rs` | IOU bridge (issued-currency issues) signs and decodes |
| TC-330 | P0 | `src/flare.rs` | FXRP C2 amount must exceed `max(min fee, fee_bips share) + executor fee`: below the minimum fee (whole payment goes to the fee receiver on Flare), exactly on the boundary → refused; one unit above → accepted; percentage share dominates on large payments |
| TC-331 | P0 | `src/flare.rs` | FXRP C3 receipt classified from AssetManager events only: `DirectMintingExecuted` → minted, `DirectMintingDelayed` / `LargeDirectMintingDelayed` → delayed (`executionAllowedAt`), `DirectMintingPaymentTooSmallForFee` → flagged; same events from another address or no events → unknown |
| TC-332 | P1 | `src/components/panels/fxrp_direct_mint.rs` | FXRP panel shows `MINT PAUSED` / `EMERGENCY PAUSE Ln` (full and compact); unknown or clear state shows nothing |
| TC-333 | P0 | `src/components/panels/wallet.rs` | FXRP Mint composer queues params carrying the read fee views; `mintingPaused` or any emergency level refuses to queue |
| TC-334 | P0 | `src/flare.rs` | Mint-limit replay + classification: a stale window drains by whole elapsed windows (no roll-over), headroom is one granule below the limit, never underflows; the cap check comes first (`CapExceeded`), a mint at the large threshold is delayed (inclusive) and skips the windows. Live (`#[ignore]`): mainnet `fetch_fxrp_direct_mint_info` decodes pause + limits (cap 170M XRP, large threshold 4M XRP / 2 h matched the docs); the 2026-10-11 Coston2 smoke receipt decodes as `Minted` (4.8 FXRP) with the declared event ABI |
| TC-335 | P0 | `src/components/panels/wallet.rs` | FXRP Mint composer refuses an amount over the cap headroom (the XRP would wait in the Core Vault); an over-window (delayed) amount still queues |
| TC-336 | P2 | `src/components/panels/fxrp_direct_mint.rs` | FXRP panel `Max now` (min of window, large threshold, cap headroom) and `Cap used` rows; no limits → no rows |
| TC-337 | P1 | `src/flare.rs` | Redemption-queue summary (totals, whole lots, per-agent ordering, saturating sum, no lot size → null) and `getSettings()` word picker (field index, short answer, unexpected offset). Live smoke: `lazyxrp -x fxrp --tag N` on mainnet paged 4 × 20 tickets to `next = 0` (80 tickets, 1,541,120 XRP); tag 1 → `reserved: false` (the contract reverts `ERC721: invalid token ID`), tag 30 → `reserved: true` with the owner / recipient matching a raw `eth_call`, `pending_executor_change: null`. Only a revert means unreserved; transport errors propagate. The report also shows Core Vault holdings: Core Vault + queue matched `totalSupply` within ~7 XRP |
| TC-338 | P2 | `src/components/panels/flare_ftso.rs` | FTSO feed age label (`s` / `m` / `h`) and stale flag: 60 s is fresh, 61 s stale, timestamp ahead of now reads `0s`, timestamp 0 = `never` (stale) |
| TC-339 | P1 | `src/flare.rs`, `src/components/panels/fxrp_direct_mint.rs` | Smart Account vault positions: `fetch_smart_account_info` (local JSON-RPC mock, extends TC-241) lists a vault from `getVaults()` with its share balance and ERC-4626 `convertToAssets`, vaults with zero shares are skipped, a failing vault read is skipped; the FXRP panel renders `Vault N <Firelight / Upshift> <assets> FXRP`. Live (`#[ignore]`): mainnet `getVaults()` decodes 3 vaults (types 1, 2, 2) and a zero-balance account has no positions. Also: the FTSO live test asserts every feed timestamp is within 120 s of now |
| TC-340 | P0 | `src/fdc.rs` | FDC proof fetch: verifier `abiEncodedRequest` must match the asked tx / proof owner / `XRPPayment` type / source id (other tx, other owner, wrong type or source, wrong length refused); voting-round arithmetic; per-network endpoint defaults, https-only overrides, no Songbird. Live (`#[ignore]`, `FDC_LIVE_TX`): mainnet registry names, `firstVotingRoundStartTs` / 90 s epoch, a finalized past round, verifier answer for a real Core Vault payment, `getRequestFee` = 20 FLR |
| TC-341 | P0 | `src/flare_cmd.rs` | Redeem validation (zero, below `minimumRedeemAmountUBA`, above balance, tag width) and tag-address guards (zero / self). CLI smoke on mainnet with an unfunded throwaway key: no `execute` → refused, no `--yes` → refused, redeem 5 XRP → "exceeds the FXRP balance", 1 XRP → below minimum (5 XRP), `fxrp-tag-reserve` → FLR below the 100 FLR fee, `fxrp-tag-set 30` → owned by another address |
| TC-342 | P0 | `src/flare_cmd.rs` | Vault instruction layout `[id \| wallet \| value(10) \| 2 ignored \| vaultId \| 16 ignored]`; only `0x11–0x13` / `0x21–0x23` encode, zero value refused. CLI smoke: `sa-vault upshift deposit 1 …` → "vault 1 is type 1 … not 2"; `sa-vault firelight deposit 1 1000000` prints the memo and the 0.5 XRP fee, then stops at the mainnet `--yes` gate |
| TC-343 | P0 | `src/lib.rs` | `check_network_pair` (XRPL Mainnet ↔ Flare, Testnet ↔ Coston2; Songbird / Devnet / Xahau / crossed pairs refused) and `check_xrpl_tx_hash` (64 hex, optional `0x`; short / long / non-hex refused). Runs before the execute / key / `--yes` gates |
| TC-344 | P0 | `src/flare_cmd.rs` | Memo recovery layouts from upstream `MemoInstructions.sol`: `0xE0` / `0xE1` 42 B, `0xE2` 50 B, `0xD0` 30 B, `0xD1` 10 B (`[op \| walletId 0 \| fee u64 BE \| body]`); `0xE1` refuses `newNonce <= current` and a jump above `u32::MAX` (boundary tested), missing / short `--target`, zero executor; target state per op (`0xE0` / `0xE2` only while `isTransactionIdUsed` is false, `0xE1` only once true; `check_target_state`) |
| TC-300 | P0 | `src/xrpl/txspec/identity.rs` | Identity family field validation tables: CredentialType 64/65 bytes, URI 128/129, DID fields 256/257, at-least-one rules, Oracle id / provider / asset class / series 10/11, domain id (length, hex, zero hash), AcceptedCredentials 10/11 |
| TC-301 | P0 | `src/xrpl/txspec/mpt.rs` | MPT family field validation tables: TransferFee 50000/50001, AssetScale 255/256, MaximumAmount 0 / 1 / i64::MAX / +1, metadata 1024/1025, id 47/48/49, transfer-fee / domain / lock-unlock combinations |
| TC-302 | P0 | `src/xrpl/txspec/identity.rs` | Unsigned `tx_json` per identity type: text fields hex-encoded, `PriceDataSeries` → `PriceData` entries (decimal price → hex, scale), `AcceptedCredentials` → `Credential` entries, ids upper-cased, no Fee / LastLedgerSequence |
| TC-303 | P0 | `src/xrpl/txspec/mpt.rs` | Unsigned `tx_json` per MPT type: flag bits, metadata hex, ids upper-cased, unsupported optional fields absent |
| TC-304 | P0 | `src/xrpl/txspec/identity.rs` | Signed blobs of all 9 identity types decode with distinct Sequence / Fee / LastLedgerSequence and a signature |
| TC-305 | P0 | `src/xrpl/txspec/mpt.rs` | Signed blobs of all 4 MPT types decode with distinct Sequence / Fee / LastLedgerSequence and a signature |
| TC-306 | P0 | `src/xrpl/txspec/identity.rs`, `src/xrpl/txspec/mpt.rs` | Typed confirm phrases: `DELETE` on CredentialDelete / DIDDelete / OracleDelete / PermissionedDomainDelete, `DESTROY` on MPTokenIssuanceDestroy; none on the other types |
| TC-307 | P1 | `src/xrpl/txspec/identity.rs`, `src/xrpl/txspec/mpt.rs` | Builder applies the crate model rules: OracleSet without a series refused; MPTokenIssuanceSet / MPTokenAuthorize `Holder` = signer refused |
| TC-308 | P1 | `src/xrpl/txspec/identity.rs` | Flat grammars: `BASE/QUOTE=price@scale` (delete form, u64 bounds, scale 20/21, duplicate / base = quote) and `rIssuer:Type` credentials (hex-encoded type, duplicates, bad issuer, size) |
| TC-309 | P0 | `src/xrpl/txspec/identity.rs` | Generic submit of DIDSet vs local RPC mock: empty form never reaches RPC; `server_info` → `account_info` → `simulate` → `submit`, hex-encoded URI in simulate request and signed blob |
| TC-310 | P0 | `src/xrpl/txspec/mpt.rs` | Generic submit of MPTokenIssuanceCreate vs local RPC mock: transfer fee without canTransfer never reaches RPC; flags / metadata / Sequence / Fee / LastLedgerSequence in simulate request and signed blob |
| TC-088 / 190 / 234 | P0 | (extended) | TC-088 mainnet-gate table gains `generic_nft_mint` / `generic_nft_burn`; TC-190 `spec_subcommands_parse`; TC-234 `--watch` refuses `tx` / `nft-mint` / `nft-burn`, still allows `coverage` |
| TC-106 | P1 | `src/xrpl/submit.rs`, `src/app.rs` | Submit failure emits error + account resync; user actions mark dirty |
| TC-107 | P1 | `src/xrpl/submit.rs`, `src/app.rs` | Closed action channel returns failure without panic; ticks redraw |
| TC-108 | P1 | `src/app.rs` | Dirty-render — `run()` draws once per loop iteration when `needs_draw` (`draw_frame`: clears dirty, counts toward FPS). No automated test (needs a real terminal): smoke via PTY — splash and main UI must appear without a resize |
| TC-109 | P2 | `src/components/tabs/nft.rs` | NFT with URI emits `NftImageRequest` |
| TC-110 | P2 | `src/components/tabs/nft.rs` | NFT without URI emits no preview request |
| TC-111 | P1 | `src/network.rs` | Every network resolves distinct `https://`/`wss://` endpoints (xahau included) |
| TC-112 | P2 | `src/network.rs` | serde roundtrip for all networks including xahau variants |
| TC-113-114 | P2 | `src/config.rs` | `[flare]` `network`/`display` load from a config file (coston2 preset URL) and default to Flare/Full |
| TC-115 | P1 | `src/app.rs` | NetworkSwitchCycle updates `app.network`, net_tx, and StatusBar badge padding |
| TC-116 | P2 | `src/components/tabs/overview.rs` | Overview Off omits combined Flare panel |
| TC-117 | P2 | `src/components/tabs/overview.rs` | Overview Compact uses one-line FTSO/FXRP summaries (44/56 split kept) |
| TC-118 | P2 | `src/components/tabs/market_oracle.rs` | Market Off omits FTSO panel, keeps XRPL oracle |
| TC-119 | P2 | `src/components/tabs/market_oracle.rs` | Market Compact uses one-line FTSO summary |
| TC-120 | P2 | `src/config.rs` | `[flare.wallet] address` EVM validation + normalization |
| TC-121 | P2 | `src/components/panels/flare_wallet.rs` | Unconfigured wallet shows setup guidance |
| TC-122 | P2 | `src/components/panels/flare_wallet.rs` | Configured wallet renders balance rows from `FlareWalletBalance` |
| TC-123 | P2 | `src/xrpl/dunl.rs` | dUNL cache — hit within TTL, miss after clear |
| TC-124 | P2 | `src/flare.rs` | Flare registry cache — addresses stored per RPC URL |
| TC-125 | P1 | `src/cli.rs` | CLI `--network` accepts `xahau` / `xahau-test` (rejects clap-default `xahau-testnet`) |
| TC-126 | P1 | `src/cli.rs` | `--allow-insecure-rpc` defaults off; flag enables TUI and `rp` |
| TC-127 | P1 | `src/config.rs` | Config file `network = "xahau-test"` loads XahauTest (merged with TC-042 test) |
| TC-128 | P2 | `src/components/shared/theme.rs` | DESIGN.md frontmatter `colors.*` hex values match `theme.rs` constants |
| TC-129 | P1 | `src/components/panels/wallet.rs` | Fxrp proof preview truncates multibyte JSON by chars (byte-slice panic regression) |
| TC-130 | P1 | `src/components/panels/wallet_keys.rs` | `payment_edit_keys` input filters — IOU currency ≤3 chars, amount digits + single dot, destination graphic ASCII |
| TC-131 | P1 | `src/components/panels/wallet_keys.rs` | `account_set_edit_keys` — rows 0/1 locked, domain free text, tick/transfer digits only |
| TC-132 | P2 | `src/components/panels/wallet_composer.rs` | Composer row nav — `[`/`]` wrap, Enter advances while editing, `e` toggles edit |
| TC-133 | P1 | `src/components/shared/selectable_table.rs` | Selection clamps on `reset_len`, deselects at len 0, stops at ends |
| TC-134 | P1 | `src/components/shared/tx_detail/mod.rs` | Open overlay swallows Select keys (scroll, saturating) and returns true; hidden returns false |
| TC-135 | P2 | `src/components/shared/tx_detail/mod.rs` | `handle_panel_action` — TxDetailToggle opens overlay, unknown tx or hidden → false |
| TC-136 | P2 | `src/components/shared/status_bar.rs` | `freshness_label` boundaries — 0s/59s/1m/59m/1h; None keeps `-` path |
| TC-137 | P1 | `src/xrpl/parse.rs` | `book_offer_best_price` — string quality, invert, non-numeric → None, missing offers → None |
| TC-139 | P2 | `src/xrpl/format.rs` | `format_ripple_time_utc` — known value, epoch zero and u64::MAX clamp without panic |
| TC-140 | P2 | `src/xrpl/parse.rs` | Non-numeric quality rows render `price = "-"` without breaking numeric rows |
| TC-141 | P2 | `src/xrpl/format.rs` | `xrp_to_drops` max fractional boundary → u64::MAX; overflow → Err |
| TC-142 | P2 | `src/xrpl/toml.rs` | Matching validator returns attestation (case-insensitive) |
| TC-143 | P2 | `src/xrpl/toml.rs` | Invalid TOML → Err; missing VALIDATORS → count 0 |
| TC-145 | P1 | `src/xrpl/poll.rs` | Poll backoff escalates 0→2→4→8 and arms window; success resets to 0/None |
| TC-146 | P1 | `src/xrpl/nft_image.rs` | `fetch_resource` follows ≤5 redirects, rejects 6th; per-hop client revalidation |
| TC-147 | P1 | `src/xrpl/nft_image.rs` | `fetch_resource` rejects >4 MiB by header and by streamed bytes |
| TC-148 | P2 | `src/xrpl/dunl.rs` | dUNL parse error paths — missing blob/base64/JSON/sequence/expiration |
| TC-149 | P2 | `src/xrpl/dunl.rs` | dUNL expiration pin — exact ripple-epoch UTC string |
| TC-150 | P1 | `src/uninstall.rs` | `is_safe_uninstall_dir` HOME guard branches + `perform_self_uninstall` removes safe dirs |
| TC-151 | P2 | `src/components/panels/ledger_objects.rs` | `LedgerObjectFilter::keep` classifies PayChannel/Escrow/Misc rows |
| TC-152 | P2 | `src/components/shared/tx_detail/format.rs` | `format_value` truncates long objects on char boundaries (multibyte safe) |
| TC-153 | P2 | `src/components/panels/tx_history.rs` | TxHistoryPanel — filter input renders matching rows only |
| TC-154 | P2 | `src/components/panels/tx_history.rs` | TxHistoryPanel — append preserves filtered cache |
| TC-155 | P2 | `src/components/panels/tx_history.rs` | TxHistoryPanel — tx detail toggle shows selected tx JSON |
| TC-156 | P2 | `src/components/panels/tx_history.rs` | TxHistoryPanel — full history replace invalidates cached rows |
| TC-157 | P1 | `src/xrpl/cli_exec.rs` | All read-only CLI commands succeed against a deterministic local JSON-RPC server |
| TC-158 | P1 | `tests/cli_smoke.rs` | `lazyxrp` and `rp` binaries parse argv, return exit status, and render local-RPC output |
| TC-159 | P2 | `src/components/tabs/assets.rs` | Assets focus cycles NFT → objects → pay channels → escrows in both directions |
| TC-160 | P2 | `src/components/panels/account.rs`, `src/components/panels/book.rs` | Account panel renders loading and populated states; Book panel also renders its empty state |
| TC-161 | P2 | `src/components/panels/amm.rs` | AMM panel renders loading and populated pool summary |
| TC-162 | P2 | `src/components/panels/oracle.rs` | Oracle panel renders not-configured and price-table states |
| TC-163 | P2 | `src/components/panels/trust_lines.rs` | Trust-lines panel renders loading, empty, and populated states |
| TC-164 | P2 | `src/components/panels/path_find.rs` | Path-find panel renders loading and route summary states |
| TC-165 | P2 | `src/components/panels/server.rs` | Server panel renders loading and server metrics states |
| TC-166 | P1 | `src/xrpl/address.rs` | Payment destination resolve — empty/invalid rejected, X-address tag preserved, classic has no tag; X-address network guard rejects mismatched test/production and passes matching networks |
| TC-167 | P3 | `src/components/panels/server_dunl.rs` | `validator_row_label` — domain preferred when it fits, middle-truncates long domains, no-domain fallbacks to shortened public key |
| TC-168 | P3 | `src/components/panels/wallet_keygen.rs` | Keygen popup — renders nothing without keygen result; shows seed/address/pubkey + offline warning |
| TC-169 | P3 | `src/components/panels/server_detail.rs` | Validator detail — identity + dUNL + fetching state, verified TOML with rotated master + raw dump, missing manifest + TOML error state |
| TC-170 | P3 | `src/components/tabs/account_wallet.rs` | Wallet detection — seed-configured, mnemonic-configured, and no-signing-config branches |
| TC-171 | P3 | `src/components/shared/splash.rs` | Splash helpers — `splash_art_column` centering/width edges, `splash_ascii_lines_for_height` fits/tight |
| TC-172 | P2 | `src/flare.rs` | FTSO feed normalization — trims, drops empties, maps `FXRP/USD` → `XRP/USD`, dedups preserving order. Batched `getFeedsById` fetch with fee-aware retry and unknown-feed failure are covered only by `#[ignore]` live tests |
| TC-173 | P2 | `src/xrpl/dunl.rs`, `src/xrpl/poll.rs` | dUNL cache — stale-on-error serves cached summary during 60 s retry cooldown, none after clear; tab gates — account panels poll only on Account tab (market/assets gates unchanged) |
| TC-174 | P2 | `src/components/shared/status_bar.rs` | Status bar health — staleness-aware XRPL chip (STALE after 30 s, `server_info` error = OFFLINE), flare dot (enabled/failed/stale), dUNL data-age marker |
| TC-175 | P1 | `src/lib.rs` | Flare RPC URL — `http://` rejected without `--allow-insecure-rpc`; refused for signing even with it |
| TC-176 | P1 | `src/signing.rs` | Node-chosen Fee > `MAX_NODE_FEE_DROPS` rejected before signing (cap itself accepted) |
| TC-177 | P1 | `src/xrpl/toml.rs`, `src/xrpl/nft_image.rs` | toml domain accepts only plain hostnames; private/reserved literals (incl. `100.64/10`, `198.18/15`, `240/4`) refused with no request |
| TC-178 | P1 | `src/components/shared/fmt.rs`, `src/xrpl/format.rs`, `src/flare.rs` | Remote multibyte strings / NaN amounts / oversized uint256 do not panic display helpers or sort |
| TC-179 | P1 | `src/xrpl/client.rs` | Chunked body over 16 MiB is rejected while streaming |
| TC-180 | P2 | `src/xrpl/parse.rs` | Control characters stripped from submit error text and tx hash |
| TC-181 | P2 | `src/config.rs` | Group/world-readable credential config yields a (stderr) warning; 0600 does not |
| TC-182 | — | `install.sh`, `.github/workflows/cd.yml` | Manual: token not in argv (stub-curl check); `cargo package` token-free then `publish --no-verify` |
| TC-183 | P2 | `src/xrpl/parse.rs` | `account_offers` rows — lowercase keys mapped; entries without u32 `seq` skipped |
| TC-184 | P1 | `src/signing.rs` | OfferCancel simulate JSON + signed blob carry `OfferSequence`/`Sequence` |
| TC-185 | P1 | `src/components/panels/wallet.rs`, `src/xrpl/submit.rs` | OfferCancel composer — `8` fetches offers, `j`/`s` queue selected seq, no-selection error, fetch error rendered; mainnet without `--yes` rejected (TC-088 also covers EscrowCreate / EscrowFinish·Cancel) |
| TC-186 | P1 | `src/signing.rs` | Payment SendMax — set for cross-currency (no `Paths`), XRP→XRP SendMax rejected |
| TC-187 | P1 | `src/signing.rs`, `src/components/panels/wallet.rs`, `wallet_keys.rs` | `send_max_xrp_drops` (XRP→drops, IOU→0, invalid→Err); composer Send max row editing, trimmed/empty→`None`, preview `spend ≤` |
| TC-188 | P1 | `src/xrpl/cli_exec.rs` | CLI `send` — local JSON-RPC sees exactly `server_info → account_info → simulate → submit` (network check first; no unsimulated signing) |
| TC-189 | P1 | `src/lib.rs` | Command resolve — bare → TUI keeps `--account`; bare subcommand errors with `lazyxrp -x <name>`; `-x` alone errors; `-x info` → Info |
| TC-190 | P1 | `src/cli.rs` | DEX/faucet CLI parse — `offers <ADDR>`, `offer-cancel <SEQ>`, `offer-create <GETS> <PAYS>`, `faucet [ADDR]`; non-numeric seq rejected |
| TC-191 | P1 | `src/xrpl/cli_exec.rs` | DEX CLI — `offers` reads `account_offers`; `offer-cancel` without key fails before RPC; bad `offer-create` spec fails before RPC (`TakerGets:`); with key both run `server_info → account_info → simulate → submit` |
| TC-192 | P1 | `src/xrpl/format.rs` | Quote source currency — Send max prefix wins, IOU Amount defaults to XRP, XRP Amount needs non-XRP prefix, IOU prefix needs issuer |
| TC-193 | P1 | `src/xrpl/format.rs` | Quote → Send max spec — cheapest alternative, slippage rounded up (drops / 6 sig. digits), no path → Err |
| TC-194 | P1 | `src/components/panels/wallet.rs` | Payment `p` — invalid form flashes; valid form queues `PaymentQuote` (default 100 bps); QuoteOk fills Send max; QuoteErr flashes |
| TC-195 | P2 | `src/config.rs` | User `[keybindings.Splash]` adds keys and overrides defaults; untouched defaults kept |
| TC-196 | P0 | `src/xrpl/cli_exec.rs` | CLI `send` — rejected submit (`tefPAST_SEQ`) returns Err → non-zero exit |
| TC-197 | P1 | `src/signing.rs` | `validate_payment_input` — destination checksum / X-address, positive amount, IOU currency+issuer, SendMax spec (XRP→XRP rejected, blank = none) |
| TC-198 | P1 | `src/components/panels/wallet.rs` | Payment preview + `s` share validation — bad checksum / bad Send max stay orange and submit refuses with flash |
| TC-199 | P1 | `src/xrpl/submit.rs`, `src/test_support.rs` | Poll write paths vs local JSON-RPC — Payment / OfferCancel `server_info → account_info → simulate → submit` → Ok; quote sends `source_currencies`, returns slippage spec |
| TC-200 | P0 | `src/xrpl/parse.rs` | `simulate` without `LastLedgerSequence` (rippled 3.4.1 shape) → filled `ledger_index + 20` so signing proceeds; server value kept |
| TC-201 | P1 | `src/xrpl/cli_exec.rs` | `faucet` — testnet/devnet URLs only (mainnet/xahau refused); POSTs `destination`, reports amount + hash; response naming another address → Err |
| TC-202 | P1 | `src/xrpl/cli_exec.rs` | CLI write wait — polls `tx` until validated (`tes` → Ok, `tec` → Err with code); not found after validated ledger > LLS → Err (expired); node that never advances / omits `validated_ledger` → Err (timed out, 5 min cap) |
| TC-203 | P0 | `src/network.rs`, `src/xrpl/cli_exec.rs`, `src/xrpl/submit.rs` | Endpoint `network_id` must match the selected network before signing (mainnet omits = 0); mainnet node under `--network testnet` refused after `server_info` only (CLI); devnet session on a testnet node refused (TUI) |
| TC-204 | P0 | `src/xrpl/parse.rs`, `src/xrpl/submit.rs` | simulate `Sequence` ≠ request or `LastLedgerSequence` ≤ / > `ledger_index + 100` → refused before signing (all write paths) |
| TC-205 | P0 | `src/flare.rs` | FXRP C2 Core Vault: signed only on exact match with `expected_core_vault`; mismatch / unset → refused |
| TC-206 | P1 | `src/xrpl/format.rs` | Quote picks only alternatives in requested Send max currency/issuer; mismatch → Err; XRP slippage overflow → Err (TC-193 extended) |
| TC-207 | P1 | `src/signing.rs`, `src/xrpl/submit.rs` | Submitted tx hash = SHA-512Half(`TXN\0` ‖ blob), independent vector; node-reported hash ignored |
| TC-208 | P1 | `src/xrpl/parse.rs` | simulate `engine_result(_message)` control characters stripped |
| TC-209 | P1 | `src/xrpl/cli_exec.rs` | CLI wait: tx validated after ledger passes LLS is re-checked once → Ok, not expired (TC-202 extended) |
| TC-210 | P1 | `src/xrpl/client.rs` | JSON-RPC endpoint answering 307 → Err; request not re-posted to the redirect target |
| TC-211 | P1 | `src/signing.rs` | `XRP:<drops>:<tail>` rejected (SendMax and OfferCreate specs) |
| TC-212 | P1 | `src/xrpl/cli_exec.rs` | `-x send 0` refused by `validate_payment_input` before any RPC |
| TC-213 | P1 | `src/components/panels/wallet_keygen.rs`, `src/config.rs` | Keygen popup cleared on NetworkChange, label "Esc to dismiss"; `Config::new` takes + removes the Flare EVM key env (TC-049 extended) |
| TC-214 | P1 | `src/xrpl/submit.rs` | TUI submit result = `<local hash> · fee N drops` (simulate fee that was signed) |
| TC-215 | P1 | `src/signing.rs` | Escrow duration `90s`/`30m`/`2h`/`7d` → seconds; empty, unitless, zero, negative, fractional, unknown unit, u32 overflow rejected |
| TC-216 | P1 | `src/signing.rs` | EscrowCreate input — classic Destination, XRP > 0, FinishAfter required, CancelAfter optional and strictly later |
| TC-217 | P1 | `src/signing.rs` | EscrowCreate / EscrowFinish / EscrowCancel simulate JSON + signed blobs carry Amount/Destination/FinishAfter/CancelAfter and Owner/OfferSequence |
| TC-218 | P1 | `src/xrpl/parse.rs` | Escrow rows from `account_objects` (owner, destination, times, condition, object `Sequence`); creating tx → `Sequence`, or `TicketSequence` when 0 (API v1 and v2 shapes) |
| TC-219 | P1 | `src/components/panels/wallet.rs` | EscrowCreate composer — `9` opens; invalid form flashes; valid form queues trimmed params; Esc → picker row 9 |
| TC-220 | P1 | `src/components/panels/wallet.rs` | Escrows composer — `0` fetches; `s` finish / `c` cancel selected; no selection, unresolved seq, finish of a condition escrow refused with flash |
| TC-221 | P1 | `src/xrpl/submit.rs` | Escrow paths vs local JSON-RPC — Create simulates the absolute FinishAfter (now + 1h) / CancelAfter it signs; Finish carries Owner/OfferSequence; `account_escrows` resolves seq via `tx` |
| TC-222 | P1 | `src/signing.rs` | AMM input table — deposit: same asset / zero amount rejected; withdraw: same asset, missing / bad issuer, amount in asset field, LP `0` / non-numeric rejected; empty LP = withdraw all OK |
| TC-223 | P1 | `src/signing.rs` | AMM tx JSON + signed blobs — deposit `tfTwoAsset` with Asset/Asset2 from amounts; withdraw all `tfWithdrawAll`; LP `tfLPToken` + `LPTokenIn` |
| TC-224 | P2 | `src/xrpl/parse.rs` | `amm_info` LP token identity (`lp_token` or `LPToken`) → `(currency, AMM account)` |
| TC-225 | P1 | `src/components/panels/wallet.rs` | AMM composers — `d` / `w` open; invalid form flashes `AMM ·`; valid forms queue trimmed params; empty LP = withdraw all; Esc → picker rows 10 / 11 |
| TC-226 | P1 | `src/xrpl/submit.rs` | AMM paths vs local JSON-RPC — deposit simulates `tfTwoAsset`; withdraw by LP calls `amm_info` before simulate and signs `LPTokenIn` with its currency / issuer |



## Traceability

| TC | FR / NFR | Architecture § | Design |
|----|----------|------------------|--------|
| TC-111–112 | FR-12 | §6.1 presets | §6 `Ctrl-n` |
| TC-113–114 | FR-15 | §4 Flare config | §4 Flare layouts |
| TC-115 | FR-12 | §6.1 session switch, §6.5 badge | §5 Status bar, §6 `Ctrl-n` |
| TC-116–119 | FR-15 | §6.7 | §4 Flare layouts |
| TC-120–122 | FR-16 | §6.8 Flare wallet panel | §4 Overview wallet strip |
| TC-123–124 | — | §3 public-network politeness | — |
| TC-125–127 | FR-12 | §6.1 presets, §4 CLI | — |
| TC-128 | — | — | §2 Color + YAML frontmatter |
| TC-129–135 | FR-16 | §2 wallet composer, §5.2 shared behaviors | §6 wallet/composer keys |
| TC-136 | — | §6.5 status bar | §5 Status bar |
| TC-137, 139–141 | — | §5 read APIs | §5 Table / TX row |
| TC-142–143 | — | §3 public-network politeness | — |
| TC-144–145 | — | §3 poll loop | — |
| TC-146–147 | NFR (SSRF) | §3 SSRF fetch guards | — |
| TC-248–256 | FR-18 | §2 Transaction composer, §2.1 txspec + coverage | §6 composer `m`, NFT `x` |
| TC-148–149 | — | §3 public-network politeness | — |
| TC-150 | — | §1 self-uninstall | — |
| TC-151–152 | — | §5.4 ledger objects, §5.2 tx detail | §4 Assets / tx detail |
| TC-153–165 | — | §5.1 Assets panels, §5.2 tx history, §4 CLI | §4 Assets tx history |
| TC-166–171 | — | §2 address guards, §3 dUNL | §5 (splash) |
| TC-172–174 | — | §3 public-network politeness | §5 Status bar |
| TC-239–247 | FR-17 | §6.9 Flare Smart Accounts, §2 wallet composer | §4 Overview FXRP rows, §6 composer `a` |

## Risk coverage

| Risk | Covered by | Evidence |
|------|------------|----------|
| R-006 mainnet write guard bypass | TC-088 `mainnet_submit_without_yes_is_rejected` | `src/xrpl/submit.rs` |
| R-007 config merge precedence | TC-033-035/TC-092 `config_source_precedence`, TC-036 | `src/config.rs` |
| R-001 seed priority chain | TC-045/046/049 `credential_from_secrets` + env override | `src/signing.rs`, `src/config.rs` |
| R-016 endpoint network ≠ selected network | TC-203 `endpoint_network_id_must_match` + CLI/TUI refusal | `src/network.rs`, `src/xrpl/cli_exec.rs`, `src/xrpl/submit.rs` |
| R-017 Smart Account operator / fee from one RPC answer | TC-240, TC-243 (`check_operator`, `smart_account_instruction_refusals_never_reach_rpc`); mainnet gate TC-088 | `src/flare.rs`, `src/xrpl/submit.rs` |

## Testnet E2E

**Automated (opt-in):** `mise run e2e-testnet` (`tests/testnet_e2e.rs`, `#[ignore]`, not in CI): `-x faucet` ×2 → `send` → `offer-create` → `offers` → `offer-cancel` → `offers` = `[]`, every write `Validated: tesSUCCESS`. Run before each release. Passed 2026-10-11 (≈ 27 s).

**NFT mint → burn (opt-in, `testnet_nft_mint_burn_validate`, same file):** faucet account → `nft-mint --taxon 4242 --flags burnable,transferable --transfer-fee 500` → `nfts` shows it → `nft-burn <id>` → `nfts` empty, all `Validated: tesSUCCESS`. See the run note below.

**Run note (2026-10-16):** on testnet, 12 attempts stopped at the first write's simulate with `[XRPL-11] Fee of … exceeds the requested tx limit of 100`. Cause measured, not in lazyxrp: the `fee` RPC reported `open_ledger_fee` 6509–27343 drops (base 10, `load_factor` ≈ 501, ledger 5651 txs vs expected 4953), and a bare `simulate` of an AccountSet sent outside lazyxrp fails identically; `simulate` ignores `fee_mult_max`, so only an explicit `Fee` passes. Re-run on **devnet** (quiet, open-ledger fee 1 drop) via `LAZYXRP_E2E_NETWORK=devnet mise run e2e-testnet`. Run in parallel, the mint got `telCAN_NOT_QUEUE` once, so the task now passes `--test-threads=1`; the full task then passed (2 passed, 40 s): send / offer-create / offer-cancel and NFT mint → `nfts` → burn → `nfts` empty, every write `Validated: tesSUCCESS`. Earlier single-test run: mint `75786BFA29DC04B4834F15546D3EB7112CE5D6B09882E7B85078E7769C1FEB62`, burn `B3DC83C16DFFC498704D90204251590B3D9D6A02C2187E717C06F754338C41D9`. Only NFT Mint/Burn are live-verified; the other spec families are offline-tested only. Known gap: under heavy fee escalation the app cannot simulate (no `Fee` is supplied to `simulate`).

**AMM (manual, 2026-10-11):** pool XRP/TST created on testnet by a throwaway xrpl.js script (lazyxrp has no AMMCreate). CLI `-x amm-deposit XRP:1000000 TST:<iss>:10` and `-x amm-withdraw XRP TST:<iss> --lp 1000`, then TUI composer `d` (deposit, Amount2 limited the XRP side to 0.5 XRP) and `w` with empty LP (withdraw all; the sole LP's withdrawal deleted the pool): all `tesSUCCESS`.

**FXRP C2 + C3 (manual, 2026-10-11, XRPL testnet + Coston2):** config `[flare] network = "coston2"`, `[flare.fassets] execute = true`, `expected_core_vault = "rDhpmiPq4BVBDWMVdSrmkgt8thKyRzGV1p"`, `FLARE_EVM_KEY` = executor `0xeD50…2C98`.

| Step | Result | Hash |
|------|--------|------|
| TUI composer `6` FXRP Mint, 10 XRP, recipient = executor | first try failed at simulate (`MemoFormat` null → TC-230); after the fix `tesSUCCESS`; a public executor finalized it within minutes → FXRP 9.8 | `FB8556096C0BF6AE25F858DB9AF65CC0381E51FF100505672229EAACCE5A1A78` |
| TUI `7` C3 with that payment's proof | revert `PaymentAlreadyConfirmed` (0x18dce79f) — already minted by the public executor | — |
| 5 XRP payment with a 48-byte memo (executor = ours, throwaway xrpl.js) → proof (verifier `prepareRequest` with the Flare-published testnet key, `FdcHub.requestAttestation`, DA `proof-by-request-round`; throwaway viem script) → TUI `7` C3 | `executeDirectMinting` succeeded; FXRP 9.8 → 14.7 (4.9 = 5 − 0.1 min fee; executor fee to ourselves) | XRPL `C0FC83E57C37BC4782B48E4CB4EB0123829137B52A4C1CC8B6F57E38B548AE6A`, Flare `0x1460dc9ae36bbe58beb4a8fd1a6d6bd358d4fd3f5a315583646cd31d111e5f02` |

Before the fix, C3 called a selector the contract does not have (`0x548cddd4`, `bytes data`); the deployed one is `0x78d0299e` (decoded `IXRPPayment.Response`) → TC-231.

**AccountDelete (manual, 2026-10-11, testnet):** two fresh `-x faucet` accounts (funded at ledger 21,447,697), destination = account B `rLo2…dnJ`.

| Step | Result | Hash |
|------|--------|------|
| CLI, 12 min after funding, dest = self / dest = B | `simulate temDST_IS_SRC` / `simulate tecTOO_SOON` (nothing signed) | — |
| TUI composer `x`, X-address of B with tag 777 → `DELETE` | `tesSUCCESS` at ledger 21,447,957; tx has `DestinationTag: 777`, `Fee: 200000` (owner reserve, filled by simulate). Wallet panel then shows "account not found on this network" | `1A92B4DF3AF48E11359443FF5F2A7EF0A5C96DD3A912A0717A07D9E6882958AE` |
| CLI `-x account-delete <B>` | `Validated: tesSUCCESS`; `-x account` on the deleted address → `Account not found` | `6B596EBA18F7BF35C6D8C04E78D79F90E2040A1B9E5E6FBEC2882DB62A9E421E` |

**2026-10-10 CLI (manual, 0.3.0)** — testnet rippled 3.4.1, faucet account `r15nczQYUNYBVLws8M9zkFE2SaSkU4kkk`. All hashes `validated: true`, `tesSUCCESS`.

| Step | Result | Tx hash |
|------|--------|---------|
| `lazyxrp -x send rpvyTEMx… --amount 1.5` | exit 0 | `8F3647ED55D574932C956EB81DEA093260C33F320D40C07DFEF9F1FF28CA8962` |
| `lazyxrp -x offers r15ncz…` | lists seq 21430112 | — |
| `lazyxrp -x offer-cancel 21430112` | exit 0; `offers` → `[]` | `4BA39A804683036C1D9B61478D3596555C1DF68FE31187577C8057AA3C8AE64D` |
| `offer-cancel` on the already-removed offer | exit 0 (XRPL no-op `tesSUCCESS`) | `0620179B9177C20E333E7C2D85093F642BA1454C0B329D49F3BC6C62CC404173` |
| `send` to a bad-checksum address | exit 1, no RPC write | — |

First run failed every write with `simulate tx_json missing LastLedgerSequence` (mocks returned LLS; live simulate does not) → fixed, TC-200.

**2026-10-11 TUI (driven via tmux)** — sender `rwJXoAen3drVz6tep2CzXz7jFwS7G8pKnz`; issuer B `rwENtdso…` (DefaultRipple, offer 100 USD for 100 XRP); destination D trusts B. All `validated: true`, `tesSUCCESS`.

| Step | Result | Tx hash |
|------|--------|---------|
| Composer `4` OfferCreate `XRP:1000000` → `USD:B:1` | crossed B's offer at once (A received 1 USD); OfferCancel list empty — correct | `FB22BA340747D44D4A39805081302F930BA0EEA808C2C60C6E8D4859930C7465` |
| Composer `1` Payment IOU 1 USD.B → D, `p` | Send max filled `XRP:1010000` (1 XRP + 1 %) | — |
| `s` cross-currency Payment | delivered 1 USD | `2FE724657854C59A5E87E0AD1D843B115D197E9F9CCEF2FC020ED18E6023401D` |
| Composer `4` OfferCreate `XRP:1000000` → `USD:B:100` (no cross) | offer `#21432418` listed in `8` | `3FDB2522AC0E8A698C01D5C49BF962BFC0F8C958AE8A91E5B668215D519C6F6E` |
| Composer `8` → `s` | offer gone (`(no open offers)`) | `DB7A79BDFCB81373FB98BDD40886A9593A986D2FC3F11D0EDE6833297CAF4EA8` |

Notes: the first quote returned "no payment path found" — correct, the issuer lacked DefaultRipple (`simulate` = `tecPATH_DRY`). **S-017** found here: a mainnet `XRPL_RPC_SERVER` in the shell silently overrode `--network testnet` (reads hit mainnet) → TC-203; live check: mainnet endpoint + `--network testnet` refused (exit 1), testnet endpoint validated `868477854FBAD38A1B22629659BA4A98DC945E994422728DE99805BBF43B6209`.

**2026-10-11 TUI escrow (PTY driven, pyte screen)** — owner `rGyZL1VDvkzBUWY7rwJ4BaiXRaxH85wHAR`, destination `rLo2GiHSRivGN17CZhJGFpBbYGXEACpdnJ` (both `-x faucet`). All `tesSUCCESS` in Tx History.

| Step | Result | Tx hash |
|------|--------|---------|
| Composer `9` EscrowCreate 1 XRP, FinishAfter `90s` | owner 1, list `#21443113` (seq resolved) | `4A7D8FE1756DB468A7B996A8F28E471D8E5E8A655D2E7993B142685CA31812DA` |
| Composer `9` EscrowCreate 1 XRP, `60s` / CancelAfter `120s` | list `#21443114` with finish + cancel times | `40ADC91F1B566A615761EFD8C10293765D3C1461D8852EA677797850DDFB7A8A` |
| `0` → `s` before FinishAfter | `simulate tecNO_PERMISSION` flash, nothing signed | — |
| `0` → `c` on `#21443114` after CancelAfter | refunded, owner 2 → 1 | `4B0A8C61AA9AB183A3D7E2F7CF61C112116DF762F8DF10B73BD8C2E2C45A8603` |
| `0` → `s` on `#21443113` | finished, owner 0, list `(no escrows)` | `D29C54F24D0AF9DBCE735475FA0270578476C313BE4D0B0A81A303D88FAF1B54` |

Remaining gaps (unimplemented by design — low priority):

- **R-002** submit channel-close observability: `send_action` は warn ログのみ。永続化は未実装。
- **R-003** `ArcValue` 共有 JSON 不変性: 規約のみ。違反時は視覚破損で検知。
