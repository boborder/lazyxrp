# security.md

**Role:** セキュリティ設計・機密管理（S-xxx / R-xxx、I-1〜I-11 のレビュー記録）。

Generated: 2026-05-01 | Scope: Full codebase (`src/`, `install.sh`, `docs/`)

This file tracks **security review findings (S-xxx)** and **implementation risks (R-xxx)** (table below). Invariant IDs (I-1〜I-11) are referenced across `docs/` and defined by the code guards they name (simulate-first, mainnet `--yes`, …).

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
| S-017 | R-016 | Endpoint network ≠ selected network at signing time |

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
| R-016 | S-017 | Custom endpoint on another network bypasses the `--yes` guard (**mitigated**, TC-203) |

## 2026-10-11 endpoint network 照合（S-017）

- **症状**: `XRPL_RPC_SERVER`（`rpc_server` / `--server` も同じ）は `--network` の preset より優先される（architecture §6.2）。mainnet endpoint を指したまま `--network testnet` で書き込むと、`--yes` ガードは testnet 扱いで外れ、署名・送信は mainnet に届いた。TUI の `<Ctrl-n>` も custom endpoint を維持するため同様。testnet E2E 中に、mainnet の `XRPL_RPC_SERVER` と `XRPL_SEED` が設定されたシェルで判明（その回の write は env を外していたため mainnet 送信なし）。
- **対応**: 署名前に `RpcClient::ensure_network` で `server_info.network_id` を照合し、不一致なら拒否（mainnet は `network_id` 省略 = 0、testnet 1、devnet 2、xahau 21337、xahau-test 21338 を実測）。TUI 全 XRPL submit（`fetch_account_summary_for_submit`）と CLI `send` / `offer-create` / `offer-cancel` が対象。write 1 回につき RPC が 1 回増える。
- **残余**: 照合はノードの自己申告。悪意ある endpoint は偽の `network_id` を返せる（信頼する endpoint を選ぶ前提は S-015 と同じ）。

## 2026-08-06 再レビュー対応

- **S-012 NFT外部URIのDNS SSRF**: 修正済み。`src/xrpl/nft_image.rs` は各URIのDNS応答を全件検査し、private/loopback/link-local/unique-local に加え unspecified (`0.0.0.0`)・broadcast・IPv4-mapped IPv6 (`::ffff:` 形) も拒否する。自動redirectを無効化し、redirectごとに解決・検査・接続先固定を行う (上限6ホップ・4 MiB — TC-146/147)。
- **S-013 NFT metadata再帰DoS**: 修正済み。metadata探索に深さ32・ノード数10,000の上限を設けた。レスポンス上限4 MiBも維持。
- **S-014 ウォレット同時送信**: 修正済み。poll task内の `account_info → simulate → sign → submit` を `PollContext.submit_lock` で直列化。blind retryは追加していない。submit失敗後は次操作でaccount_infoを再取得する。
- **S-015 insecure RPCでの署名**: 修正済み。署名seed保持中に `http://` または `ws://` endpointを使用すると起動を拒否する。読み取り専用利用はseed未設定時に限り許可。
- **S-016 取引入力のissuer・数値検証**: 修正済み。TrustSet、RegularKey、OfferCreateのclassic address checksum、IOU currency形式、有限かつ正数のIOU値、正のXRP dropsを検証する。

未完了の構造課題（`poll.rs`分割、Actionエラー型移行、fake RPC/WebSocket統合試験、Watch/Operate UI分離）は今回の安全修正の範囲外。次回変更ではlive testを補助扱いにし、ローカルfixtureを追加する。

## 2026-10-04 セキュリティ監査対応（ソース監査 → 局所再現 → 修正）

ソース監査のリード 14 件を順に確認し、以下を修正した（各テストは `docs/test.md` TC-175〜TC-182）。

