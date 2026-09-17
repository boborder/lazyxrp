# Test Strategy & Registry

**Role:** テスト設計・テストガイド（重要 TC カタログと FR/architecture へのトレース）。

> Last Updated: 2026-09-17 (traceability § pointers resynced to architecture.md / DESIGN.md headings)
> Target: lazyxrp (Rust TUI for XRPL)
> Catalog: important-case roster (not a 1:1 index of every `#[test]`)

## Policy

- This file is an **important-case roster**: a TC is added only for cross-module contracts, regression-prone logic, or user-visible behavior. Unannotated helper/unit tests are intentionally outside the catalog.
- Reference the `TC-xxx` ID in commit messages (e.g. `test(xrpl): add NFT parse case (TC-013)`).
- Inline `#[cfg(test)]` modules preferred; live-network cases are `#[ignore]` and run explicitly.
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
| TC-004 | P1 | `src/xrpl/util.rs` | json_str — nested path returns value |
| TC-005 | P1 | `src/xrpl/util.rs` | json_str — missing path returns empty |
| TC-006 | P1 | `src/xrpl/util.rs` | json_u32 — returns number |
| TC-007 | P1 | `src/xrpl/util.rs` | json_u32 — missing or non-numeric returns zero |
| TC-008 | P0 | `src/xrpl/format.rs` | drops_to_xrp — basic conversion |
| TC-009 | P1 | `src/xrpl/format.rs` | drops_to_xrp — invalid string returns zero |
| TC-010 | P1 | `src/xrpl/format.rs` | format_amount — None returns dash |
| TC-011 | P1 | `src/xrpl/format.rs` | format_amount — XRP drops string |
| TC-012 | P1 | `src/xrpl/format.rs` | format_amount — issued currency object |
| TC-013 | P1 | `src/xrpl/parse.rs` | account_nfts — parse response into NftRow vec |
| TC-014 | P1 | `src/xrpl/parse.rs` | account_lines — parse response into TrustLineRow vec |
| TC-015 | P2 | `src/xrpl/parse.rs` | amm_info — parse response into AmmSummary |
| TC-016 | P2 | `src/xrpl/parse.rs` | account_tx — parse response into TxRow vec (with direction) |
| TC-017 | P1 | `src/xrpl/parse.rs` | book_offers — parse response into OfferRow vec |
| TC-018 | P2 | `src/xrpl/parse.rs` | server_info/fee — parse response into summary structs |
| TC-019 | P2 | `src/config.rs` | parse_style — empty string yields default |
| TC-020 | P2 | `src/config.rs` | parse_style — foreground color |
| TC-021 | P2 | `src/config.rs` | parse_style — background color |
| TC-022 | P2 | `src/config.rs` | parse_style — modifiers combined |
| TC-023 | P2 | `src/config.rs` | extract_color_and_modifiers — extracts modifiers and color |
| TC-024 | P2 | `src/config.rs` | parse_color — RGB shorthand |
| TC-025 | P2 | `src/config.rs` | parse_color — unknown returns None |
| TC-026 | P0 | `src/config.rs` | Config::new — loads default keybindings |
| TC-027 | P1 | `src/config.rs` | parse_key_event — simple keys |
| TC-028 | P1 | `src/config.rs` | parse_key_event — with modifiers |
| TC-029 | P1 | `src/config.rs` | parse_key_event — multiple modifiers |
| TC-030 | P2 | `src/config.rs` | key_event_to_string — reverse mapping |
| TC-031 | P1 | `src/config.rs` | parse_key_event — invalid keys error |
| TC-032 | P1 | `src/config.rs` | parse_key_event — case insensitivity |
| TC-033 | P1 | `src/config.rs` | Config merge — user overrides default (`poll_interval_ms` fixture) |
| TC-034 | P1 | `src/config.rs` | Config merge — XDG directory resolution |
| TC-035 | P2 | `src/config.rs` | Config merge — fallback to ~/.config |
| TC-036 | P2 | `src/config.rs` | Config validation — invalid key sequence format |
| TC-037 | P0 | `src/network.rs` | Network::default — is Mainnet |
| TC-038 | P0 | `src/network.rs` | Network::from_str — roundtrip mainnet/testnet/devnet |
| TC-039 | P1 | `src/network.rs` | Network::from_str — case-insensitive |
| TC-040 | P1 | `src/network.rs` | Network::from_str — unknown network errors |
| TC-041 | P1 | `src/network.rs` | Network::is_production — correct boolean |
| TC-042 | P0 | `src/lib.rs` | resolve_network — CLI flag overrides env |
| TC-043 | P0 | `src/lib.rs` | resolve_rpc_url — mainnet default when no CLI/env/config override |
| TC-044 | P1 | `src/lib.rs` | resolve_ws_url — CLI --ws-server is top priority |
| TC-045 | P1 | `src/signing.rs` | credential_from_secrets — no source returns none |
| TC-046 | P1 | `src/signing.rs` | credential_from_secrets — family seed wraps SigningCredential |
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
| TC-058 | P1 | `src/cli.rs` | CLI — invalid parameters error |
| TC-059 | P2 | `src/xrpl/cli_exec.rs` | CLI — invalid classic account address fails before RPC connect |
| TC-060 | P1 | `src/app.rs` | Watch mode — App builds 4 tabs with watch account (I-9) |
| TC-061 | P1 | `src/app.rs` | Watch mode — Quit action stops background tasks |
| TC-062 | P2 | `src/app.rs` | Watch mode — RefreshAccount sends PollCommand |
| TC-063 | P2 | `src/app.rs` | Watch mode — RefreshBook sends PollCommand |
| TC-064 | P2 | `src/app.rs` | Watch mode — TabNext/TabPrev wrap around panels |
| TC-065 | P3 | `src/app.rs` | Watch mode — HelpOverlay toggles on `?` and Esc |
| TC-066 | P1 | `src/xrpl/cli_exec.rs` | CLI — `account-status` against local JSON-RPC |
| TC-067 | P2 | `src/xrpl/cli_exec.rs` | CLI — `send` manual live-network check only (`cli_send_live_manual_only`; requires `XRPL_SEED`) |
| TC-068 | P1 | `src/xrpl/client.rs` | XRPL RPC error — not found is not silently swallowed |
| TC-069 | P1 | `src/xrpl/parse.rs` | XRPL submit response — requires `tesSUCCESS` and hash |
| TC-070 | P1 | `src/xrpl/types.rs` | book_offers — issued quote uses `currency_code` |
| TC-071 | P1 | `src/xrpl/parse.rs` | account_objects — empty array parses |
| TC-072 | P1 | `src/xrpl/parse.rs` | account_objects — mixed Check / Ticket / MPT / PayChannel / Escrow |
| TC-073 | P1 | `src/xrpl/parse.rs` | ledger object filters — tab visibility helpers |
| TC-074 | P1 | `src/xrpl/parse.rs` | account_nfts — tfMutable flag (dNFT) |
| TC-075 | P2 | `src/xrpl/parse.rs` | xrpl-rust `Payment<'static>` deserialize from JSON |
| TC-076 | P1 | `src/cli.rs` | CLI — `--self-uninstall` flag parses; backup path helper |
| TC-077 | P2 | `src/components/panels/tx_history.rs` | TxHistoryPanel — filter by tx_type |
| TC-078 | P2 | `src/components/panels/tx_history.rs` | TxHistoryPanel — filter by hash partial |
| TC-079 | P2 | `src/components/panels/tx_history.rs` | TxHistoryPanel — transaction filter |
| TC-080 | P2 | `src/xrpl/parse.rs` | ripple_path_find — parse alternatives |
| TC-081 | P2 | `src/xrpl/parse.rs` | ripple_path_find — empty alternatives |
| TC-082 | P2 | `src/xrpl/parse.rs` | ripple_path_find — `source_amount` as XRP drops string |
| TC-083 | P2 | `src/xrpl/format.rs` | summarize_paths_computed — hop chain abbreviation |
| TC-084 | P2 | `src/xrpl/format.rs` | path_find_rows_from — display rows |
| TC-085 | P2 | `src/xrpl/types.rs` | asset_display_name — known hex codes |
| TC-086 | P2 | `src/xrpl/types.rs` | asset_display_name — unknown passthrough |
| TC-087 | P2 | `src/xrpl/poll.rs` | Poll trigger — coalesces rapid bursts |
| TC-088 | P0 | `src/xrpl/poll.rs` | Mainnet guard: all submit paths reject without `--yes` (R-006) |
| TC-089 | P0 | `src/xrpl/parse.rs` | account_tx not-found → empty history (I-7) |
| TC-092 | P1 | `src/config.rs` | Config merge — `XRPL_RPC_SERVER` overrides `rpc_server` from file |
| TC-093 | P1 | `src/components/shared/tx_detail/mod.rs` | TxDetail render cache — invalidated on open |
| TC-094 | P2 | `src/components/shared/tx_detail/parsers.rs` | TxDetail parser registry — required types, no duplicates |
| TC-096 | P2 | `src/xrpl/parse.rs` | get_aggregate_price — full response |
| TC-097 | P2 | `src/xrpl/parse.rs` | get_aggregate_price — trimmed_set omitted |
| TC-098 | P1 | `src/signing.rs` | IOU validation — invalid issuer / non-finite / zero amount rejected |
| TC-099 | P1 | `src/signing.rs` | IOU validation — XRP / overlong currency code rejected, 3-letter ok |
| TC-100 | P2 | `src/xrpl/nft_image.rs` | NFT metadata traversal stops at depth limit |
| TC-101 | P1 | `src/lib.rs` | Signing seed refuses plaintext RPC/WS endpoints |
| TC-102 | P1 | `src/xrpl/client.rs` | Rate-limited RPC errors retryable; permanent XRPL errors not |
| TC-103 | P2 | `src/xrpl/ws.rs`, `src/components/shared/fps.rs` | Duplicate ledger-close emits one trigger; FPS counts draws not ticks |
| TC-104 | P1 | `src/xrpl/parse.rs`, `src/components/shared/fps.rs` | account_tx marker survives parsing; tick-rate label refreshes after 1s |
| TC-105 | P2 | `src/xrpl/client.rs`, `src/app.rs` | Local HTTP 429 retried; dirty-render skips no-op Render |
| TC-106 | P1 | `src/xrpl/poll.rs`, `src/app.rs` | Submit failure emits error + account resync; user actions mark dirty |
| TC-107 | P1 | `src/xrpl/poll.rs`, `src/app.rs` | Closed action channel returns failure without panic; ticks redraw |
| TC-108 | P1 | `src/app.rs` | Dirty-render — actual draw clears dirty, counts toward FPS |
| TC-109 | P2 | `src/components/tabs/nft.rs` | NFT with URI emits `NftImageRequest` |
| TC-110 | P2 | `src/components/tabs/nft.rs` | NFT without URI emits no preview request |
| TC-111 | P1 | `src/network.rs` | Xahau networks parse, resolve endpoints, and guard writes |
| TC-112 | P2 | `src/network.rs` | serde roundtrip for all networks including xahau variants |
| TC-113 | P2 | `src/config.rs` | FlareNetwork parses case-insensitively, defaults to Flare, presets per chain |
| TC-114 | P2 | `src/config.rs` | FlareDisplay parses and defaults to full |
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
| TC-127 | P1 | `src/config.rs` | Config file `network = "xahau-test"` loads XahauTest |
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
| TC-138 | P2 | `src/xrpl/format.rs` | `drops_to_xrp` precision collapse beyond 2^53 pinned as current spec |
| TC-139 | P2 | `src/xrpl/format.rs` | `format_ripple_time_utc` — epoch zero and u64::MAX clamp without panic |
| TC-140 | P2 | `src/xrpl/parse.rs` | Non-numeric quality rows render `price = "-"` without breaking numeric rows |
| TC-141 | P2 | `src/xrpl/format.rs` | `xrp_to_drops` max fractional boundary → u64::MAX; overflow → Err |
| TC-142 | P2 | `src/xrpl/toml.rs` | Matching validator returns attestation (case-insensitive) |
| TC-143 | P2 | `src/xrpl/toml.rs` | Invalid TOML → Err; missing VALIDATORS → count 0 |
| TC-144 | P2 | `src/xrpl/poll.rs` | `action_from_account_tx_result` follows append flag (page + not-found paths) |
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
| TC-160 | P2 | `src/components/panels/account.rs`, `src/components/panels/book.rs` | Account/Book panels render loading, empty, and populated states |
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
| TC-171 | P3 | `src/components/shared/splash.rs` | Splash helpers — `trailing_dots` cycle, `splash_art_column` centering/width edges, `splash_ascii_lines_for_height` fits/tight |



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
| TC-137–141 | — | §5 read APIs | §5 Table / TX row |
| TC-142–143 | — | §3 public-network politeness | — |
| TC-144–145 | — | §3 poll loop | — |
| TC-146–147 | NFR (SSRF) | §3 SSRF fetch guards | — |
| TC-148–149 | — | §3 public-network politeness | — |
| TC-150 | — | §1 self-uninstall | — |
| TC-151–152 | — | §5.4 ledger objects, §5.2 tx detail | §4 Assets / tx detail |
| TC-153–165 | — | §5.1 Assets panels, §5.2 tx history, §4 CLI | §4 Assets tx history |
| TC-166–171 | — | §2 address guards, §3 dUNL | §5 (splash) |

## Risk coverage

| Risk | Covered by | Evidence |
|------|------------|----------|
| R-006 mainnet write guard bypass | TC-088 `mainnet_write_guard_blocks` | `src/xrpl/poll.rs` |
| R-007 config merge precedence | TC-033/092 merge fixtures, TC-035/036 | `src/config.rs` |
| R-001 seed priority chain | TC-045/046/049 `credential_from_secrets` + env override | `src/signing.rs`, `src/config.rs` |

Remaining gaps (unimplemented by design — low priority):

- **R-002** submit channel-close observability: `send_action` は warn ログのみ。永続化は未実装。
- **R-003** `ArcValue` 共有 JSON 不変性: 規約のみ。違反時は視覚破損で検知。
