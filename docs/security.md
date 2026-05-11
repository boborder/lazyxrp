# security.md

Generated: 2026-05-01 | Fixed: 2026-05-01  
Scope: Full codebase (`src/`, `install.sh`, `docs/`)  
Language: Rust (Edition 2024)  
Frameworks: tokio, ratatui, clap, xrpl-rust, secrecy

---

## Executive Summary

lazyxrp は **監視・読み取り中心**で、書き込み系（Payment 送信など）は Phase 3 として段階的に追加中。  
シード管理基盤があるため、その取り扱いが主なリスク面となる。  
致命的な脆弱性は存在しないが、**HIGH x1（設定層 `Debug` 経由のシード平文ログ、S-001）** を Phase 3 本格運用前に対処すること。環境変数経由の露出は `SigningConfig::load` で除去済み（S-002）。

---

## HIGH

### S-001: `RawSigningConfig` の `Debug` derive がシードを平文でログ出力する

**ファイル:** `src/config.rs` line 39–44  
**影響:** シードが `Debug` フォーマット経由でログや標準エラーに平文で出力される。

```rust
// src/config.rs:39
#[derive(Clone, Debug, Default, Deserialize)]   // ← Debug が問題
pub struct RawSigningConfig {
    pub seed: Option<String>,                   // ← 平文シード
}
```

`Config` → `LedgerConfig` → `RawSigningConfig` のすべてが `Debug` を持つため、  
`tracing::debug!("{:?}", config)` / `dbg!(config)` / `{:?}` フォーマットのいずれでも  
シードが平文でログファイルや stderr に出力される。

**修正方針:**  
`RawSigningConfig` の `Debug` を手動実装してシードをマスクする。

```rust
// 修正後
impl fmt::Debug for RawSigningConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawSigningConfig")
            .field("seed", &self.seed.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}
```

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

### S-003: 設定ファイルにシードが保存される場合のファイル権限チェックなし

**ファイル:** `src/config.rs` line 103–130  
**影響:** `~/.config/lazyxrp/config.toml` のパーミッションが `0644` (全ユーザー読み取り可) の場合、同一システムの他ユーザーがシードを読める。

現在、設定ファイルを読み込む際にファイルのパーミッションを検証していない。

**修正方針:**  
`RawSigningConfig::seed` が `Some(_)` の場合、設定ファイルが `0600` 以外なら警告を出す。

```rust
#[cfg(unix)]
fn warn_if_config_world_readable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = path.metadata() {
        let mode = meta.permissions().mode();
        if mode & 0o044 != 0 {
            tracing::warn!(
                "Config file {} is world/group readable (mode {:04o}). \
                 Consider: chmod 600 {}",
                path.display(), mode & 0o777, path.display()
            );
        }
    }
}
```

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

**ファイル:** `install.sh` line 10, 109  
**影響:** ユーザーが意図せず checksum 検証を無効化したままバイナリをインストールできる。

現状の設計はドキュメント化されており許容範囲だが、デフォルトの警告をより明確にすることを推奨。

---

## LOW

### S-006: `tui.rs` の `Drop` 実装で `unwrap()` を使用

**ファイル:** `src/tui.rs` line 232  
**影響:** `Drop` 中のパニックはプロセスをアボートする。セキュリティ上の直接的な影響は低いが、ターミナル状態が復元されないリスクがある。

```rust
// src/tui.rs:232
fn drop(&mut self) {
    self.exit().unwrap();  // ← Drop でのパニックは危険
}
```

**修正方針:** `let _ = self.exit();` か `if let Err(e) = self.exit() { eprintln!("tui exit error: {e}"); }` に変更。

---

### S-007: 組み込み設定の `unwrap()` がパニックを引き起こす可能性

**ファイル:** `src/config.rs` line 104  
**影響:** バイナリに埋め込まれた `.config/config.json5` のパースが失敗した場合、起動時パニックで即時クラッシュ。

```rust
// src/config.rs:104
let default_config: Config = json5::from_str(CONFIG).unwrap();
```

埋め込み値はコンパイル時に確定しているため実運用リスクは低いが、CI で検証を追加することを推奨。

---

### S-008: tracing ログのデフォルトレベルが INFO（問題なし、確認事項として記載）

**ファイル:** `src/logging.rs` line 16  
デフォルトが `INFO` であるため `DEBUG` ログは `RUST_LOG=debug` が必要。  
現状は問題ないが、将来 `debug!("{:?}", config)` のようなログが追加された場合、  
`RUST_LOG=debug` 実行時にシード（S-001 対処前）が漏洩する。S-001 対処後は問題なし。

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
