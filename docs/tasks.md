# tasks.md

## 1. ステータス定義

- `IN_PROGRESS`: 実装中または安定化作業中
- `TODO`: 未着手
- `BLOCKED`: 外部要因で停止中

## 2. 現在のタスク一覧

### 安定化（優先）

| ID | タスク | ステータス | 備考 |
| --- | --- | --- | --- |
| S-01 | `unwrap`/`expect` 監査 | ✅ DONE | 非テストコードに残存なし |
| S-02 | `docs/` 同期 | ✅ DONE | simulate フロー・新 TX 種別反映済 |
| S-03 | `ripple_path_find` UI 接続 | ✅ DONE | Market タブ `PathFindPanel`（送信額・ホップ・ルート、安い順） |
| S-04 | キー生成機能 | BLOCKED | SetRegularKey UI 有効化の前提 |

### バックエンド準備（UI 未着）

| ID | タスク | ステータス | 備考 |
| --- | --- | --- | --- |
| B-01 | SetRegularKey submit | ✅ DONE | signing + poll 完備、UI はキー生成待ち |
| B-02 | EscrowCreate submit | ✅ DONE | signing + poll 完備、UI 未実装 |
| B-03 | OfferCreate submit | ✅ DONE | signing + poll + `offer_spec_to_json_value` 完備 |

### 将来 TX 種別（バックエンドのみ計画）

| ID | タスク | ステータス |
| --- | --- | --- |
| F-01 | AccountDelete | TODO |
| F-02 | OfferCancel | TODO |
| F-03 | EscrowFinish/Cancel | TODO |
| F-04 | AMM Deposit/Withdraw | TODO |

## 3. 直近マイルストーン

### M3: 公開向け整備（完了）

- 達成：README と AGENTS の初版完成、主要コマンドの利用手順が再現可能、XDG 準拠の設定ファイル運用

### Phase 3: 書き込み系 TX（UI 部分完了）

- 達成：
  - `simulate` ベースの安全な送信フロー（`simulate_tx`→sign→`submit`）
  - Wallet UI: Payment（XRP + IOU）、AccountSet
  - バックエンド: SetRegularKey、EscrowCreate、OfferCreate
- 残：キー生成、EscrowCreate/OfferCreate UI

## 4. 完了タスク（履歴）

| ID | タスク | 備考 |
| --- | --- | --- |
| T-001〜T-007 | watch モード、XRPL ポーリング、WS サブスクライブ、CLI コマンド、ビルド安定化、スプラッシュ画面 | Phase 0 基盤 |
| T-008〜T-011 | 単体テスト、結合テスト、CI/CD、ドキュメント整理 | 品質基盤 |
| T-012 | バイナリ配布（CD ワークフロー + GitHub Releases） | `.github/workflows/cd.yml` |
| T-013 | ツール命名統一 | `lazyxrp` に統一 |
| T-014 | XDG Base Directory 準拠 | `config.toml` 読み込み |
| T-015〜T-019 | ASCII アート、アニメーション、再接続バックオフ、リフレッシュデバウンス、ヘルプオーバーレイ | TUI 改善 |
| T-020〜T-023 | Network 列挙型、`--network` フラグ、`XrplConfig` 拡張、StatusBar インジケータ | Phase 2 基盤 |
| T-024〜T-027 | NFT / TrustLine / AMM / Tx 履歴 の RPC + CLI | Phase 1 読み取り拡張 |
| T-028〜T-029 | `SigningConfig` 導入、mainnet 書き込み確認プロンプト | Phase 3 準備 |
| T-030 | `docs/*.md` 重複・参照導線整備 | 本整理の前身 |
| T-031〜T-034 | コンポーネント再編、WalletPanel、4タブ統合、`account_objects` | TUI 再構成 |
| T-035 | `AccountSummary` 拡張（Flags, RegularKey, Domain hex） | ウォレット表示強化 |
| T-036 | Payment IOU 対応 + Wallet UI（`i` トグル） | simulate フロー |
| T-037 | `ripple_path_find` RPC API（client + types + tests） | パスファインディング準備 |
| T-040 | `ripple_path_find` TUI（`PathFindPanel`） | Market タブ・安い順ソート・Enter で JSON |
| T-038 | OfferCreate submit 配線（backend 完備） | signing + poll |
| T-039 | TicketCreate 削除 + TrustSet 削除 | ユーザー判断 |
