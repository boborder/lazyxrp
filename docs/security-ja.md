# security.md（日本語版）

[English](security.md)

**Role:** セキュリティ設計・機密管理（S-xxx / R-xxx、I-1〜I-11 のレビュー記録）。

Generated: 2026-05-01 | Scope: コードベース全体（`src/`、`install.sh`、`docs/`）

このファイルは **セキュリティレビュー指摘（S-xxx）** と **実装リスク（R-xxx）** を追跡する（下表）。不変条件 ID（I-1〜I-11）は `docs/` とコードの両方から参照される。

| ID | 不変条件 | 強制箇所 |
|----|----------|----------|
| I-1 | 平文 `seed` / `mnemonic` は `Config::new()` で `None` に消し、`SecretString` のみ残す。`lib.rs::run` が `Config::take_secrets()` で `Secrets`（seed / mnemonic / Flare EVM 鍵）へ移してから `Arc<Config>` を共有する（共有 config は秘密を持たない） | `config.rs` / `lib.rs` |
| I-2 | mainnet（`Network::is_production()`）の write は `--yes` または `skip_mainnet_prompt: true` が必須 | `poll.rs` / `submit.rs` の各 submit |
| I-3 | write は必ず simulate → `tesSUCCESS` → `Sequence`/`Fee` 取得 → sign → submit。simulate なしで署名しない | `poll.rs` / `submit.rs` |
| I-4 | `XRPL_SEED` / `XRPL_MNEMONIC` は `Config::new()` が poll / UI タスクを spawn する前に一度だけ取り出して env から除去（`take_secret_env`）。以降 `remove_var` は呼ばれない。Linux では起動時の `/proc/self/environ` ブロックは書き換わらず、同一 uid の読み手には見え続ける（既知の限界） | `config.rs` |
| I-5 | 設定優先順位: CLI > env > user `config.toml` > 組み込み `config.json5`。資格情報は上位ソースが seed / mnemonic のどちらかを設定したら下位ソースの両方を破棄（同一階層で両方設定は `credential_from_secrets` がエラー） | `lib.rs` resolve 関数 + `config.rs` (`layer_credentials`, `apply_cli_credentials`) |
| I-6 | WebSocket の ledger close トリガーは直前の poll から `MIN_POLL_INTERVAL`（10 s）以内なら poll しない。定期 tick は `poll_interval_ms`（下限 500 ms）に従う | `poll.rs` `drive_poll_loop` |
| I-7 | `account_tx` の not-found は空リスト（エラーにしない） | `poll.rs` |
| I-8 | `Tui::drop` は端末を復元し、drop 中に panic しない | `tui.rs` |
| I-9 | `TAB_TITLES` とパネルの数・順序が一致（`App::new` の `debug_assert_eq!`）。パネル追加時は同じ変更で更新 | `app.rs` |
| I-10 | `TxRow` の `ArcValue` JSON を `Arc::make_mut` しない | `xrpl/types.rs`（規約） |
| I-11 | 全 RPC は `tokio::time::timeout(RPC_TIMEOUT, …)` で包む | `poll.rs` / `client.rs` |
| — | 署名は family seed **または** BIP39 mnemonic（`credential_from_secrets`）。mnemonic を seed に変換しない | `signing.rs` |

## S-xxx ↔ R-xxx 相互参照

| S-ID | 項目 | 関連 R-ID | 備考 |
|------|------|-----------|------|
| S-001 | seed または mnemonic が `Debug` 出力に出る | R-001 | 平文ログ出力。秘密は `Secrets`（`Config::take_secrets()` 経由）に置き、共有 `Arc<Config>` には置かない |
| S-002 | 署名用 env 変数が消されない | R-001, I-4 | `Config::new()` が `XRPL_SEED` と `XRPL_MNEMONIC` を除去 |
| S-003 | config ファイルの資格情報のパーミッション | R-001 | Unix でグループ/ワールド読取時に警告 |
| S-010 | 平文の資格情報が `Config` に残る | R-001 | 平文は `Config::new()` で消去。`Arc<Config>` の前に `take_secrets()` で秘密を移す |
| S-011 | CLI の資格情報が `ps` から見える | R-001 | env/ファイルを推奨。起動時に警告 |
| S-006 | `Tui::drop` の panic / raw mode | R-005, I-8 | `exit()` 失敗時は `eprintln!` |
| S-009 | `--self-uninstall` のデータ削除 | — | ユーザー主導。R 項目なし |
| S-004, S-005, S-007, S-008 | R-007 (merge), — | 下表を参照 |
| S-012, S-013 | R-012 | NFT の DNS 解決とメタデータの上限 |
| S-014 | R-013 | ウォレット submit の直列化 |
| S-015 | R-014 | 署名 seed を持つ状態での平文 RPC |
| S-016 | R-015 | issuer と金額入力の検証 |
| S-017 | R-016 | 署名時に endpoint の network が選択 network と異なる |
| S-018 | R-017 | Smart Account の operator（支払い先）と fee が 1 回の Flare RPC 応答で決まる |
| S-019 | R-019 | FXRP Mint の受取先が Flare RPC の応答（`getPersonalAccount`）で埋められる |