- **Flare RPC の https 強制・署名拒否（S-015 拡張, R-014）**: `FLARE_RPC_URL` / preset を `lib.rs::check_flare_endpoint` で検査。`http://` は `--allow-insecure-rpc` なしで拒否、署名資格情報（XRPL seed/mnemonic または Flare EVM 鍵 env）がある場合は `--allow-insecure-rpc` でも拒否。Core Vault 宛先は Flare RPC の応答が決めるため、2026-10-11 から `[flare.fassets] expected_core_vault`（運用者が別経路で確認した値）と一致しない限り署名しない（`flare::check_core_vault`、未設定も拒否）。
- **node が決める Fee の上限**: `signing::MAX_NODE_FEE_DROPS`（2 XRP = 2,000,000 drops）超の Fee は署名前に拒否（TUI・CLI `Send` とも simulate 経路の `sequence_fee_ledger_from_simulate`）。CLI `Send` も simulate 必須（`tesSUCCESS` 以外は署名しない）、残高検査は `amount + fee` を `checked_add`。2026-10-11 から `parse::check_simulate_bound_to_request`（全 write 共通の `simulate_tx_requiring_tes_success` 内）で simulate の `Sequence` が要求値と一致し、`LastLedgerSequence` が `ledger_index` 超 `+ MAX_LLS_WINDOW`（100）以下であることも検査。残余: 窓は node 自身の `ledger_index` 基準。
- **`xrp-ledger.toml` 取得（S-012 拡張）**: manifest 由来 domain を ASCII ホスト名のみに制限し、NFT 画像と同じガード付き取得（`nft_image::fetch_via`/`client_for_url`: private/loopback 拒否・DNS 固定・redirect 毎に再検査・上限 1 MiB）を使用。
- **proxy 経由の DNS 固定バイパス**: `client_for_url` は `.no_proxy()`。**HTTP(S)_PROXY 必須環境では NFT 画像・toml の取得は失敗する（fail closed）**。private 判定に `100.64/10`・`192.0.0.0/24`・`198.18/15`・`240/4` を追加。
- **クラッシュ系**: 遠隔文字列のバイトスライス（`short_hex` / tx hash / path address）を文字単位に、Flare の `U256::to::<T>()` を `saturating_to` / `try_into` に、`ripple_path_find` のソートを `f64::total_cmp`（非有限値は末尾）に変更。panic hook は `process::exit(1)` のため、これらはすべて遠隔入力で TUI を終了させ得た。
- **RPC 本文上限**: 16 MiB を `Content-Length` 事前検査に加えて chunk 読み込み中にも強制。
- **CLI 出力の制御文字**: `error_message` / `engine_result(_message)` / tx hash を `util::sanitize_remote` で制御文字除去。
- **S-003**: 警告は `Config::new()`（`logging::init` より前）で出るため `tracing` ではなく `eprintln!`。
- **`install.sh`**: GitHub token を curl `-K -`(stdin) / wget `--config`(fd) 経由にして argv から除去。想定外文字を含む token は無視。
- **`cd.yml`**: `cargo package --locked`（token なし）で検証ビルドし、token 付きは `cargo publish --locked --no-verify`。

調査して**修正しなかった**もの（同一オペレーター内の操作 / ハードニング）: Ctrl-N 後のネットワーク表示と接続先のずれ、config の keybindings からの Action 束縛、URL に入れた provider token のログ出力、dUNL 署名未検証（表示専用）、NFT デコードの寸法上限、CD の ref/environment 制約（GitHub 側 ruleset で担保）。二重送信は 2026-10-11 に対応（TUI: 同一 params の write は結果 Ok/Err 到着まで拒否、TC-228）。

## 2026-10-11 セキュリティ監査 run-2 対応

needs_validation 4 件（Core Vault ピン・simulate の Sequence/LLS 照合・quote 通貨照合・simulate 文字列 sanitize）は上の 2026-10-04 節の各項目を参照。以下はハードニング（テストは `docs/test.md` TC-204〜214）。

