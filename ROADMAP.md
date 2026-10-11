# Roadmap — lazyxrp

**Role:** 進捗管理（最終ゴール・優先順位・マイルストーンの SSOT）。注意点・進路変更記録 → [`docs/roadmap-log.md`](docs/roadmap-log.md)。

**Planning view**. Requirements → [`requirements.md`](docs/requirements.md) · System behavior → [`architecture.md`](docs/architecture.md) · UI/UX → [`DESIGN.md`](DESIGN.md).

**Current version:** `Cargo.toml` → 0.3.0（2026-10-10 公開: GitHub Release + crates.io）
**Last updated:** 2026-10-11

**Priority:** P0 = リリースブロッカー · P1 = 次マイルストーンの主目標 · P2 = 計画済み · P3 = 余力・需要次第。各 horizon 内は上から着手。

## Final goal (最終ゴール)

XRPL エコシステム（開発者・ユーザー双方）で人気の CLI/TUI ツールになること。**セキュリティの維持は非交渉条件** — guards を弱める機能は出さない（[`docs/security.md`](docs/security.md)）。

## Long-term (長期)

方向性: XRPL / Xahau / Flare の multichain 深掘りと普及（install path・onboarding）。着手は中期消化後に査定。

| Item | Priority | 前提 / Notes |
|------|----------|--------------|
| TX surface completion — F-01 AccountDelete, F-03 EscrowFinish/Cancel | Later | 0.4.0 の `poll.rs` 分割 + EscrowCreate 後（同じ submit 追加パターン） |
| Watch / Operate UI 分離 | Later | `docs/security.md` 構造課題。署名系 UI を閲覧専用起動から外す |
| Action エラー型移行（`String` → 型付き） | Later | `docs/security.md` 構造課題。UI 文言の SSOT 化と同時に |
| Global row cache (A1) — render perf at scale | Later | 計測で問題が出たら |
| パネル内キー（`j`/`k`・`t`・composer）の設定化 | Later | グローバルキーは設定可（DESIGN §6）。需要が出たら中期へ |

## Mid-term (中期) — **0.4.0: write 基盤整理 + TX 拡張**

依存順: `poll.rs` 分割 → EscrowCreate / AMM（submit 種別追加は分割後）。

| # | Item | Source | Priority | 完了条件 |
|---|------|--------|----------|----------|
| 1 | `poll.rs` 分割（2,574 行: poll loop / read batch / submit 群） | `docs/security.md` 構造課題 | P1 | `submit_*` + `simulate_then_finalize` を `xrpl/submit.rs` へ移動、挙動不変（既存 TC 緑・新規 TC なし）。#3・#4 の前提 |
| 2 | Sequence / LastLedgerSequence 整合検査 | `docs/security.md` Fee cap 節の残余 | P2 | simulate 由来 `Sequence` ≠ `account_info` なら署名拒否。LLS は 0.3.0 から `ledger_index + 20` を自前補完（TC-200）なので、**ノード返却 LLS が `ledger_index` から過大（> +100 目安）なら拒否**を追加 + TC |
| 3 | B-02 EscrowCreate composer UI | `docs/problems.md` P-003 | P2 | #1 後。signing/poll 既存、composer + Action 配線 + TC。testnet で validated 1 件 |
| 4 | F-04 AMM Deposit/Withdraw TX | DEX trading の後続 | P2 | #1 後。composer + `-x` CLI、simulate 経路再利用、testnet で validated |
| 5 | Fake WebSocket integration fixtures | `docs/test.md` | P2 | ledger close → poll trigger → panel 更新をローカル WS で検証 |
| 6 | 二重送信ガード（同一内容の連続 submit） | `docs/security.md` 未修正リスト | P3 | 直前 submit の結果待ち中に同一 params を拒否 + TC |
| 7 | `account_offers` marker paging（> 200 件） | `client.rs` ponytail 注記 | P3 | 200 件超アカウントで全件表示 |
| 8 | Mainnet C3 execute smoke (`execute=true`) | — | P3 | **外部ブロック**: FDC verifier API key + 実資金。入手まで据え置き |

