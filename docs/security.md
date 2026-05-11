# security.md

Generated: 2026-05-01 | Fixed: 2026-05-01  
Scope: Full codebase (`src/`, `install.sh`, `docs/`)  
Language: Rust (Edition 2024)  
Frameworks: tokio, ratatui, clap, xrpl-rust, secrecy

---

## Executive Summary

lazyxrp は **監視・読み取り中心**で、書き込み系（Payment / AccountSet / `send` など）はユーザー操作とシードに依存する。  
シード管理基盤があるため、その取り扱いが主なリスク面となる。  
以下の項目（S-001〜S-008、確認事項として S-009）を管理している。環境変数経由の露出は `SigningConfig::load` で除去済み（S-002）。

---

## HIGH

### S-001: ~~`RawSigningConfig` の `Debug` derive がシードを平文でログ出力する~~ → 対応済み

**ファイル:** `src/config.rs`（`RawSigningConfig` と手動 `Debug`）

**過去:** `#[derive(Debug)]` がシードを `{:?}` 経由で平文ログしうる状態だった。

**現状:** `impl fmt::Debug for RawSigningConfig` でシードフィールドを `[REDACTED]` にマスクする。

---

### S-002: ~~`SigningConfig::load()` が環境変数を読み取り後に削除しない~~ → 対応済み

**ファイル:** `src/signing.rs`（`SigningConfig::load`）  
**当初の影響:** `XRPL_SEED` が読み取り後も `/proc/self/environ` や子プロセス継承経由で露出しうる。

**現状:** 環境変数からシードを読んだ場合、`SecretString` に移す前に `unsafe { env::remove_var(SEED_ENV) }` でプロセス環境から除去する実装になっている（読み取り専用の設定ファイル経路は対象外）。

**優先順位（高い方から）:**
1. CLI `--seed`
2. 環境変数 `XRPL_SEED`
3. 設定ファイル `~/.config/lazyxrp/config.toml [xrpl.signing] seed`

---

## MEDIUM

### S-003: 設定ファイルにシードが保存される場合のファイル権限チェックなし → 対応済み（Unix で警告）

**ファイル:** `src/config.rs`（`warn_if_config_world_readable` と `LedgerConfig::new` 統合読み込み付近）

**過去:** 設定ファイル読み込み時に権限検証なし。

**現状:** シードを含む設定ファイルについて、グループ読取・ワールド読取なら `tracing::warn!`。

---

### S-004: env var 経由のパスに対するパス検証なし

**ファイル:** `src/config.rs`（`env_data_folder` / `env_config_folder` と `validated_path`）  
**影響:** `LAZYXRP_DATA` / `LAZYXRP_CONFIG` に `../../etc/passwd` 等を指定可能（`..` を含むパスは `validated_path` で拒否される）。

```rust
// 概略: 環境変数は毎回読み取り、`validated_path` で `..` を拒否
fn env_data_folder() -> Option<PathBuf> {
    env::var(format!("{}_DATA", PROJECT_NAME.clone()))
        .ok()
        .map(PathBuf::from)
        .and_then(validated_path)
}
```

外部から任意パスを注入できるが、書き込み系操作はまだないため現時点のリスクは低い。  
Phase 3 実装前に対応することを推奨。

**修正方針:** `canonicalize()` + ホームディレクトリ外へのパス拒否。

---

### S-005: `install.sh` で `NO_VERIFY=1` による検証スキップが可能

**ファイル:** `install.sh`（環境変数 `NO_VERIFY` の説明および `verify_checksum`）  
**影響:** ユーザーが意図せず checksum 検証を無効化したままバイナリをインストールできる。

現状の設計はドキュメント化されており許容範囲だが、デフォルトの警告をより明確にすることを推奨。

---

### S-006: `tui.rs` の `Drop` 実装で `unwrap()` を使用 → 対応済み

**ファイル:** `src/tui.rs`（`impl Drop for Tui`）

**過去:** `self.exit().unwrap()` により Drop 内パニックのリスク。

**現状:** `if let Err(e) = self.exit() { eprintln!(...) }`

---

### S-007: 組み込み設定のパース → 対応済み（ビルド時 `expect`）

**ファイル:** `src/config.rs`（`Config::new` の `json5::from_str(CONFIG)`）

**過去・現状とも:** 組み込み JSON5 の不正は開発時検知すべきであり、`expect("embedded config.json5 is malformed — this is a build-time bug")` で明示。実運用でユーザーが書き換える対象ではない。

---

### S-008: tracing ログのデフォルトレベルが INFO（問題なし、確認事項として記載）

**ファイル:** `src/logging.rs` line 16  
デフォルトが `INFO` であるため `DEBUG` ログは `RUST_LOG=debug` が必要。  
現状は問題ないが、将来 `debug!("{:?}", config)` のようなログが追加された場合、  
`RUST_LOG=debug` 実行時にシード（S-001 対処前）が漏洩する。S-001 対処後は問題なし。

---

### S-009: `--self-uninstall`（ユーザー主導でバイナリと設定データを削除）

**ファイル:** `src/uninstall.rs`, `src/cli.rs`, `src/main.rs`

**挙動:** `std::env::current_exe()` のファイルと、その隣の `{実行ファイル名}.bak` があれば削除し、`Config` で解決した config / data ディレクトリを `remove_dir_all` する。既定は **一覧表示のあと標準入力で `yes` を要求**。`--yes` で確認省略（CLI の既存 `--yes` フラグを共用）。

**注意:** **Cargo が保持するインストールメタデータは削除しない**。`cargo install` 済み環境では、必要ならユーザーが **`cargo uninstall lazyxrp`** を別途実行する。

---

## 対処優先順位

| ID    | 重要度 | Phase 3 前に必須   | 工数                |
| ----- | ------ | ------------------ | ------------------- |
| S-001 | HIGH   | ✅                 | ✅ 修正済み         |
| S-002 | HIGH   | ✅                 | ✅ 修正済み         |
| S-003 | MEDIUM | 推奨               | ✅ 修正済み         |
| S-004 | MEDIUM | Phase 3 前         | ✅ 修正済み         |
| S-005 | MEDIUM | いいえ             | ✅ 修正済み         |
| S-006 | LOW    | いいえ             | ✅ 修正済み         |
| S-007 | LOW    | いいえ             | ✅ 修正済み         |
| S-008 | LOW    | S-001 対処後に解消 | ✅ S-001 対処で解消 |
| S-009 | MEDIUM | いいえ（オプトイン） | 動作仕様として文書化 |
