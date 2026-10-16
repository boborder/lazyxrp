# security.md

**Role:** セキュリティ設計・機密管理（S-xxx / R-xxx、I-1〜I-11 のレビュー記録）。

Generated: 2026-05-01 | Scope: Full codebase (`src/`, `install.sh`, `docs/`)

This file tracks **security review findings (S-xxx)** and **implementation risks (R-xxx)** (table below). Invariant IDs (I-1〜I-11) are referenced across `docs/` and the code:

| ID | Invariant | Enforced in |
|----|-----------|-------------|
| I-1 | 平文 `seed` / `mnemonic` は `Config::new()` で `None` に消し、`secret_seed` / `secret_mnemonic`（`SecretString`）のみ残す | `config.rs` |
| I-2 | mainnet（`Network::is_production()`）の write は `--yes` または `skip_mainnet_prompt: true` が必須 | `poll.rs` / `submit.rs` の各 submit |
| I-3 | write は必ず simulate → `tesSUCCESS` → `Sequence`/`Fee` 取得 → sign → submit。simulate なしで署名しない | `poll.rs` / `submit.rs` |
| I-4 | `XRPL_SEED` / `XRPL_MNEMONIC` は tokio 起動前の単一スレッド `Config::new()` で env から除去 | `config.rs` |
| I-5 | 設定優先順位: CLI > env > user `config.toml` > 組み込み `config.json5` | `lib.rs` resolve 関数 + `config.rs` |
| I-6 | 定期 poll は `MIN_POLL_INTERVAL`（10 s）未満にしない | `poll.rs` `drive_poll_loop` |
| I-7 | `account_tx` の not-found は空リスト（エラーにしない） | `poll.rs` |
| I-8 | `Tui::drop` は端末を復元し、drop 中に panic しない | `tui.rs` |
| I-9 | `TAB_TITLES` とパネルの数・順序が一致（TC-060）。パネル追加時は同じ変更で更新 | `app.rs` |
| I-10 | `TxRow` の `ArcValue` JSON を `Arc::make_mut` しない | `xrpl/types.rs`（規約） |
| I-11 | 全 RPC は `tokio::time::timeout(RPC_TIMEOUT, …)` で包む | `poll.rs` / `client.rs` |
| — | 署名は family seed **または** BIP39 mnemonic（`credential_from_secrets`）。mnemonic を seed に変換しない | `signing.rs` |

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
| S-018 | R-017 | Smart Account operator (payment destination) and fee come from one Flare RPC answer |

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
| R-017 | S-018 | Smart Account instruction Payment to an RPC-supplied operator / fee (**mitigated**, TC-240 / TC-243) |
| R-018 | — | Spec'd (generic) write path: guards live only in shared helpers; irreversible types must set `confirm` (**mitigated**, TC-088 table / TC-252–255) |

## 2026-10-16 宣言的 TX spec の書き込み面（NFT family、R-018）