対応する S 項目を持たないリスク（例: R-006 mainnet ガード迂回、R-008 RPC 429）は、下の R-xxx 表と [`test-ja.md`](test-ja.md) で追跡する。

## R-xxx 実装リスク

| R-ID | S-ID（あれば） | 一行説明 |
|------|----------------|----------|
| R-001 | S-001, S-002, S-010, S-011 | seed の解決 / `Secrets` と消去済み `seed` |
| R-002 | — | `action_tx` が閉じている場合に submit エラーが捨てられる |
| R-003 | — | `ArcValue` の共有 JSON の変更 |
| R-004 | — | channel の無制限な増大 |
| R-005 | S-006 | TUI の Drop / 端末 raw mode |
| R-006 | — | mainnet / **Xahau mainnet** の `--yes` ガード迂回 |
| R-007 | — | config マージ優先順位（キー単位） |
| R-008 | — | RPC 429 / backoff |
| R-009 | — | submit hash が未検証 |
| R-010 | — | ledger close 時の poll 重複 |
| R-011 | — | poll の `RpcClient::connect` の「即死」（**受容 / 非問題**） |
| R-012 | S-012, S-013 | NFT URI の DNS SSRF / メタデータ再帰（**対応済み**） |
| R-013 | S-014 | ウォレットの同時送信（**poll task 内で対応済み**） |
| R-014 | S-015 | 署名 seed を持つ状態での insecure RPC（**対応済み**） |
| R-015 | S-016 | issuer と金額数値入力の未検証（**対応済み**） |
| R-016 | S-017 | 別 network の custom endpoint が `--yes` ガードを迂回（**対応済み**、TC-203） |
| R-017 | S-018 | RPC が返した operator / fee への Smart Account instruction Payment（**対応済み**、TC-240 / TC-243） |
| R-019 | S-019 | C2 の受取先既定値 = RPC が返した personal account（**受容**: 全桁表示・編集可能・ユーザーが確認、TC-367） |
| R-018 | — | spec 化された（汎用）write 経路: ガードは共有ヘルパだけにある。不可逆な種別は `confirm` を必ず設定（**対応済み**、TC-088 表 / TC-252–255） |

## 2026-10-16 宣言的 TX spec の書き込み面（NFT family、R-018）