## Next (次期) — **0.3.1: live 検証 + testnet DX**（実装済み・未リリース）

0.3.0 の testnet E2E で、mock が実ノードの `simulate` 応答と違っていたため全 write の失敗を見逃した（TC-200）。faucet・OfferCreate・validated 確認には外部の xrpl.js が要った。この 2 点を塞いだ。

| # | Item | Priority | 状態 |
|---|------|----------|------|
| 1 | ~~Testnet E2E の自動化（opt-in）~~ **済** | P1 | `mise run e2e-testnet`（`tests/testnet_e2e.rs`、`#[ignore]`）: faucet ×2 → send → offer-create → offers → offer-cancel、全 write validated（約 27 秒）。`docs/maintenance.md` のリリース前手順に記載 |
| 2 | ~~`-x faucet [ADDR]`（testnet/devnet 限定）~~ **済** | P1 | ADDR 省略時はローカル生成（seed は表示のみ、faucet へは address だけ）。応答の address 不一致は Err（TC-201） |
| 3 | ~~`-x offer-create <GETS> <PAYS>`~~ **済** | P1 | spec は RPC 前に検証（TC-190/191） |
| 4 | ~~submit 後の validated 確認~~ **済**（CLI） | P2 | CLI write は標準で validated まで待機、`tes` 以外・LLS 超過で非 0 終了、`--no-wait` で従来動作（TC-202）。TUI は Tx History に最終結果が出る（E2E で確認）ため追加なし |
| 5 | ~~Endpoint network 照合（S-017）~~ **済** | P0 | E2E 中に発見: mainnet の `XRPL_RPC_SERVER` 下で `--network testnet` の write が `--yes` なしで mainnet へ届き得た。署名前に `server_info.network_id` を照合（TC-203、live 確認済み） |
| 6 | ~~OSS パッケージ整備~~ **済** | P1 | crate から `docs/` を外し `tests/`・`CHANGELOG.md`・`SECURITY.md` を同梱（`cargo package` 警告 0、249 KiB）。description / keywords / categories 更新、crate doc を docs.rs の入口に（公開 API は `run` / `run_rp` のみ）、README バッジ、MSRV 1.94.1 を実測して CI job 化、private vulnerability reporting 有効化、GitHub description / topics 設定、`mise run release` が CHANGELOG を確定 |
| 6b | ~~セキュリティ監査 run-2 対応~~ **済** | P0 | simulate の Sequence/LLS 照合、Core Vault ピン（`expected_core_vault`、破壊的）、quote の通貨照合、tx hash ローカル計算、simulate 文字列 sanitize、CLI wait の expired 競合、faucet/RPC redirect 無効化、env 秘密の起動時取得 + zeroize、spec 末尾拒否、CLI send 入力検証、keygen popup 消去、TUI に署名 Fee 表示、Actions SHA 固定（TC-204–214、testnet E2E 緑） |
| 7 | 0.3.1 リリース | P0 | `mise run e2e-testnet` 緑 → `mise run release 0.3.1` → push。README に「既定で validated 待ち」「network 照合」を記載済み |

## Short-term (短期) — 0.3.0 公開後の残務

| # | Task | Priority | 完了条件 |
|---|------|----------|----------|
| 1 | ~~Testnet E2E — TUI~~ **済**（2026-10-11、tmux で操作） | — | OfferCreate（即約定）・非約定 OfferCreate → OfferCancel・`p` 見積り → cross-currency Payment（1 USD 着金）を validated。hash は `docs/test.md` Testnet E2E |
| 2 | ~~リリース運用の最適化~~ **済**（2026-10-11） | — | Release 本文を自動生成（前タグからのコミット + GitHub 生成ノート）、`mise run release <ver>`（bump + lock 同期 + gates + 1 コミット）、週次 CI（advisory / yanked）。手順は `docs/maintenance.md` |
| 3 | ~~v0.3.0 Release 本文の後埋め~~ **済**（2026-10-11） | — | 破壊的変更・移行リンク・コミット一覧を記入 |