- **新しい書き込み面**: `Action::GenericTxSubmit` → `PollCommand::GenericTx` → `submit_generic_transaction`（`-x tx` / `nft-mint` / `nft-burn` は `cli_exec` 経由で同じ `simulate_confirm_submit`）。全 spec'd 種別がこの 1 経路を通るため、ガードは種別ごとに複製せず共有ヘルパだけに置く: `TxSpec::prepare`（フィールド検証・未知フィールド拒否・`Time` を絶対秒に 1 回だけ確定）→ mainnet `--yes`（`run_submit` / `resolve_submit_wallet`）→ `ensure_network` → `account_info` → simulate `tesSUCCESS` かつ要求への束縛（`check_simulate_bound_to_request`）→ Fee 上限 → 署名 → submit、すべて poll の `submit_lock` 下。`--watch` は `Cmd::signs` で拒否。
- **不可逆操作の確認規則**: 不可逆（NFTokenBurn = NFT 破棄、NFTokenAcceptOffer = 資産移転と支払い）な種別は `TxSpec::confirm` に確認フレーズ（`BURN` / `ACCEPT`）を持ち、TUI composer は `s` で直接 queue せず AccountDelete と同じ type-to-confirm（`DangerConfirm`）を必ず挟む（部分入力は拒否、Esc でフォーム復帰）。CLI は mainnet の確認プロンプト / `--yes` と simulate 結果の表示。**今後の family で不可逆な種別を追加するときは `confirm` を必ず設定する**（設定しない種別は 1 キーで署名まで進む）。
- **入力の信頼境界**: URI は平文を hex 化するため任意バイト列にならない（≤ 256 bytes）。id は 64 hex・アドレスは checksum・金額は既存の `parse_offer_amount`（0・非有限は拒否）で検証し、composer と poll で同じ `TxSpec::validate` を使う。Assets タブ `x` の prefill は表示中の NFT 行（ノード由来）の `nft_id` だが、フォームの確認と simulate を必ず通る。
- **残余**: NFT 一覧は watch アカウントのもので署名アカウントとは限らない（所有者が違えば simulate が `tec*` で拒否）。`tfMutable` mint・0 価格オファーは未対応（crate / 共通パーサの制約）。
- **Account objects family（Check / Ticket / DepositPreauth / SignerListSet / Clawback）**: 資産移転・不可逆・ロックアウトの恐れがある CheckCash（`CASH`）/ SignerListSet（`SIGNERS`：誤った list + master key 無効でアカウントがロック）/ Clawback（`CLAWBACK`）は型付き確認フレーズ必須。SignerEntries は `addr:weight` を検証（≤ 8 件・重み 1..65535・重複なし・quorum ≤ 重み合計）。DepositPreauth の credentials・Clawback の MPT は未対応。
- **XChain family（XChainCreateBridge / Commit / Claim / AccountCreateCommit）**: ブリッジ新設（削除 tx なし）・資産ロック／バーンを伴う Commit・Claim・AccountCreateCommit は型付き確認（`BRIDGE` / `COMMIT` / `CLAIM`）必須。`XChainBridge` は 4 要素の flat 文字列を厳格検証（checksum・door 相違・XRP/IOU 混在拒否）し、Commit / Claim の Amount は bridge の issue と一致必須。witness attestation は署名を form で作れないため未対応。
- **Channel / AMM family（PaymentChannelCreate / Fund / Claim、AMMCreate / Bid / Delete）**: XRP ロック・資産移転・LP トークン消費・不可逆な種別は型付き確認（`LOCK` / `FUND` / `CLAIM` / `CREATE` / `BID` / `DELETE`）必須、AMMVote のみ確認なし。Claim の `Signature` / `PublicKey` は貼り付け入力の hex 検証のみで、署名はローカルで生成しない（`CredentialIDs` は未対応）。drops は 1..=1e17 の整数、AuthAccounts は ≤ 4 件・重複なし・署名者自身は拒否。AMMCreate は owner reserve 相当の特別 Fee を burn するため Fee 上限ガード（共通経路）が効く。
- **MPT / Identity family（MPTokenIssuanceDestroy、CredentialDelete、DIDDelete、OracleDelete、PermissionedDomainDelete）**: 不可逆な削除系は型付き確認（`DESTROY` / `DELETE`）必須。Credential / DID / Oracle の blob は平文を hex 化（任意バイト列を直接入れない）、価格は decimal u64 のみ、Domain ID はゼロハッシュ拒否。MPT の確認不要な Set（lock / unlock）・Authorize は可逆。Vault* と MPT mutation / confidential 系は未対応（`coverage.rs` に理由）。

## 2026-10-16 Smart Account operator pin（S-018）

- **新しい信頼境界**: Flare Smart Accounts の instruction Payment の宛先（`MasterAccountController.getXrplProviderWallets()[0]`）と `Amount`（`getInstructionFee(id)`）は Flare RPC の単一の応答で決まる。悪意ある / 侵害された RPC は自分の XRPL アドレスを operator として返し、ユーザーの XRP と FXRP 操作の意図（32-byte memo）を横取りできる。Core Vault（C2）と同じ構造。
- **対応**: `[flare.smart_accounts] expected_operator`（運用者が別経路で確認した値）と `flare::check_operator` で完全一致したときのみ署名。未設定・不一致は署名前に拒否（RPC 前、TC-240 / TC-243）。fee は `signing::check_smart_account_fee` で `1..=10 XRP` に限定（0 や桁違いの応答を拒否）。Payment は classic 宛先・`DestinationTag` なし（tag があると direct minting 経路で tag 保有者に credit される）、operator ≠ 署名アカウント。mainnet は `--yes`（TC-088 表に追加）、simulate `tesSUCCESS` 後のみ署名。
- **残余**: pin は運用者の責任（誤った値を設定すれば誤った宛先に送る）。operator の実行（FDC 証明 + `executeInstruction`）は XRPL 外で、lazyxrp は結果を検証しない（TUI は「personal account の FXRP 残高を確認」と案内）。`getPersonalAccount` / 残高は表示用で署名可否には使わない（残高超過の transfer だけ composer で拒否）。
- **単位（FXRP の `decimals()`）**: transfer の value は FXRP の最小単位そのもの（コントラクトは換算せず `transfer(to, value)` に渡す）。composer は 10 進の額を、同じ Flare RPC から読んだ `decimals()` で整数演算のみで変換する（桁数超過・`decimals > 18`・未取得は拒否、TC-247）。**残余**: `decimals()` と残高は operator と同じ単一 RPC の応答なので、悪意ある RPC は桁をずらした額を送らせうる。ただし送金額はユーザー自身の personal account の FXRP に限られ、宛先はユーザーが入力した受取先で、operator だけが pin で固定される（RPC の `decimals()` / 残高は pin 対象外。信頼する RPC を選ぶ前提は S-015 と同じ）。