- **新しい書き込み面**: `Action::GenericTxSubmit` → `PollCommand::GenericTx` → `submit_generic_transaction`（`-x tx` / `nft-mint` / `nft-burn` は `cli_exec` 経由で同じ `simulate_confirm_submit`）。全 spec'd 種別がこの 1 経路を通るため、ガードは種別ごとに複製せず共有ヘルパだけに置く。流れは `TxSpec::prepare`（フィールド検証・未知フィールド拒否・`Time` を絶対秒に 1 回だけ確定）→ mainnet `--yes`（`run_submit` / `resolve_submit_wallet`）→ `ensure_network` → `account_info` → simulate `tesSUCCESS` かつ要求への束縛（`check_simulate_bound_to_request`）→ Fee 上限 → 署名 → submit で、すべて poll の `submit_lock` 下にある。`--watch` は `Cmd::signs` で拒否。
- **不可逆操作の確認規則**: 不可逆（NFTokenBurn = NFT 破棄、NFTokenAcceptOffer = 資産移転と支払い）な種別は `TxSpec::confirm` に確認フレーズ（`BURN` / `ACCEPT`）を持ち、TUI composer は `s` で直接 queue せず AccountDelete と同じ type-to-confirm（`DangerConfirm`）を必ず挟む（部分入力は拒否、Esc でフォーム復帰）。CLI は mainnet の確認プロンプト / `--yes` と simulate 結果の表示。**今後の family で不可逆な種別を追加するときは `confirm` を必ず設定する**（設定しない種別は 1 キーで署名まで進む）。
- **入力の信頼境界**: URI は平文を hex 化するため任意バイト列にならない（≤ 256 bytes）。id は 64 hex・アドレスは checksum・金額は既存の `parse_offer_amount`（0・非有限は拒否）で検証し、composer と poll で同じ `TxSpec::validate` を使う。Assets タブ `x` の prefill は表示中の NFT 行（ノード由来）の `nft_id` だが、フォームの確認と simulate を必ず通る。
- **残余**: NFT 一覧は watch アカウントのもので署名アカウントとは限らない（所有者が違えば simulate が `tec*` で拒否）。`tfMutable` mint・0 価格オファーは未対応（crate / 共通パーサの制約）。
- **Account objects family（Check / Ticket / DepositPreauth / SignerListSet / Clawback）**: 資産移転・不可逆・ロックアウトの恐れがある CheckCash（`CASH`）/ SignerListSet（`SIGNERS`：誤った list + master key 無効でアカウントがロック）/ Clawback（`CLAWBACK`）は型付き確認フレーズ必須。SignerEntries は `addr:weight` を検証（≤ 8 件・重み 1..65535・重複なし・quorum ≤ 重み合計）。DepositPreauth の credentials・Clawback の MPT は未対応。
- **XChain family（XChainCreateBridge / Commit / Claim / AccountCreateCommit）**: ブリッジ新設（削除 tx なし）・資産ロック／バーンを伴う Commit・Claim・AccountCreateCommit は型付き確認（`BRIDGE` / `COMMIT` / `CLAIM`）必須。`XChainBridge` は 4 要素の flat 文字列を厳格検証（checksum・door 相違・XRP/IOU 混在拒否）し、Commit / Claim の Amount は bridge の issue と一致必須。witness attestation は署名を form で作れないため未対応。
- **Channel / AMM family（PaymentChannelCreate / Fund / Claim、AMMCreate / Bid / Delete）**: XRP ロック・資産移転・LP トークン消費・不可逆な種別は型付き確認（`LOCK` / `FUND` / `CLAIM` / `CREATE` / `BID` / `DELETE`）必須、AMMVote のみ確認なし。Claim の `Signature` / `PublicKey` は貼り付け入力の hex 検証のみで、署名はローカルで生成しない（`CredentialIDs` は未対応）。drops は 1..=1e17 の整数、AuthAccounts は ≤ 4 件・重複なし・署名者自身は拒否。AMMCreate は owner reserve 相当の特別 Fee を burn するため Fee 上限ガード（共通経路）が効く。
- **MPT / Identity family（MPTokenIssuanceDestroy、CredentialDelete、DIDDelete、OracleDelete、PermissionedDomainDelete）**: 不可逆な削除系は型付き確認（`DESTROY` / `DELETE`）必須。Credential / DID / Oracle の blob は平文を hex 化（任意バイト列を直接入れない）、価格は decimal u64 のみ、Domain ID はゼロハッシュ拒否。MPT の確認不要な Set（lock / unlock）・Authorize は可逆。Vault* と MPT mutation / confidential 系は未対応（`coverage.rs` に理由）。

## 2026-10-24 C2 受取先の既定値と finalize 待ち（S-019 / R-019）

