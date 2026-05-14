# tasks.md

## 1. ステータス定義

- `IN_PROGRESS`: 実装中または安定化作業中
- `TODO`: 未着手
- `BLOCKED`: 外部要因で停止中

## 2. 現在のタスク一覧

現在、アクティブな未完了タスクはありません。主要機能（Phase 1 読み取り拡張、Phase 2 ネットワーク抽象化）は実装完了済み。

## 3. 直近マイルストーン

### M3: 公開向け整備

- 目標:
  - README と AGENTS の初版完成
  - 主要コマンドの利用手順が再現可能
  - 配布物と設定ファイル運用（XDG準拠）が利用者に明確
- 完了条件:
  - 新規利用者が `cargo run -- watch --account <address>` まで実行できる

### Phase 3: 書き込み系 TX

- 目標: シードを用いた署名・送信（Payment / AccountSet）の本番実装
- 完了条件:
  - `cargo test` が全ケースで通る
  - testnet で書き込み系 CLI/TUI が正常動作すること

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
| T-031〜T-034 | コンポーネント再編、WalletPanel、5タブ統合、`account_objects` | TUI 再構成 |
