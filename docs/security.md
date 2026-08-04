# security.md

Generated: 2026-05-01 | Scope: Full codebase (`src/`, `install.sh`, `docs/`)

**SSOT split:** This file tracks **security review findings (S-xxx)** — what was audited and fixed. **Implementation risks (R-xxx)** and suggested tests live in [`agent/RISK_REGISTER.md`](agent/RISK_REGISTER.md). Invariants (I-1〜I-11) are in [`agent/INVARIANTS.md`](agent/INVARIANTS.md).

## S-xxx ↔ R-xxx cross-reference

| S-ID | Topic | Related R-ID | Notes |
|------|-------|--------------|-------|
| S-001 | Seed in `Debug` output | R-001 | Plaintext logging; use `secret_seed` only after `Config::new()` |
| S-002 | `XRPL_SEED` not cleared from env | R-001, I-4 | `prime_seed_source` + `env_lock` |
| S-003 | Config file seed permissions | R-001 | Unix warn on group/world read |
| S-010 | Plaintext `seed` retained on `Config` | R-001, I-1 | Cleared in `Config::new()` |
| S-011 | `--seed` visible in `ps` | R-001 | Prefer env/file; README warning |
| S-006 | `Tui::drop` panic / raw mode | R-005, I-8 | `eprintln!` on `exit()` failure |
| S-009 | `--self-uninstall` data deletion | — | User-driven; not an R entry |
| S-004, S-005, S-007, S-008 | Paths, install verify, build-time config, logging | R-007 (merge), — | See table below |

Risks without a matching S entry (e.g. R-002 submit errors swallowed, R-006 mainnet guard bypass, R-008 RPC 429) are tracked only in [`agent/RISK_REGISTER.md`](agent/RISK_REGISTER.md) and [`test.md`](test.md) / [`agent/RISK_TO_TESTS.md`](agent/RISK_TO_TESTS.md).

## 未対応・確認事項

### RUSTSEC-2026-0235: rkyv 0.7.46（未コンパイルの optional 依存）

- `Cargo.lock` に `rkyv 0.7.46` が残るのは `rust_decimal` の optional `rkyv` feature 経由。`xrpl-rust` → `rust_decimal` はこの feature を有効化しないため、rkyv はビルド・実行されない（`cargo tree -i rkyv` = 空）。
- 0.7.x に修正版なし。upstream fix は rkyv 0.8 移行（rust_decimal 未対応）。
- CI では `cargo audit --ignore RUSTSEC-2026-0235`（`ci.yml`）と `deny.toml` の `ignore` で記録済み。rust_decimal が rkyv 0.8 対応 or feature 削除したら ignore を外す。

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
| S-005 | `install.sh` で `NO_VERIFY=1` の警告が不十分 | `usage()` / ヘッダに MITM リスク警告を追加 |
| S-010 | `Config` が平文 `seed` を `Arc<Config>` として継続保持 | `RawSigningConfig` に `secret_seed: Option<SecretString>` を追加し、`Config::new()` / `main.rs` / `app.rs` で平文を即座にクリア |
| S-011 | CLI `--seed` がプロセス引数に残り `ps` から閲覧可能 | `README.md` 警告 + 起動時 `eprintln!` 警告 + `--help` に非推奨注記 |