- **新しい信頼境界（S-019）**: XRPL 署名だけで「XRP → 自分の personal account の FXRP」を完結させるため、FXRP Mint composer は受取先を署名アカウントの personal account（`getPersonalAccount` の応答）で埋める。この値は Flare RPC の単一の応答で、Core Vault / operator のような pin はなく、悪意ある / 侵害された RPC は攻撃者のアドレスを返して mint 先を奪える（Core Vault は pin 済みなので XRP の宛先は変わらないが、FXRP の受取人は変わる）。**対応**: 読み取りが署名アカウントのもので 300 秒以内のときだけ埋める、全桁を表示して「Flare RPC の応答なので explorer で照合」と明示、編集可能、送信直前の `verify_direct_mint_terms` は従来どおり。**残余（accepted）**: RPC 自体を信用する前提は S-015 と同じで、2 つ目の独立した情報源での照合や pin（`expected_personal_account`）は設けていない。心配なら受取先を手入力するか、信頼する RPC（`FLARE_RPC_URL`）を使う。表示とユーザーによる確認以外で検討した唯一の緩和策は、任意の `[flare.smart_accounts] expected_personal_account` pin だが、未実装（ROADMAP F5-b、P3）。
- **Smart Account のデータは永続化しない**: Smart Account については何も保存しない。表示する内容（personal account、nonce、operator wallet、instruction fee、FXRP 残高、vault 持ち分）はすべて Overview の poll ごとに Flare から読み、メモリ上にだけ保持する（`SmartAccountInfo`。300 秒より古い、または別 network のスナップショットは拒否され、`verify_*_terms` が署名直前に view を読み直す。TC-363）。
- **finalize は第三者の取引**: mint の Flare 側（`executeDirectMinting[WithData]`）は executor が行う。mainnet の読み取り調査（2026-10-24、`docs/test.md`）で公開 executor は動いている（通常 905 件 / 10 者、Smart Account 1000+ 件 / 8 者、XRPL 確定から 95–177 s）が、低手数料の支払いが拾われる保証はない（手数料下限は未測定）。拾われない場合 XRP は Core Vault に残り、自分で finalize するには EVM 鍵・FLR（mainnet の attestation 約 20 FLR + gas）が要る。Smart Account の `executeDirectMintingWithData` が revert した場合も FXRP は mint されず XRP は自動では戻らない（`0xE0` 回復手順）。
- **`sa-memo` の待機・`sa-status`**: `isTransactionIdUsed` は読み取り専用で、署名可否・拒否には使わない（情報表示のみ）。支払いの validated 後は何があってもコマンドを失敗させない。Flare 読み取りの失敗は stderr に再確認の手順（`sa-status`）を出して exit 0 とする（非 0 だと再実行で Core Vault への 2 回目の支払いを誘う）。executor が固定されている場合、`0xE0` / `0xE1` / `0xE2` は固定された executor しか finalize できないので待たず、その旨を表示する（`0xD0` / `0xD1` は固定を迂回）。`sa-call`（`0xFE`）は公開 executor が userOp のバイトを持たないため待たない。この view は plain の recipient-memo mint（C2）を追跡しない（Coston2 で、公開 executor が finalize した C2 の tx が `false` なのを確認）。ガード（Core Vault pin・network 対・simulate・`--yes`）は変更なし。
- **未検証**: Flare mainnet への書き込みは引き続き一度も実送信していない（上記 2026-10-20 節）。`0xD1`（固定なしの executor 解除）の mainnet での通過も未確認。

## 2026-10-20 Flare EVM 書き込み面の拡張と FDC proof 取得（ユーザー承認済みの方針転換）

- **新しい書き込み面**（Flare EVM key）: `-x fxrp-execute`（FDC 手数料 + `executeDirectMinting`）/ `fxrp-redeem`（FXRP を burn）/ `fxrp-tag-reserve` / `fxrp-tag-set` / `fxrp-tag-transfer`。TUI には出さず CLI のみ。ガードは全て共通: `[flare.fassets] execute = true`（既定 false）→ 起動時に取り込んだ EVM key → https の Flare RPC → Flare mainnet は `--yes`。`--watch` は key を空にするので拒否される。各書き込みは前提を on-chain で読んでから `estimate_gas`（revert ならここで止まる）→ 送信。
- **FDC proof 取得は lazyxrp の外、という roadmap-log の方針を覆した**。新しい外部 HTTP 面は verifier と DA layer の 2 つ。対策は次のとおり。https 限定（`FDC_VERIFIER_URL` / `FDC_DA_URL` の上書きも）、redirect 無効、応答 1 MiB 上限、30 s timeout、verifier の `abiEncodedRequest` を tx hash / `proofOwner` / 型 / source id と byte 照合してから手数料を払う（偽の request に手数料を払わせない）、手数料上限 100 FLR（実測 20 FLR）、DA 応答は実行前に proof parser で形を検査。proof の正否は contract が Merkle 検証するため、DA / verifier が偽造できるのは手数料の浪費まで。proof は署名鍵の address に束縛される（他者は使えない）。
- **残るリスク**: 手数料（約 20 FLR）は round が finalize しない / DA が応答しない場合に戻らない。`fxrp-tag-transfer` は取り消せない。`sa-vault` の `value` は生値で受ける（単位は upstream `Instructions.sol` で確認: deposit = FXRP 最小単位、redeem = shares、claim = period / `YYYYMMDD`）ため、桁の取り違えは利用者側の責任で、メモ hex を事前表示する。`fxrp-execute` / `fxrp-redeem` / `sa-vault` は XRPL と Flare の network 対（mainnet ↔ flare、testnet ↔ coston2）を RPC 前に強制する。実送信の確認状況: Coston2 では `fxrp-redeem`（`redeemAmount` / `redeemWithTag`）、`sa-vault`、`sa-memo`、`sa-call`、`fxrp-execute` を実送信で確認済み（ROADMAP F3-b〜F3-d、TC-345）。Flare mainnet への書き込みと `fxrp-tag-reserve` / `fxrp-tag-set` / `fxrp-tag-transfer` は実送信していない（拒否経路のみ実測、ROADMAP F3-a）。

