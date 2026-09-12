# security.md

**Role:** セキュリティ設計・機密管理（S-xxx / R-xxx、I-1〜I-11 のレビュー記録）。

Generated: 2026-05-01 | Scope: Full codebase (`src/`, `install.sh`, `docs/`)

**SSOT split:** This file tracks **security review findings (S-xxx)** and **implementation risks (R-xxx)** (table below). Invariant IDs (I-1〜I-11) are referenced across `docs/` and defined by the code guards they name (simulate-first, mainnet `--yes`, …).

## S-xxx ↔ R-xxx cross-reference

| S-ID | Topic | Related R-ID | Notes |
|------|-------|--------------|-------|
| S-001 | Seed or mnemonic in `Debug` output | R-001 | Plaintext logging; use `secret_seed` / `secret_mnemonic` only after `Config::new()` |
| S-002 | Signing env var not cleared | R-001, I-4 | `Config::new()` removes `XRPL_SEED` and `XRPL_MNEMONIC` |
| S-003 | Config file credential permissions | R-001 | Unix warn on group/world read |
| S-010 | Plaintext credential retained on `Config` | R-001 | Cleared in `Config::new()` |
| S-011 | CLI credentials visible in `ps` | R-001 | Prefer env/file; startup warning |
| S-006 | `Tui::drop` panic / raw mode | R-005, I-8 | `eprintln!` on `exit()` failure |
| S-009 | `--self-uninstall` data deletion | — | User-driven; not an R entry |
| S-004, S-005, S-007, S-008 | R-007 (merge), — | See table below |
| S-012, S-013 | R-012 | NFT DNS resolution and metadata bounds |
| S-014 | R-013 | Wallet submit serialization |
| S-015 | R-014 | Plaintext RPC with signing seed |
| S-016 | R-015 | Issuer and financial input validation |

Risks without a matching S entry (e.g. R-006 mainnet guard bypass, R-008 RPC 429) are tracked in the R-xxx table below and [`test.md`](test.md).

## R-xxx implementation risks

| R-ID | S-ID (if any) | One-line |
|------|---------------|----------|
| R-001 | S-001, S-002, S-010, S-011 | Seed resolution / `secret_seed` vs cleared `seed` |
| R-002 | — | Submit errors dropped on closed `action_tx` |
| R-003 | — | `ArcValue` shared JSON mutation |
| R-004 | — | Unbounded channel growth |
| R-005 | S-006 | TUI Drop / terminal raw mode |
| R-006 | — | Mainnet / **Xahau mainnet** `--yes` guard bypass |
| R-007 | — | Config merge precedence per-key |
| R-008 | — | RPC 429 / backoff |
| R-009 | — | Submit hash not verified |
| R-010 | — | Duplicate poll on ledger close |
| R-011 | — | Poll `RpcClient::connect` “instant death” (**accepted / non-issue**) |
| R-012 | S-012, S-013 | NFT URI DNS SSRF / metadata recursion (**mitigated**) |
| R-013 | S-014 | Concurrent wallet submissions (**mitigated in poll task**) |
| R-014 | S-015 | Insecure RPC with signing seed (**mitigated**) |
| R-015 | S-016 | Unvalidated issuer and financial numeric inputs (**mitigated**) |

## 2026-08-06 再レビュー対応

- **S-012 NFT外部URIのDNS SSRF**: 修正済み。`src/xrpl/nft_image.rs` は各URIのDNS応答を全件検査し、private/loopback/link-local/unique-local IPを拒否する。自動redirectを無効化し、redirectごとに解決・検査・接続先固定を行う。
- **S-013 NFT metadata再帰DoS**: 修正済み。metadata探索に深さ32・ノード数10,000の上限を設けた。レスポンス上限4 MiBも維持。
- **S-014 ウォレット同時送信**: 修正済み。poll task内の `account_info → simulate → sign → submit` を `PollContext.submit_lock` で直列化。blind retryは追加していない。submit失敗後は次操作でaccount_infoを再取得する。
- **S-015 insecure RPCでの署名**: 修正済み。署名seed保持中に `http://` または `ws://` endpointを使用すると起動を拒否する。読み取り専用利用はseed未設定時に限り許可。
- **S-016 取引入力のissuer・数値検証**: 修正済み。TrustSet、RegularKey、OfferCreateのclassic address checksum、IOU currency形式、有限かつ正数のIOU値、正のXRP dropsを検証する。

未完了の構造課題（`poll.rs`分割、Actionエラー型移行、fake RPC/WebSocket統合試験、Watch/Operate UI分離）は今回の安全修正の範囲外。次回変更ではlive testを補助扱いにし、ローカルfixtureを追加する。

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
| S-003 | 設定ファイルに署名資格情報（seed / mnemonic）が保存される場合のファイル権限チェックなし | Unix でグループ/ワールド読取時に `tracing::warn!`（`warn_if_config_world_readable`） |
| S-004 | env var 経由のパスに対するパス検証なし | `validated_path` で `..` を拒否。`canonicalize()` + ホーム外拒否は Phase 3 前に推奨 |
| S-006 | `tui.rs` の `Drop` 実装で `unwrap()` を使用 | `if let Err(e) = self.exit() { eprintln!(...) }` |
| S-007 | 組み込み設定のパース | ビルド時 `expect` で開発時検知 |
| S-008 | tracing ログのデフォルトレベルが INFO | S-001 対処後に問題なし |
| S-005 | `install.sh` で `NO_VERIFY=1` の警告が不十分 | `usage()` / ヘッダに MITM リスク警告を追加 |
| S-010 | `Config` が平文 `seed` を `Arc<Config>` として継続保持 | `RawSigningConfig` に `secret_seed: Option<SecretString>` を追加し、`Config::new()` / `main.rs` / `app.rs` で平文を即座にクリア |
| S-011 | CLI `--seed` がプロセス引数に残り `ps` から閲覧可能 | `README.md` 警告 + 起動時 `eprintln!` 警告 + `--help` に非推奨注記 |