## 2026-10-11 endpoint network 照合（S-017）

- **症状**: `XRPL_RPC_SERVER`（`rpc_server` / `--server` も同じ）は `--network` の preset より優先される（architecture §6.2）。mainnet endpoint を指したまま `--network testnet` で書き込むと、`--yes` ガードは testnet 扱いで外れ、署名・送信は mainnet に届いた。TUI の `<Ctrl-n>` も custom endpoint を維持するため同様。testnet E2E 中に、mainnet の `XRPL_RPC_SERVER` と `XRPL_SEED` が設定されたシェルで判明（その回の write は env を外していたため mainnet 送信なし）。
- **対応**: 署名前に `RpcClient::ensure_network` で `server_info.network_id` を照合し、不一致なら拒否（mainnet は `network_id` 省略 = 0、testnet 1、devnet 2、xahau 21337、xahau-test 21338 を実測）。TUI 全 XRPL submit（`fetch_account_summary_for_submit`）と CLI `send` / `offer-create` / `offer-cancel` が対象。write 1 回につき RPC が 1 回増える。
- **残余**: 照合はノードの自己申告。悪意ある endpoint は偽の `network_id` を返せる（信頼する endpoint を選ぶ前提は S-015 と同じ）。

## 2026-08-06 再レビュー対応

- **S-012 NFT外部URIのDNS SSRF**: 修正済み。`src/xrpl/nft_image.rs` は各URIのDNS応答を全件検査し、private/loopback/link-local/unique-local に加え unspecified (`0.0.0.0`)・`0.0.0.0/8`・broadcast・site-local `fec0::/10`、IPv6 に埋め込まれた IPv4（mapped `::ffff:`・compatible `::a.b.c.d`・NAT64 `64:ff9b::/96`・6to4 `2002::/16`）も中身の IPv4 で判定して拒否する。自動redirectを無効化し、redirectごとに解決・検査・接続先固定を行う (上限6ホップ・4 MiB — TC-146/147)。https → http の redirect は拒否（TC-237）。
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
- **`--watch`（閲覧専用起動、2026-10-11、TC-234–235）**: `lib::enter_watch_mode` が起動直後に `Config` から全鍵（seed / mnemonic の平文欄と secret 欄、`flare_evm_key`）を消し、アドレスだけを表示用に残す。TUI は Wallet（composer・鍵生成）を出さず、poll task に credential が渡らない。`-x` の署名系サブコマンドは RPC 前に拒否。
- **クラッシュ系**: 遠隔文字列のバイトスライス（`short_hex` / tx hash / path address）を文字単位に、Flare の `U256::to::<T>()` を `saturating_to` / `try_into` に、`ripple_path_find` のソートを `f64::total_cmp`（非有限値は末尾）に変更。panic hook は `process::exit(1)` のため、これらはすべて遠隔入力で TUI を終了させ得た。
- **RPC 本文上限**: 16 MiB を `Content-Length` 事前検査に加えて chunk 読み込み中にも強制。
- **リモート文字列の制御・不可視文字**: `error_message` / `engine_result(_message)` / tx hash（`util::sanitize_remote`）と Server パネルの host / version（`sanitize_text`）から制御文字・bidi override・zero-width 書式文字を除去（TC-236）。
- **S-003**: 警告は `Config::new()`（`logging::init` より前）で出るため `tracing` ではなく `eprintln!`。
- **`install.sh`**: GitHub token を curl `-K -`(stdin) / wget `--config`(fd) 経由にして argv から除去。想定外文字を含む token は無視。curl は `--proto '=https' --proto-redir '=https'`（redirect 先も https のみ）、README のワンライナーも `--proto '=https' --tlsv1.2`。wget フォールバックは scheme 制限なし（`--https-only` は再帰時のみ有効）。
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
