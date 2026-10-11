# Roadmap — lazyxrp

**Role:** 進捗管理（最終ゴール・優先順位・マイルストーンの SSOT）。注意点・進路変更記録 → [`docs/roadmap-log.md`](docs/roadmap-log.md)。

**Planning view**. Requirements → [`requirements.md`](docs/requirements.md) · System behavior → [`architecture.md`](docs/architecture.md) · UI/UX → [`DESIGN.md`](DESIGN.md).

**Current version:** `Cargo.toml` → 0.6.0（2026-10-11 公開: GitHub Release + crates.io）
**Last updated:** 2026-10-11

**Priority:** P0 = リリースブロッカー · P1 = 次マイルストーンの主目標 · P2 = 計画済み · P3 = 余力・需要次第。各 horizon 内は上から着手。

## Final goal (最終ゴール)

XRPL エコシステム（開発者・ユーザー双方）で人気の CLI/TUI ツールになること。**セキュリティの維持は非交渉条件** — guards を弱める機能は出さない（[`docs/security.md`](docs/security.md)）。

## Long-term (長期)

方向性: XRPL / Xahau / Flare の multichain 深掘りと普及（install path・onboarding）。着手は中期消化後に査定。

| Item | Priority | 前提 / Notes |
|------|----------|--------------|
| ~~TX surface completion — F-01 AccountDelete~~ **済**（0.5.0、2026-10-11） | — | composer `x`（`DELETE` 入力の確認必須）+ `-x account-delete`。Fee（owner reserve）は simulate 由来。TC-232–233、testnet で TUI（X-address tag 付き）/ CLI を validated |
| ~~Watch / Operate UI 分離~~ **済**（0.6.0、2026-10-11） | — | ユーザー選択: `--watch` フラグ。全鍵を起動時に破棄しアドレスだけ表示、TUI は Account パネル（composer・鍵生成なし）、`-x` 署名系は拒否。TC-234–235 |
| Action エラー型移行（`String` → 型付き） | Later | `docs/security.md` 構造課題。UI 文言の SSOT 化と同時に |
| Global row cache (A1) — render perf at scale | Later | 計測で問題が出たら |
| パネル内キー（`j`/`k`・`t`・composer）の設定化 | Later | グローバルキーは設定可（DESIGN §6）。需要が出たら中期へ |

## Mid-term (中期) — **0.4.0: write 基盤整理 + TX 拡張**

依存順: `poll.rs` 分割 → EscrowCreate / AMM（submit 種別追加は分割後）。

| # | Item | Source | Priority | 完了条件 |
|---|------|--------|----------|----------|
| 1 | ~~`poll.rs` 分割~~ **済**（2026-10-11） | `docs/security.md` 構造課題 | P1 | 書き込み経路（`submit_*`・`simulate_then_finalize`・quote・関連テスト）を `xrpl/submit.rs`（1,526 行）へ移動、`poll.rs` は 2,633 → 1,120 行。挙動不変（260 テスト緑、新規 TC なし） |
| 2 | ~~Sequence / LastLedgerSequence 整合検査~~ **済**（0.3.1、監査 run-2） | `docs/security.md` Fee cap 節の残余 | P2 | TC-204 |
| 3 | ~~B-02 EscrowCreate composer UI~~ **済**（2026-10-11、F-03 Finish/Cancel を前倒し） | `docs/problems.md` P-003 | P2 | 実際は signing も未実装（poll が黙って捨てていた）。signing + `submit_escrow_*` + composer `9` Create（相対時間 `90s`〜`7d`、CancelAfter 任意）・`0` 一覧から Finish/Cancel、TUI のみ。TC-215–221、testnet で Create ×2 / Cancel / Finish を validated |
| 4 | ~~F-04 AMM Deposit/Withdraw TX~~ **済**（2026-10-11） | DEX trading の後続 | P2 | two-asset deposit・withdraw 全額 / LP 量。composer `d` / `w` + `-x amm-deposit` / `-x amm-withdraw`。TC-222–226、testnet で CLI ×2 / TUI ×2 を validated（プールは使い捨てスクリプトで作成、AMMCreate は対象外） |
| 5 | ~~Fake WebSocket integration fixtures~~ **済**（2026-10-11） | `docs/test.md` | P2 | TC-227。fixture が実バグを検出: `xrpl_receive` が stream メッセージの `type` を落とし、ledger close / account tx の WS イベントが一度も届いていなかった（interval poll のみで動作）。生フレームを読むよう修正、testnet で ledger close 受信を確認。panel 側の反映は既存 TC（account.rs）が担保 |
| 6 | ~~二重送信ガード（同一内容の連続 submit）~~ **済**（2026-10-11） | `docs/security.md` 未修正リスト | P3 | TC-228。WalletPanel の出口 1 か所で、同一 params の write を結果（Ok/Err）到着まで拒否。CLI は validated 待ちのため対象外 |
| 7 | ~~`account_offers` marker paging（> 200 件）~~ **済**（2026-10-11） | `client.rs` ponytail 注記 | P3 | TC-229。200 件/page で marker を追う（上限 50 page）。TUI OfferCancel と `-x offers` の両方 |
| 8 | ~~C3 execute smoke (`execute=true`)~~ **済**（2026-10-11、testnet） | — | P3 | ユーザー判断で実資金は testnet（XRPL testnet + Coston2）。検証で C2 の simulate 拒否（memo null、TC-230）と C3 の ABI 不一致（存在しない selector、TC-231）を発見・修正し、C2 → 公開 executor による mint、48-byte memo → lazyxrp C3 で FXRP 9.8 → 14.7 を確認。mainnet は未実施 |

## Next (次期)

中期 #1–#8 はすべて済み（0.4.0–0.4.2）。長期は F-01 AccountDelete（0.5.0）と Watch / Operate 分離（0.6.0）が済み。残り（Action エラー型・row cache・パネル内キー設定化）は完了条件が未定義のため着手前に査定。

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
| Live 検証 + testnet DX + 監査 run-2 | 0.3.1 | **Done** (2026-10-11) | `-x faucet` / `-x offer-create`、CLI write の validated 待ち、endpoint `network_id` 照合（S-017）、opt-in testnet E2E、監査 run-2 修正（TC-204–214、Core Vault ピンは破壊的）、OSS パッケージ整備、dependabot + Actions SHA 固定 |
| write 基盤整理 + TX 拡張 | 0.4.0 | **Done** (2026-10-11) | `poll.rs` → `submit.rs` 分割、EscrowCreate / Finish / Cancel（TUI）、AMM Deposit / Withdraw（TUI + `-x`）、`amm_info` 実応答形の parse 修正（TC-015）。testnet で Escrow ×4・AMM ×4 validated |
| 中期 #5–#7 | 0.4.1 | **Done** (2026-10-11) | WS イベント未認識の修正（TC-227）、二重送信ガード（TC-228）、`account_offers` paging（TC-229） |
| C3 testnet smoke | 0.4.2 | **Done** (2026-10-11) | FXRP C2 simulate の null 拒否修正（TC-230）、C3 ABI 修正（TC-231）。XRPL testnet + Coston2 で mint / execute を確認 |
| AccountDelete | 0.5.0 | **Done** (2026-10-11) | 長期 F-01。composer `x` + `-x account-delete`、TUI は `DELETE` 入力の確認必須（TC-232–233）。testnet で TUI / CLI validated |
| Watch / Operate 分離 | 0.6.0 | **Done** (2026-10-11) | `--watch` 閲覧専用起動（TC-234–235） |

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