## 2026-10-16 Smart Account operator pin（S-018）

- **新しい信頼境界**: Flare Smart Accounts の instruction Payment の宛先（`MasterAccountController.getXrplProviderWallets()[0]`）と `Amount`（`getInstructionFee(id)`）は Flare RPC の単一の応答で決まる。悪意ある / 侵害された RPC は自分の XRPL アドレスを operator として返し、ユーザーの XRP と FXRP 操作の意図（32-byte memo）を横取りできる。Core Vault（C2）と同じ構造。
- **対応**: `[flare.smart_accounts] expected_operator`（運用者が別経路で確認した値）と `flare::check_operator` で完全一致したときのみ署名。未設定・不一致は署名前に拒否（RPC 前、TC-240 / TC-243）。fee は `signing::check_smart_account_fee` で `1..=10 XRP` に限定（0 や桁違いの応答を拒否）。Payment は classic 宛先・`DestinationTag` なし（tag があると direct minting 経路で tag 保有者に credit される）、operator ≠ 署名アカウント。mainnet は `--yes`（TC-088 表に追加）、simulate `tesSUCCESS` 後のみ署名。
- **残余**: pin は運用者の責任（誤った値を設定すれば誤った宛先に送る）。operator の実行（FDC 証明 + `executeInstruction`）は XRPL 外で、lazyxrp は結果を検証しない（TUI は「personal account の FXRP 残高を確認」と案内）。`getPersonalAccount` / 残高は表示用で、Smart Account instruction の署名可否には使わない（残高超過の transfer だけ composer で拒否）。**例外**: FXRP Mint composer（C2）の受取先の既定値（S-019 / R-019、上記 2026-10-24）。
- **単位（FXRP の `decimals()`）**: transfer の value は FXRP の最小単位そのもの（コントラクトは換算せず `transfer(to, value)` に渡す）。composer は 10 進の額を、同じ Flare RPC から読んだ `decimals()` で整数演算のみで変換する（桁数超過・`decimals > 18`・未取得は拒否、TC-247）。**残余**: `decimals()` と残高は operator と同じ単一 RPC の応答なので、悪意ある RPC は桁をずらした額を送らせうる。ただし送金額はユーザー自身の personal account の FXRP に限られ、宛先はユーザーが入力した受取先で、operator だけが pin で固定される（RPC の `decimals()` / 残高は pin 対象外。信頼する RPC を選ぶ前提は S-015 と同じ）。

## 2026-10-11 endpoint network 照合（S-017）

- **症状**: `XRPL_RPC_SERVER`（`rpc_server` / `--server` も同じ）は `--network` の preset より優先される（architecture §6.2）。mainnet endpoint を指したまま `--network testnet` で書き込むと、`--yes` ガードは testnet 扱いで外れ、署名・送信は mainnet に届いた。TUI の `<Ctrl-n>` も custom endpoint を維持するため同様。testnet E2E 中に、mainnet の `XRPL_RPC_SERVER` と `XRPL_SEED` が設定されたシェルで判明（その回の write は env を外していたため mainnet 送信なし）。
- **対応**: 署名前に `RpcClient::ensure_network` で `server_info.network_id` を照合し、不一致なら拒否（mainnet は `network_id` 省略 = 0、testnet 1、devnet 2、xahau 21337、xahau-test 21338 を実測）。TUI 全 XRPL submit（`fetch_account_summary_for_submit`）と CLI の全 XRPL write（`send`・`offer-*`・`amm-*`・`account-delete`・`tx` / `nft-*`）が対象。write 1 回につき RPC が 1 回増える。
- **残余**: 照合はノードの自己申告。悪意ある endpoint は偽の `network_id` を返せる（信頼する endpoint を選ぶ前提は S-015 と同じ）。