## Shipped

完了マイルストーン（履歴）。次の着手先は上の horizon 参照。

| Milestone | Version | Status | Notes |
|-----------|---------|--------|-------|
| Quality pass | 0.1.23 | **Done** | |
| Agent/docs hygiene | — | **Done** | |
| Wallet TX + FXRP | 0.2.0+ | **Done** (2026-09-12) | C3 mainnet smoke → Mid-term (P2) |
| Docs / agent hygiene | — | **Done** | Human doc index in README; `docs/` self-contained |
| Ratatui optimization | — | **Done** | dirty render + shared table shipped |
| Docs hierarchy redesign | — | **Done** (2026-09-12) | keys → `DESIGN.md`; architecture = behavior; removed `docs/README` + `docs/tasks` |
| Network multichain | 0.2.5+ | **Done** (2026-09-12) | Xahau + `<Ctrl-n>` session switch + Flare display + wallet read panel |
| Test coverage / local integration pass | 0.2.9 | **Done** (2026-09-16) | Local RPC read-only CLI coverage, `lazyxrp`/`rp` binary smoke tests, Assets and panel state coverage |
| Review batch #3: production guard + test coverage | 0.2.10 | **Done** (2026-09-17) | Xahau production write guard, local CLI RPC migration, nft_image SSRF hardening, binary smoke tests, alloy 2 / MSRV 1.94.1 |
| Flare read-path stabilization | 0.2.11 | **Done** (2026-09-17) | Batched FTSOv2 `getFeedsById` (5→1 RPC/poll), fee-aware retry, `FXRP/USD`→`XRP/USD` premap, AssetManager/wallet reads via `try_join!`, flare-only failure cooldown (2→60s) decoupled from XRPL backoff |
| XRPL poll efficiency pass | 0.2.11 | **Done** (2026-09-17) | dUNL excluded from XRPL backoff both ways + stale-on-error with 60 s retry cooldown; Account-tab panels (`account_tx`, wallet overview) gated like Market/Assets |
| Status bar health tracks | 0.2.11 | **Done** (2026-09-17) | XRPL chip goes STALE during quiet backoff + OFFLINE on `server_info` errors; flare dot (`flr:`) with red-on-failure/orange-stale; `dunl:` data-age marker with `!` past cache TTL |
| DEX trading + CLI redesign + keymap docs | 0.3.0 | **Done** (2026-10-10) | Wallet composer OfferCancel (`8`) + Payment Send max / `p` path quote (`slippage_bps`); CLI: bare subcommands + `watch` removed, `-x send` simulate-first + non-zero exit on failure, `-x offers` / `-x offer-cancel`; shared Payment validation (P-004); live `simulate` LLS fill (TC-200); `--seed`/`--mnemonic` kept with stderr warning (FR-13); global keybindings user-mergeable (DESIGN §6); `rc` alias stays README-only |

## Task tracking

| Concern | SSOT |
|---------|------|
| Goal hierarchy + priorities + shipped history | This file |
| Planning cautions + pivot log | `docs/roadmap-log.md` |
| This week's implementation checkboxes | `openspec/changes/<name>/tasks.md` (when using OpenSpec) |
| Completion history | `openspec/changes/archive/**` + git |
| Human doc index | Root `README.md` § Documentation |
| Agent contracts | `openspec/specs/` + `openspec/config.yaml` |

## How to add work

1. Horizon（短期 / 中期 / 長期）を選んでこのファイルに row 追加（優先度も記録）
2. New requirement → `docs/requirements.md` (FR/NFR)
3. Shipped behavior / network / config rules → `docs/architecture.md` (+ TC in `docs/test.md`)
4. Keys, layout, colors, modals → root [`DESIGN.md`](DESIGN.md)
5. Active implementation checkboxes (when using OpenSpec) → `openspec/changes/<name>/tasks.md`
6. 進路変更・計画上の注意点 → [`docs/roadmap-log.md`](docs/roadmap-log.md)