- **env 秘密の取得**: `XRPL_SEED` / `XRPL_MNEMONIC` / Flare EVM 鍵（`[flare.fassets] evm_key_env`）は `config::take_secret_env` が `Config::new()` で一度だけ取り出して env から削除し、`SecretString` で保持、生 `String` は zeroize。EVM 鍵は以前は初回 execute 時に poll task 内で `remove_var`（他タスクが env を読み得る実行中の変更、失敗すると以後使えない）だった。**Linux では `remove_var` は初期 `/proc/self/environ` を書き換えない**（同一 uid からは読める）— env 渡しは引き続き同一ユーザー内の信頼が前提。
- **JSON-RPC client は redirect を追わない**（`client.rs`、`Policy::none()`）。https endpoint が `submit`（署名済み blob）を http へ 307 で転送する経路を閉じた。system proxy はそのまま（運用者の選択）。faucet も redirect なし + 本文上限。
- **tx hash** は署名済み blob からローカル計算。TUI の送信結果に simulate 由来の署名済み Fee を表示（`<hash> · fee N drops`）。署名前の確認ステップは無い（Fee 上限 `MAX_NODE_FEE_DROPS` が担保）。
- **入力**: `XRP:100:tail` のような余分な区切りを拒否。CLI `send` も composer と同じ `validate_payment_input`（amount > 0 など）を RPC 前に実行。
- **鍵生成 popup / faucet**: `GeneratedWalletKeys` は drop 時に seed を zeroize、popup は Network 切替で消去（表示は「Esc to dismiss · g new key」）。`-x faucet` の生成 seed は `SecretString`。
- **CI**: 全 GitHub Actions をコミット SHA に固定（タグはコメント）、`cross@0.2.5` 固定。

## 2026-09-17 依存監査対応（`mise run audit` 整備で検知）

- **RUSTSEC-2026-0253: lru 0.16.4 `LruCache::pop()` unsound（use-after-free）**: `alloy-provider` → `lru` の実行時依存。0.16.x 系に修正版が無く、alloy 1.8.3（1.x 最終）も古い lru のままだったため **alloy 1.8.3 → 2.4.2 にメジャーアップ**（`Cargo.toml` `alloy = "2"`）。API 使用面は primitives + `ProviderBuilder` + `EthereumWallet` のみで変更不要。alloy 2.4.2 の MSRV に合わせ `rust-version` 1.91 → 1.94.1 — **`rust-version` を 1.94.1 未満へ戻すと lru 0.16.4 が復活するので戻さない**。`cargo tree -i lru` = 単一 0.18.4。
- **RUSTSEC-2026-0285: rustls 0.23.44 TLS 1.3 handshake 暗号化レベル検証不備（2026-09-14 公表, Medium）**: `cargo update -p rustls` で **0.23.45** に更新（0.23.x 系内パッチ）。
- いずれも ignore 追加ではなく依存更新で解決。`mise run audit`（cargo-audit + cargo-deny）が CI の security-advisories と同一基準でローカル検知できるようになった初回実行で発見。

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
| S-002 | `Config::new()` が環境変数を読み取り後に削除しない | `unsafe { env::remove_var(SEED_ENV) }` で除去 |
| S-003 | 設定ファイルに署名資格情報（seed / mnemonic）が保存される場合のファイル権限チェックなし | Unix でグループ/ワールド読取時に **stderr へ** 警告（`warn_if_config_world_readable`。`logging::init` 前に走るため `tracing` は不可） |
| S-004 | env var 経由のパスに対するパス検証なし | `validated_path` で `..` を拒否。`canonicalize()` + ホーム外拒否は Phase 3 前に推奨 |
| S-006 | `tui.rs` の `Drop` 実装で `unwrap()` を使用 | `if let Err(e) = self.exit() { eprintln!(...) }` |
| S-007 | 組み込み設定のパース | ビルド時 `expect` で開発時検知 |
| S-008 | tracing ログのデフォルトレベルが INFO | S-001 対処後に問題なし |
| S-005 | `install.sh` で `NO_VERIFY=1` の警告が不十分 | `usage()` / ヘッダに MITM リスク警告を追加 |
| S-010 | `Config` が平文 `seed` を `Arc<Config>` として継続保持 | `RawSigningConfig` に `secret_seed: Option<SecretString>` を追加し、`Config::new()` / `main.rs` / `app.rs` で平文を即座にクリア |
| S-011 | CLI `--seed` がプロセス引数に残り `ps` から閲覧可能 | `README.md` 警告 + 起動時 `eprintln!` 警告 + `--help` に非推奨注記 |