## 2026-08-06 再レビュー対応

- **S-012 NFT外部URIのDNS SSRF**: 修正済み。`src/xrpl/nft_image.rs` は各URIのDNS応答を全件検査し、private/loopback/link-local/unique-local に加え unspecified (`0.0.0.0`)・`0.0.0.0/8`・broadcast・site-local `fec0::/10`、IPv6 に埋め込まれた IPv4（mapped `::ffff:`・compatible `::a.b.c.d`・NAT64 `64:ff9b::/96`・6to4 `2002::/16`）も中身の IPv4 で判定して拒否する。自動redirectを無効化し、redirectごとに解決・検査・接続先固定を行う (上限6ホップ・4 MiB、TC-146/147)。https → http の redirect は拒否（TC-237）。
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
- **`--watch`（閲覧専用起動、2026-10-11、TC-234–235）**: `lib::enter_watch_mode` が起動直後に `Secrets`（`take_secrets()` で `Config` から移した seed / mnemonic / `flare_evm_key`）を空にし、アドレスだけを表示用に残す。TUI は Wallet（composer・鍵生成）を出さず、poll task に credential が渡らない。`-x` の署名系サブコマンドは RPC 前に拒否。
- **クラッシュ系**: リモートから受け取った文字列のバイトスライス（`short_hex` / tx hash / path address）を文字単位に、Flare の `U256::to::<T>()` を `saturating_to` / `try_into` に、`ripple_path_find` のソートを `f64::total_cmp`（非有限値は末尾）に変更。panic hook は `process::exit(1)` のため、これらはすべてリモートからの入力で TUI を終了させ得た。
- **RPC 本文上限**: 16 MiB を `Content-Length` 事前検査に加えて chunk 読み込み中にも強制。
- **リモート文字列の制御・不可視文字**: `error_message` / `engine_result(_message)` / tx hash（`util::sanitize_remote`）と Server パネルの host / version（`sanitize_text`）から制御文字・bidi override・zero-width 書式文字を除去（TC-236）。
- **S-003**: 警告は `Config::new()`（`logging::init` より前）で出るため `tracing` ではなく `eprintln!`。
- **`install.sh`**: GitHub token を curl `-K -`(stdin) / wget `--config`(fd) 経由にして argv から除去。想定外文字を含む token は無視。curl は `--proto '=https' --proto-redir '=https'`（redirect 先も https のみ）、README のワンライナーも `--proto '=https' --tlsv1.2`。wget フォールバックは scheme 制限なし（`--https-only` は再帰時のみ有効）。
- **`cd.yml`**: `cargo package --locked`（token なし）で検証ビルドし、token 付きは `cargo publish --locked --no-verify`。

調査して**修正しなかった**もの（同一オペレーター内の操作 / ハードニング）: Ctrl-N 後のネットワーク表示と接続先のずれ、URL に入れた provider token のログ出力、dUNL 署名未検証（表示専用）、NFT デコードの寸法上限、CD の ref/environment 制約（GitHub 側 ruleset で担保）。二重送信は 2026-10-11 に対応した（TUI: 同一 params の write は結果 Ok/Err 到着まで拒否、TC-228）。config の keybindings からの Action 束縛は後日修正した。`config::is_bindable` の許可リスト（payload なしのナビ/リフレッシュ系のみ）外はロード時エラーになる（`*Submit` 等は束縛不可）。

## 2026-10-11 セキュリティ監査 run-2 対応

needs_validation 4 件（Core Vault ピン・simulate の Sequence/LLS 照合・quote 通貨照合・simulate 文字列 sanitize）は上の 2026-10-04 節の各項目を参照。以下はハードニング（テストは `docs/test.md` TC-204〜214）。

