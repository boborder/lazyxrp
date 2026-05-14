# security.md

Generated: 2026-05-01 | Scope: Full codebase (`src/`, `install.sh`, `docs/`)

## 未対応・確認事項

### S-005: `install.sh` で `NO_VERIFY=1` による検証スキップ

- ユーザーが意図せず checksum 検証を無効化したままバイナリをインストールできる。
- 現状の設計はドキュメント化されており許容範囲だが、デフォルトの警告をより明確にすることを推奨。

### S-009: `--self-uninstall`（ユーザー主導でバイナリと設定データを削除）

- `std::env::current_exe()` のファイルと `.bak` を削除し、`Config` で解決した config/data ディレクトリを `remove_dir_all` する。
- 既定は一覧表示のあと標準入力で `yes` を要求。`--yes` で確認省略。
- **Cargo が保持するインストールメタデータは削除しない**。`cargo install` 済み環境では、必要ならユーザーが **`cargo uninstall lazyxrp`** を別途実行する。

## 対応済み（履歴）

| ID | 内容 | 修正 |
| --- | --- | --- |
| S-001 | `RawSigningConfig` の `Debug` derive がシードを平文でログ出力 | `impl fmt::Debug` で `[REDACTED]` にマスク |
| S-002 | `SigningConfig::load()` が環境変数を読み取り後に削除しない | `unsafe { env::remove_var(SEED_ENV) }` で除去 |
| S-003 | 設定ファイルにシードが保存される場合のファイル権限チェックなし | Unix でグループ/ワールド読取時に `tracing::warn!` |
| S-004 | env var 経由のパスに対するパス検証なし | `validated_path` で `..` を拒否。`canonicalize()` + ホーム外拒否は Phase 3 前に推奨 |
| S-006 | `tui.rs` の `Drop` 実装で `unwrap()` を使用 | `if let Err(e) = self.exit() { eprintln!(...) }` |
| S-007 | 組み込み設定のパース | ビルド時 `expect` で開発時検知 |
| S-008 | tracing ログのデフォルトレベルが INFO | S-001 対処後に問題なし |
