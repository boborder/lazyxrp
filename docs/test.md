# Test Strategy & Registry

**Role:** テスト設計・テストガイド（重要 TC カタログと FR/architecture へのトレース）。

> Last Updated: 2026-09-12 (TC-111–122: Xahau / Flare multichain)
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
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps --document-private-items --all-features
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
| TC-041 | P1 | `src/network.rs` | Network::is_mainnet — correct boolean |
| TC-042 | P0 | `src/lib.rs` | resolve_network — CLI flag overrides env |
| TC-043 | P0 | `src/lib.rs` | resolve_rpc_url — mainnet default when no CLI/env/config override |
| TC-044 | P1 | `src/lib.rs` | resolve_ws_url — CLI --ws-server is top priority |
| TC-045 | P1 | `src/signing.rs` | credential_from_secrets — no source returns none |
| TC-046 | P1 | `src/signing.rs` | credential_from_secrets — family seed wraps SigningCredential |
| TC-047 | P1 | `src/signing.rs` | prompt_mainnet_confirmation — testnet/devnet skips |
| TC-048 | P2 | `src/signing.rs` | prompt_mainnet_confirmation — --yes skips on mainnet |
| TC-049 | P1 | `src/config.rs` | Config::new — env XRPL_SEED overrides file seed |
| TC-050 | P0 | `src/xrpl/cli_exec.rs` | CLI info — exits with code 0 |
| TC-051 | P1 | `src/xrpl/cli_exec.rs` | CLI account — exits with code 0 |
| TC-052 | P1 | `src/xrpl/cli_exec.rs` | CLI book — exits with code 0 |
| TC-053 | P1 | `src/xrpl/cli_exec.rs` | CLI summary — exits with code 0 |
| TC-054 | P1 | `src/xrpl/cli_exec.rs` | CLI nfts — exits with code 0 |
| TC-055 | P1 | `src/xrpl/cli_exec.rs` | CLI lines — exits with code 0 |
| TC-056 | P2 | `src/xrpl/cli_exec.rs` | CLI amm — exits with code 0 |
| TC-057 | P2 | `src/xrpl/cli_exec.rs` | CLI tx-history — exits with code 0 |
| TC-058 | P1 | `src/cli.rs` | CLI — invalid parameters error |
| TC-059 | P2 | `src/xrpl/cli_exec.rs` | CLI — invalid r-address format |
| TC-060 | P1 | `src/app.rs` | Watch mode — App builds 4 tabs with watch account (I-9) |
| TC-061 | P1 | `src/app.rs` | Watch mode — Quit action stops background tasks |
| TC-062 | P2 | `src/app.rs` | Watch mode — RefreshAccount sends PollCommand |
| TC-063 | P2 | `src/app.rs` | Watch mode — RefreshBook sends PollCommand |
| TC-064 | P2 | `src/app.rs` | Watch mode — TabNext/TabPrev wrap around panels |
| TC-065 | P3 | `src/app.rs` | Watch mode — HelpOverlay toggles on `?` and Esc |
| TC-066 | P1 | `src/xrpl/cli_exec.rs` | CLI — `account-status` against live RPC |
| TC-067 | P2 | `src/xrpl/cli_exec.rs` | CLI — `send` simulation with seed |
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

## Traceability

| TC | FR / NFR | Architecture § | Design |
|----|----------|------------------|--------|
| TC-111–112 | FR-12 | §6.1 presets | `Ctrl-n` global keymap |
| TC-113–114 | FR-15 | §6.7 Flare display | Flare display layouts |
| TC-115 | FR-12 | §6.1 session switch, §6.5 badge | `Ctrl-n`, status bar |
| TC-116–119 | FR-15 | §6.7 | Flare display layouts |
| TC-120–122 | FR-16 | §6.8 Flare wallet panel | Overview wallet panel |

## Risk coverage

| Risk | Covered by | Evidence |
|------|------------|----------|
| R-006 mainnet write guard bypass | TC-088 `mainnet_write_guard_blocks` | `src/xrpl/poll.rs` |
| R-007 config merge precedence | TC-033/092 merge fixtures, TC-035/036 | `src/config.rs` |
| R-001 seed priority chain | TC-045/046/049 `credential_from_secrets` + env override | `src/signing.rs`, `src/config.rs` |

Remaining gaps (unimplemented by design — low priority):

- **R-002** submit channel-close observability: `send_action` は warn ログのみ。永続化は未実装。
- **R-003** `ArcValue` 共有 JSON 不変性: 規約のみ。違反時は視覚破損で検知。