- **env 秘密の取得**: `XRPL_SEED` / `XRPL_MNEMONIC` / Flare EVM 鍵（`[flare.fassets] evm_key_env`）は `config::take_secret_env` が `Config::new()` で一度だけ取り出して env から削除し、`SecretString` で保持、生 `String` は zeroize。`lib.rs::run` は CLI 資格情報の適用直後に `Config::take_secrets()` で seed / mnemonic / EVM 鍵を `Config` から `Secrets` へ移し（`Config` 側は `None`）、`Arc<Config>` を共有する panel / status bar / splash には秘密が渡らない。`Secrets` は署名経路（`App` → poll task、`run_flare_command`）だけが受け取り、UI には `App::new` が一度だけ導出した address（`AccountWalletTab::new` → `WalletPanel`）だけが渡る。EVM 鍵は以前は初回 execute 時に poll task 内で `remove_var`（他タスクが env を読み得る実行中の変更、失敗すると以後使えない）だった。**Linux では `remove_var` は初期 `/proc/self/environ` を書き換えない**（同一 uid からは読める）。env 渡しは引き続き同一ユーザー内の信頼が前提。
- **JSON-RPC client は redirect を追わない**（`client.rs`、`Policy::none()`）。https endpoint が `submit`（署名済み blob）を http へ 307 で転送する経路を閉じた。system proxy はそのまま（運用者の選択）。faucet も redirect なし + 本文上限。
- **tx hash** は署名済み blob からローカル計算。TUI の送信結果に simulate 由来の署名済み Fee を表示（`<hash> · fee N drops`）。署名前の確認ステップは無い（Fee 上限 `MAX_NODE_FEE_DROPS` が担保）。
- **入力**: `XRP:100:tail` のような余分な区切りを拒否。CLI `send` も composer と同じ `validate_payment_input`（amount > 0 など）を RPC 前に実行。
- **鍵生成 popup / faucet**: `GeneratedWalletKeys` は drop 時に seed を zeroize、popup は Network 切替で消去（表示は「Esc to dismiss · g new key」）。`-x faucet` の生成 seed は `SecretString`。
- **CI**: 全 GitHub Actions をコミット SHA に固定（タグはコメント）、`cross@0.2.5` 固定。

## 2026-09-17 依存監査対応（`mise run audit` 整備で検知）

- **RUSTSEC-2026-0253: lru 0.16.4 `LruCache::pop()` unsound（use-after-free）**: `alloy-provider` → `lru` の実行時依存。0.16.x 系に修正版が無く、alloy 1.8.3（1.x 最終）も古い lru のままだったため **alloy 1.8.3 → 2.4.2 にメジャーアップ**（`Cargo.toml` `alloy = "2"`）。API 使用面は primitives + `ProviderBuilder` + `EthereumWallet` のみで変更不要。alloy 2.4.2 の MSRV に合わせ `rust-version` を 1.91 → 1.94.1 に変更した。**`rust-version` を 1.94.1 未満へ戻すと lru 0.16.4 が復活するので戻さない**。`cargo tree -i lru` = 単一 0.18.4。
- **RUSTSEC-2026-0285: rustls 0.23.44 TLS 1.3 handshake 暗号化レベル検証不備（2026-09-14 公表, Medium）**: `cargo update -p rustls` で **0.23.45** に更新（0.23.x 系内パッチ）。
- いずれも ignore 追加ではなく依存更新で解決。`mise run audit`（cargo-audit + cargo-deny）が CI の security-advisories と同一基準でローカル検知できるようになった初回実行で発見。
- **RUSTSEC-2026-0387: crypto-bigint 0.5.5 定数時間演算のタイミング変動**: `k256`/`ecdsa` 0.16（`xrpl-rust`・`kobe-xrpl`・`alloy-signer`）経由。修正版 0.7.6 は semver 非互換で `cargo update` では届かない。上流が `elliptic-curve` 0.14 系へ移るまで `deny.toml` と `.cargo/audit.toml` で ignore し、移行後に両方削除する（deny 側は `unused-ignored-advisory = "deny"` が古い ignore を検出する）。影響: `k256` 署名はユーザーのローカルで実行され、遠隔から計測できる経路はないが、共有ホストでは残存リスクとなる。

## 未対応・確認事項

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
| S-005 | `install.sh` で `NO_VERIFY=1` の警告が不十分 | `usage()` / ヘッダに加え、`NO_VERIFY=1` 実行時にも MITM リスクの赤字警告バナーを表示 |
| S-010 | `Config` が平文 `seed` を `Arc<Config>` として継続保持 | `RawSigningConfig` に `secret_seed: Option<SecretString>` を追加し、`Config::new()` で平文をクリア。さらに `lib.rs::run` が `Config::take_secrets()` で秘密を `Secrets` へ移してから `Arc<Config>` を共有 |
| S-011 | CLI `--seed` がプロセス引数に残り `ps` から閲覧可能 | `README.md` 警告 + 起動時 `eprintln!` 警告 + `--help` に非推奨注記 |
