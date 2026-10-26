# テスト戦略とレジストリ

[English](test.md)

**役割:** テスト設計・テストガイド（重要 TC カタログと FR / architecture へのトレース）。

> 最終更新: 2026-10-21（同一コントラクトのテストはテーブル駆動。lib スイートの件数はここでは管理しない — 現在の件数は `cargo test` で確認する）
> 対象: lazyxrp（XRPL 向け Rust TUI）
> カタログ: 重要ケース一覧（すべての `#[test]` との 1:1 索引ではない）

## 方針

- 本ファイルは**重要ケース一覧**である。TC を追加するのは、モジュール横断の契約、回帰しやすいロジック、ユーザーに見える挙動のみ。注釈のないヘルパー／単体テストは意図的にカタログ対象外とする。
- コミットメッセージで `TC-xxx` ID を参照する（例: `test(xrpl): add NFT parse case (TC-013)`）。
- インライン `#[cfg(test)]` モジュールを優先する。ライブネットワークのケースは `#[ignore]` とし、明示的に実行する。
- テストは lib（`src/lib.rs`）、`tests/cli_smoke.rs`（バイナリ境界）、`tests/testnet_e2e.rs`（オプトイン、ライブの testnet/devnet: send / offer / NFT mint → burn。`mise run e2e-testnet`、CI では実行しない）に置く。`Cargo.toml` の `[lib] doctest = false` と両 `[[bin]]` ターゲットの `test = false` は、空のテストターゲットをスキップして `cargo test` の再ビルドを速く保つ。単体テストは `main.rs` / `bin/rp.rs` ではなく lib モジュールに置くこと。置く場合は先に `test = false` を外す。
- **トレーサビリティ:** ユーザーに見える TC は `docs/requirements.md` の FR/NFR、または `docs/architecture.md` の § に対応づける。キー／レイアウトの TC は `DESIGN.md` にトレースする。ネットワーク／Flare の行は [トレーサビリティ](#トレーサビリティ) を参照。
- **バイリンガル文書:** `docs/*.md` と `ROADMAP.md` はすべて日本語版 `*-ja.md`（本ファイルは `test-ja.md`）を持つ。エージェントにとっては英語版が正本であり、片方を変更するときは同じ変更でもう片方も変更しなければならない。ID 集合（TC / FR / NFR / S / R / I / P / F）は一致させる。

## コマンド

```bash
cargo test                # all tests
cargo test <name>         # one case
# CI gates (.github/workflows/ci.yml):
cargo test --locked --all-features --workspace
cargo fmt --all --check
cargo clippy --locked --all-targets --all-features --workspace -- -D warnings
cargo doc --locked --no-deps --document-private-items --all-features --workspace --examples
cargo audit
cargo deny check
```

## レジストリ

| TC | P | 対象 | ケース |
|---|---|--------|------|
| TC-001-003 | P0 | `src/xrpl/parse.rs` | book_currency — XRP は大文字／大文字小文字非依存、issued は issuer 付き |
| TC-004-005 | P1 | `src/xrpl/util.rs` | json_str — ネストしたパスは値を返す。存在しないパスは空を返す |
| TC-006-007 | P1 | `src/xrpl/util.rs` | json_u32 — 数値／数値文字列は変換される。欠落または非数値はゼロを返す |
| TC-008-009 | P0 | `src/xrpl/format.rs` | drops_to_xrp — 基本変換。不正な文字列はゼロを返す |
| TC-010-012 | P1 | `src/xrpl/format.rs` | format_amount — None はダッシュ／XRP drops 文字列／issued currency オブジェクト |
| TC-013, TC-074 | P1 | `src/xrpl/parse.rs` | account_nfts — レスポンスを NftRow の Vec にパース。tfMutable フラグ（dNFT） |
| TC-014 | P1 | `src/xrpl/parse.rs` | account_lines — レスポンスを TrustLineRow の Vec にパース |
| TC-015 | P2 | `src/xrpl/parse.rs` | amm_info — 実際の rippled の形（`amount` / `amount2` / `lp_token` / `trading_fee`。資産は amount から暗黙的に決まる）を AmmSummary にパース |
| TC-016 | P2 | `src/xrpl/parse.rs` | account_tx — レスポンスを TxRow の Vec にパース（direction 付き） |
| TC-017 | P1 | `src/xrpl/parse.rs` | book_offers — レスポンスを OfferRow の Vec にパース |
| TC-018 | P2 | `src/xrpl/parse.rs` | server_info/fee — レスポンスをサマリー構造体にパース |
| TC-019-022 | P2 | `src/config.rs` | parse_style — 空はデフォルト、前景／背景のマッピング、修飾子の組み合わせ、gray==grey |
| TC-023 | P2 | `src/config.rs` | extract_color_and_modifiers — 修飾子と色を抽出する |
| TC-024-025 | P2 | `src/config.rs` | parse_color — RGB 短縮形。不明は None を返す。範囲外の `rgb`/`grayNN`/`bright colorNN` は None を返す（u8 オーバーフローなし） |
| TC-026 | P0 | `src/config.rs` | Config::new — デフォルトのキーバインドを読み込む |
| TC-027-029,031-032 | P1 | `src/config.rs` | parse_key_event — 修飾キーの組み合わせ、大文字小文字非依存、不正なキーはエラー |
| TC-033-035, TC-092 | P1 | `src/config.rs` | Config ソース優先順位 — ファイルがデフォルトを上書き、`XRPL_RPC_SERVER` 環境変数がファイルを上書き、XDG 解決、`~/.config` フォールバック |
| TC-036 | P2 | `src/config.rs` | Config 検証 — 不正なキーシーケンス形式 |
| TC-037 | P0 | `src/network.rs` | Network::default — Mainnet である |
| TC-038-040 | P0 | `src/network.rs` | Network::from_str — mainnet/testnet/devnet のラウンドトリップ、大文字小文字非依存、不明はエラー |
| TC-041 | P1 | `src/network.rs` | Network::is_production — 正しい真偽値 |
| TC-042 | P0 | `src/config.rs` | `Config::new` で `XRPL_NETWORK` がファイルの network を上書きする。パース不能／`" testnet"` の値は無視される |
| TC-043 | P0 | `src/lib.rs` | resolve_rpc_url/resolve_ws_url — 優先順位 CLI > 環境変数 > 設定ファイル > ネットワークのデフォルト |
| TC-044 | P1 | `src/lib.rs` | resolve_ws_url — CLI の --ws-server が最優先 |
| TC-045-046 | P1 | `src/signing.rs` | credential_from_secrets — none / seed / mnemonic / 両方指定は拒否、の結果マトリクス |
| TC-047 | P1 | `src/signing.rs` | prompt_production_confirmation — testnet/devnet ではスキップ |
| TC-048 | P2 | `src/signing.rs` | prompt_production_confirmation — --yes で mainnet でもスキップ |
| TC-049 | P1 | `src/config.rs` | Config::new — 環境変数 XRPL_SEED がファイルの seed を上書きする |
| TC-050 | P0 | `src/xrpl/cli_exec.rs` | CLI info — ローカル JSON-RPC で成功 |
| TC-051 | P1 | `src/xrpl/cli_exec.rs` | CLI account — ローカル JSON-RPC で成功 |
| TC-052 | P1 | `src/xrpl/cli_exec.rs` | CLI book — ローカル JSON-RPC で成功 |
| TC-053 | P1 | `src/xrpl/cli_exec.rs` | CLI summary — ローカル JSON-RPC で成功 |
| TC-054 | P1 | `src/xrpl/cli_exec.rs` | CLI nfts — ローカル JSON-RPC で成功 |
| TC-055 | P1 | `src/xrpl/cli_exec.rs` | CLI lines — ローカル JSON-RPC で成功 |
| TC-056 | P2 | `src/xrpl/cli_exec.rs` | CLI amm — ローカル JSON-RPC で成功 |
| TC-057 | P2 | `src/xrpl/cli_exec.rs` | CLI tx-history — ローカル JSON-RPC で成功 |
| TC-058 | P1 | `src/cli.rs` | CLI — 不正なパラメータはエラー。削除済みの `watch` サブコマンドはもうパースされない |
| TC-059 | P2 | `src/xrpl/cli_exec.rs` | CLI — 不正なクラシックアドレスは RPC 接続前に失敗する |
| TC-060 | P3 | `src/app.rs` | Watch モード — タブ巡回／終了／更新は TC-061–065 でカバー（旧 4 タブ構築テストは、コンストラクタのデフォルトを検証するだけだったため削除） |
| TC-061 | P1 | `src/app.rs` | Watch モード — `q` キーで `should_quit` が立つ |
| TC-062-063 | P2 | `src/app.rs` | Watch モード — `r`/`b` で Account/Book の PollCommand を送る。500 ms 以内の 2 回目の押下はデバウンスされる |
| TC-064 | P2 | `src/app.rs` | Watch モード — TabNext/TabPrev でパネルを巡回する |
| TC-065 | P3 | `src/app.rs` | Watch モード — HelpOverlay は `?` と Esc で切り替わる |
| TC-066 | P1 | `src/xrpl/cli_exec.rs` | CLI — ローカル JSON-RPC に対する `account-status` |
| TC-068 | P1 | `src/xrpl/client.rs` | XRPL RPC エラー — not found が気づかれないまま握りつぶされることはない |
| TC-069 | P1 | `src/xrpl/parse.rs` | XRPL submit レスポンス — `tesSUCCESS` と hash が必須 |
| TC-070 | P1 | `src/xrpl/types.rs` | book_offers — issued の quote は `currency_code` を使う |
| TC-071-072 | P1 | `src/xrpl/parse.rs` | account_objects — 空配列をパースできる。Check / Ticket / MPT / PayChannel / Escrow の混在 |
| TC-076 | P1 | `src/cli.rs` | CLI — `--self-uninstall` フラグをパースする。バックアップパスのヘルパー |
| TC-077-079 | P2 | `src/components/panels/tx_history.rs` | TxHistoryPanel — tx_type／hash の部分一致でフィルタ。空フィルタは全件表示 |
| TC-080-082 | P2 | `src/xrpl/parse.rs` | ripple_path_find — alternatives、空の alternatives、XRP drops 文字列としての `source_amount` |
| TC-083 | P2 | `src/xrpl/format.rs` | summarize_paths_computed — ホップ連鎖の省略表記 |
| TC-084 | P2 | `src/xrpl/format.rs` | path_find_rows_from — 表示行。安い順の並び |
| TC-085-086 | P2 | `src/xrpl/types.rs` | asset_display_name — 既知の hex コード。不明はそのまま通す |
| TC-087 | P2 | `src/xrpl/poll.rs` | Poll トリガー — 短時間の連続発火をまとめる |
| TC-088 | P0 | `src/xrpl/submit.rs` | Mainnet ガード: `--yes` なしではすべての submit 経路が拒否される（R-006）。テーブルに Smart Account instruction を含む |
| TC-089/TC-144 | P0 | `src/xrpl/parse.rs`, `src/xrpl/poll.rs` | account_tx 結果のマッピング — not-found → 空の履歴（I-7）。アクションは append フラグに従う（Ok/Err × append のマトリクス） |
| TC-093 | P1 | `src/components/shared/tx_detail/mod.rs` | TxDetail の描画キャッシュ — 開いたときに無効化される |
| TC-094 | P2 | `src/components/shared/tx_detail/parsers.rs` | TxDetail パーサーレジストリ — 必須タイプが揃い、重複なし |
| TC-096-097 | P2 | `src/xrpl/parse.rs` | get_aggregate_price — 完全なレスポンス。trimmed_set 省略時 |
| TC-098 | P1 | `src/signing.rs` | IOU 検証 — 不正な issuer／非有限値／ゼロ金額は拒否される |
| TC-099 | P1 | `src/signing.rs` | IOU 検証 — XRP／長すぎる通貨コードは拒否される。3 文字は OK |
| TC-100 | P2 | `src/xrpl/nft_image.rs` | NFT メタデータの走査は深さ上限で止まる |
| TC-101 | P1 | `src/lib.rs` | 署名用 seed は平文の RPC/WS エンドポイントを拒否する |
| TC-102 | P1 | `src/xrpl/client.rs` | レート制限された RPC エラーは再試行可能。恒久的な XRPL エラーは再試行しない |
| TC-103 | P2 | `src/xrpl/ws.rs`, `src/components/shared/fps.rs` | 重複した ledger-close は 1 回だけトリガーを出す。FPS は tick ではなく描画を数える |
| TC-227 | P1 | `src/xrpl/ws.rs` | ローカル WebSocket フィクスチャ（実際の rippled フレーム形状）: subscribe は account + `ledger` ストリームを含む。ledger close → `XrplLedgerClose` + poll トリガー 1 回（重複は破棄）。account tx → `XrplAccountTx`。サーバーによる close → 再接続 + 再 subscribe。cancel でタスクが止まる |
| TC-228 | P1 | `src/components/panels/wallet.rs` | 二重 submit ガード: 同一の書き込みパラメータは、その書き込みの Ok/Err が届くまで拒否される（flash）。パラメータが異なる場合、または結果後の再試行は通る |
| TC-229 | P2 | `src/xrpl/client.rs` | `account_offers` は `marker` が返らなくなるまでページをまたいで追跡する（ローカル JSON-RPC） |
| TC-230 | P1 | `src/xrpl/client.rs` | `simulate` リクエストに JSON null を含めない（xrpl-rust が作る Direct Mint の memo は `MemoFormat: null` を持ち、rippled に拒否される） |
| TC-231 | P1 | `src/flare.rs` | C3 `executeDirectMinting` のセレクタ = デプロイ済みの `0x78d0299e`（`IXRPPayment.Proof`）。実際の Coston2 DA の `{response, proof}` 応答がデコードできる（`{data, merkleProof}` エイリアスも同一にエンコードされる）。不正な形／旧 hex-`response` の形は拒否される |
| TC-232 | P1 | `src/xrpl/submit.rs` | AccountDelete（ローカル JSON-RPC）: X-address 宛先のタグが simulate と署名済み blob に届く。blob は基本手数料ではなく simulate が選んだ手数料（owner reserve）を持つ。`--yes` なしの Mainnet は拒否（TC-088 のテーブル） |
| TC-233 | P1 | `src/components/panels/wallet.rs` | AccountDelete コンポーザー: `s` は直接キューに積まない — 不正な宛先は flash、正しい宛先は `DELETE` の type-to-confirm を開く。Esc で宛先を保持。部分的な単語は拒否。`DELETE` + Enter でトリム済みの宛先をキューに積む |
| TC-234 | P1 | `src/lib.rs` | `--watch -x <write>`（send / offer-cancel / offer-create / amm-deposit / amm-withdraw / account-delete）は RPC の前に拒否される。read、faucet、TUI 経路は従来通り解決される |
| TC-235 | P1 | `src/lib.rs` | `enter_watch_mode` は `Config` から seed、mnemonic（平文 + secret）、Flare EVM キーを取り除き、キーのアドレスを返す（キーがなければ None） |
| TC-236 | P1 | `src/xrpl/util.rs`, `src/components/panels/server_detail.rs` | `sanitize_text`（TUI のサーバーパネル、バリデータ詳細ポップアップ — domain、TOML テキスト、attestation、エラー、Content-Type — および `sanitize_remote` の CLI/エラーテキスト）は制御文字、bidi オーバーライド（`U+202A–202E`、`U+2066–2069`、`U+061C`）、ゼロ幅の書式文字を除去する。通常の Unicode は残り、上限は文字数で数える |
| TC-237 | P1 | `src/xrpl/nft_image.rs` | リダイレクトのホップ https → http は拒否される（NFT 画像 + `xrp-ledger.toml`）。スキーム相対／相対のホップは https のまま。http → https は許可。SSRF テーブルに `::a.b.c.d`、NAT64、6to4、`fec0::/10`、`0.0.0.0/8` を追加（公開の NAT64 / 6to4 は引き続き許可） |
| TC-238 | P2 | `src/components/shared/tx_detail/mod.rs` | tx フィールド由来の Ripple epoch 時刻はオーバーフローせず飽和する。`u64::MAX` は `----` のプレースホルダーを描画する |
| TC-239 | P0 | `src/signing.rs` | Smart Account の payment reference（32 バイト）: id はバイト 0、walletId は 1、value は 2..12（10 バイト、ビッグエンディアン）、recipient は 12..32。リファレンスエンコーダのベクタ（transfer value 10 `0x01 00 …0a f548…131d`、value 1234567 `…12d687 f548…131d`、redeem 3 `0x02 00 …03` + ゼロ埋めの末尾）と、バイトが互いに異なるオフセットテストに固定。value 0 / ≥ 2^80 は拒否（2^80−1 は収まる）、recipient が不正／ゼロアドレスは拒否、大文字小文字 + 空白は正規化される。`check_smart_account_fee` は 1..=10 XRP を境界とする。`parse_token_amount`: 厳密な整数の 10 進数 → decimals 0 / 6 / 8 / 18 での最小単位。小数桁が多すぎる／decimals > 18／`.5`／`1.`／符号／指数／u128 オーバーフローは拒否される |
| TC-240 | P0 | `src/flare.rs` | `check_operator`: RPC の operator は `[flare.smart_accounts] expected_operator` と完全一致した場合のみ署名される。不一致／未設定／空白の pin は拒否（R-017） |
| TC-241 | P1 | `src/flare.rs` | ローカル JSON-RPC モックに対する `fetch_smart_account_info`: registry → controller / AssetManager、personal account、nonce、operators、両方の fee、FXRP 残高（最小単位）+ `decimals()`（6 / 8 / 0 が引き継がれる。19 → エラー）、`eth_getCode` → デプロイ済みフラグ。未デプロイ（`0x`）。範囲外の uint256 → エラー（panic しない）。不正な XRPL owner は RPC に到達しない。`format_token_units` は任意の decimals で厳密 |
| TC-242 | P0 | `src/xrpl/submit.rs` | ローカル JSON-RPC に対する Smart Account の transfer + redeem: `server_info → account_info → simulate → submit` の順。simulate と署名済み blob は pin 済み operator への Payment、`Amount` = instruction fee（厳密）、`DestinationTag` なし、Memo は 1 つで、その `MemoData` は 32 バイトの instruction（id、wallet 0、value、recipient / ゼロ）にデコードされる |
| TC-243 | P0 | `src/xrpl/submit.rs` | Smart Account の拒否はすべて RPC の前に起きる: pin が未設定／空白／不一致、fee が 0 または > 10 XRP、recipient がゼロ／不正、value が 0 / 10 バイト超、operator がアドレスでない、operator が X-address（タグ）、operator = 署名者。いずれも error アクションがちょうど 1 つ（R-017） |
| TC-244 | P1 | `src/components/panels/wallet.rs` | Smart Account コンポーザー: 不正／ゼロの recipient、ゼロ／空／残高超過の金額、非数値／ゼロの lots、別アカウントの read → flash のみ。正しい transfer / redeem は正確なパラメータをキューに積む（FXRP の 10 進数 → read した `decimals()` による最小単位、operator + fee は read から）。ピッカー `a`、`i` で transfer ↔ redeem を切り替え、入力 + `s`、Esc → ピッカー行 13。read がないとコンポーザーは開かない。描画は operator、pin の状態、fee、残高を表示する |
| TC-245 | P1 | `src/components/panels/fxrp_direct_mint.rs` | FXRP パネルは `SmartAccountInfo` の値（personal account、`fxrp_decimals` で整形した FXRP 残高、nonce + deployed / not deployed、operator、両方の fee）を描画する。リモート文字列中の制御文字は除去される。最初の `SmartAccountInfo` の前は行なし |
| TC-246 | P2 | `src/config.rs` | `[flare.smart_accounts] expected_operator` をパースする。デフォルトは未設定 |
| TC-247 | P0 | `src/components/panels/wallet.rs` | Smart Account コンポーザーは read した `decimals()` に従う（6 と仮定せず 8 と 0）: 8 decimals で `12.5` → 1_250_000_000 単位。残高ちょうどは受理、残高 + 1 単位は拒否。decimals より多い小数桁は拒否。decimals > 18、および read なしではキューに積まれない |
| TC-104 | P1 | `src/xrpl/parse.rs`, `src/components/shared/fps.rs` | account_tx の marker はパース後も保持される。tick-rate ラベルは 1 秒後に更新される |
| TC-105 | P2 | `src/xrpl/client.rs` | ローカル HTTP 429 は成功するまで再試行される |
| TC-248 | P0 | `src/coverage.rs` | XRPL カバレッジのずれ: `xrpl-coverage` マーカー間の ROADMAP.md の表が `rows()`（type、family、read、TUI、CLI、status）と一致し、README が書き込み可能／全体の `**N / M**` を記載する。レジストリが整合している（type の重複掲載なし、すべての Spec type に tx_detail パーサーがある、Existing の行が配線されている） |
| TC-249 | P0 | `src/xrpl/txspec/{mod,nft}.rs` | NFT type ごとのフィールド検証テーブル（accept / reject。TransferFee 50000 は OK / 50001 は拒否され、transferable が必要。URI 256 / 257 バイト（マルチバイト含む）。id 63 / 64 / 65 hex。taxon は u32 最大。500 / 501 offers。sell / buy の Owner ルール。BrokerFee は両方の offer が必要。不明なフィールド／フラグ）。レジストリが正しく構成されている。CLI の `Field=value` パース。相対 `Time` は `prepare` で 1 回だけ解決される |
| TC-250 | P0 | `src/xrpl/txspec/nft.rs` | NFT type ごとの未署名 `tx_json` の形（hex URI、flags ビット、Amount は drops / IOU オブジェクト、Account / Sequence） |
| TC-251 | P0 | `src/xrpl/txspec/nft.rs` | NFT type ごとの署名済み blob が、互いに異なる Sequence 41 / Fee 13 / LastLedgerSequence 987654 でデコードされる（引数を入れ替えると失敗する）。ビルダーは crate のモデルルールを適用する |
| TC-252 | P0 | `src/xrpl/submit.rs` | ローカル RPC モックに対する汎用 submit: メソッド順 `server_info` → `account_info` → `simulate` → `submit`。署名された Sequence 7 / Fee 15 / LLS 80,000,020 は simulate 由来。相対 Expiration は 1 回だけ固定される |
| TC-253 | P0 | `src/xrpl/submit.rs` | 汎用 submit は `tec*` の simulate で止まる: `submit` 呼び出しなし、Err アクション（`tesSUCCESS` ガードを外すと失敗する） |
| TC-254 | P0 | `src/xrpl/cli_exec.rs` | `-x tx` / `nft-mint` / `nft-burn`: simulate → 確認 → submit、`--no-wait`、拒否（不正なフィールド、不明な type、`--yes` なしの mainnet）は RPC に到達しない、simulate での停止 |
| TC-255 | P0 | `src/components/panels/wallet.rs` | トランザクションコンポーザー: フィールド仕様から生成されるフォーム、インライン検証、キューに積まれる `GenericTxSubmit` は 1 つのみ。burn は `BURN` の入力が必要（部分入力は拒否、Esc でフォームを復元）で、プリフィル済みで開く。ヘルプ／インラインエラーが描画される |
| TC-256 | P1 | `src/app.rs`, `src/components/tabs/nft.rs` | Assets の `x` は選択中の NFTokenID で burn コンポーザーを開く（他のタブでは開かない。未選択なら何もしない） |
| TC-260 | P0 | `src/xrpl/txspec/checks_misc.rs` | CheckCreate / CheckCash / CheckCancel / TicketCreate / DepositPreauth / SignerListSet / Clawback のフィールド検証テーブル: TicketCount 0 / 1 / 250 / 251、id 63 / 64 / 65、exactly-one ルール、signers 8 / 9、weight 0 / 65535 / 65536、quorum と総 weight、delete 形式、XRP の clawback。typed 確認フレーズは CheckCash / SignerListSet / Clawback のみ |
| TC-261 | P0 | `src/xrpl/txspec/checks_misc.rs` | type ごとの未署名 `tx_json` の形（id は大文字化、ネストした `SignerEntry` オブジェクト、delete 形式は `SignerEntries` を省略、`Fee` / `LastLedgerSequence` なし） |
| TC-262 | P0 | `src/xrpl/txspec/checks_misc.rs` | type ごとの署名済み blob が、互いに異なる Sequence 41 / Fee 13 / LastLedgerSequence 987654 でデコードされる。ビルダーはモデルルールを適用する（署名者を含む signer list、自己 clawback） |
| TC-263 | P0 | `src/xrpl/txspec/checks_misc.rs` | ローカル RPC モックに対する TicketCreate の汎用 submit: 不正なフォームは RPC に到達しない。`server_info` → `account_info` → `simulate` → `submit`、blob には simulate が選んだ Sequence / Fee / LLS が入る |
| TC-280 | P0 | `src/xrpl/txspec/channel_amm.rs` | PaymentChannelCreate / Fund の検証: drops 1 / 1e17 / 1e17+1 / 0 / 小数 / `XRP:` プレフィックス、SettleDelay は u32 最大 / +1、PublicKey の形（66 hex、`ED` / `02` / `03` のみ）、channel id 63 / 64、`CancelAfter` Time、必須フィールド、不明なフィールド |
| TC-281 | P0 | `src/xrpl/txspec/channel_amm.rs` | PaymentChannelClaim のフィールド間ルール: Signature には Amount / Balance / PublicKey が必要、Balance ≤ Amount、renew と close は排他、close / renew でなければ Balance 必須、signature の長さ 144 / 146、奇数長／非 hex は拒否 |
| TC-282 | P0 | `src/xrpl/txspec/channel_amm.rs` | AMMCreate / Vote / Bid / Delete の検証: TradingFee 1000 / 1001、同一の amount／asset は拒否、asset の文法（`XRP` / `CUR:issuer`）、BidMin ≤ BidMax かつ同じ LP トークン、XRP の bid は拒否、AuthAccounts 4 / 5 / 重複 / 不正なアドレス |
| TC-283 | P0 | `src/xrpl/txspec/channel_amm.rs` | type ごとの未署名 `tx_json`（drops 文字列、id / key / signature は大文字化、claim のフラグビット、`Asset` オブジェクト、ネストした `AuthAccounts`、任意フィールドは省略、`Fee` / `LastLedgerSequence` なし） |
| TC-284 | P0 | `src/xrpl/txspec/channel_amm.rs` | type ごとの署名済み blob が、互いに異なる Sequence 41 / Fee 13 / LastLedgerSequence 987654 と型付きフィールドでデコードされる。送信者自身を AuthAccount に指定するとビルド時に拒否される |
| TC-285 | P0 | `src/xrpl/txspec/channel_amm.rs` | typed 確認フレーズ `LOCK` / `FUND` / `CLAIM` / `CREATE` / `BID` / `DELETE`。AMMVote にはなし |
| TC-286 | P0 | `src/xrpl/txspec/channel_amm.rs` | ローカル RPC モックに対する PaymentChannelCreate + AMMDelete の汎用 submit: 不正なフォームは RPC に到達しない。`server_info` → `account_info` → `simulate` → `submit`。相対 `CancelAfter` は 1 回だけ固定される（simulate = blob）。blob には simulate が選んだ Sequence / Fee / LLS が入る |
| TC-320 | P0 | `src/xrpl/txspec/xchain.rs` | フラットな `XChainBridge` 文法（`lockDoor,lockIssue,issueDoor,issueIssue`）: 4 要素、チェックサム付きで互いに異なる door、`XRP` / `CUR:issuer` の issue、通貨をまたぐペアリングは拒否 |
| TC-321 | P0 | `src/xrpl/txspec/xchain.rs` | 6 つの XChain type のフィールド検証テーブル: drops 0 / max / max+1、claim id は hex 1..=16 桁かつ非ゼロ、必須フィールド、XRP のみの最小値、modify は変更が必要、Amount は bridge の issue でなければならない、account-create は XRP-XRP が必要 |
| TC-322 | P0 | `src/xrpl/txspec/xchain.rs` | XChain type ごとの未署名 `tx_json`（bridge オブジェクト、drops 文字列、modify のフラグ `0x10000`、claim id は大文字化、`Fee` / `LastLedgerSequence` なし。IOU の bridge issue は `{currency, issuer}`） |
| TC-323 | P0 | `src/xrpl/txspec/xchain.rs` | XChain type ごとの署名済み blob が、互いに異なる Sequence 41 / Fee 13 / LastLedgerSequence 987654 と bridge オブジェクトでデコードされる |
| TC-324 | P1 | `src/xrpl/txspec/xchain.rs` | ビルダーは crate のモデルルールを適用する: CreateBridge / ModifyBridge の署名者は door アカウントでなければならない |
| TC-325 | P0 | `src/xrpl/txspec/xchain.rs` | typed 確認フレーズ: `BRIDGE`（CreateBridge）、`COMMIT`（XChainCommit / AccountCreateCommit）、`CLAIM`（XChainClaim）。Modify / CreateClaimID にはなし |
| TC-329 | P1 | `src/components/shared/tx_detail/parsers.rs`, `src/xrpl/txspec/xchain.rs` | 6 つすべての XChain type に対する Tx History 詳細パーサー（カバレッジ "read"）。bridge は claim id とともにフラットな `lockDoor,lockIssue,issueDoor,issueIssue` 形式で描画される |
| TC-326 | P2 | `src/xrpl/txspec/xchain.rs` | `-x tx` の `Field=value` パースは `XChainBridge` / `XChainClaimID` を大文字小文字を区別せず解決する |
| TC-327 | P0 | `src/xrpl/txspec/xchain.rs` | ローカル RPC モックに対する XChainCommit の汎用 submit: 不正な bridge は RPC に到達しない。`server_info` → `account_info` → `simulate` → `submit`、bridge と正規化済み claim id が simulate リクエストと署名済み blob に入る |
| TC-328 | P1 | `src/xrpl/txspec/xchain.rs` | IOU bridge（issued currency の issue）が署名・デコードできる |
| TC-330 | P0 | `src/flare.rs` | FXRP C2 の金額は `max(min fee, fee_bips share) + executor fee` を超える必要がある: 最小手数料未満（Flare 上では支払い全額が fee receiver に渡る）、ちょうど境界 → 拒否、1 単位上 → 受理、大口支払いでは割合分が支配的 |
| TC-331 | P0 | `src/flare.rs` | FXRP C3 のレシートは AssetManager イベントのみで分類: `DirectMintingExecuted` → minted、`DirectMintingDelayed` / `LargeDirectMintingDelayed` → delayed（`executionAllowedAt`）、`DirectMintingPaymentTooSmallForFee` → flagged。別アドレスからの同イベントやイベントなし → unknown |
| TC-332 | P1 | `src/components/panels/fxrp_direct_mint.rs` | FXRP パネルが `MINT PAUSED` / `EMERGENCY PAUSE Ln` を表示（full / compact）。状態が不明または解除済みなら何も表示しない |
| TC-333 | P0 | `src/components/panels/wallet.rs` | FXRP Mint コンポーザは読み取った手数料ビューを載せたパラメータをキューに積む。`mintingPaused` またはいずれかの emergency レベルではキューを拒否 |
| TC-334 | P0 | `src/flare.rs` | Mint 上限のリプレイと分類: 古いウィンドウは経過した完全なウィンドウ数分だけ減少（繰り越しなし）、余裕は上限の 1 granule 下、アンダーフローしない。cap チェックが最初（`CapExceeded`）、large 閾値ちょうどの mint は delayed（境界含む）でウィンドウをスキップ。Live（`#[ignore]`）: mainnet の `fetch_fxrp_direct_mint_info` が pause と上限をデコード（cap 170M XRP、large 閾値 4M XRP / 2 h がドキュメントと一致）。2026-10-11 の Coston2 スモークレシートが宣言済みイベント ABI で `Minted`（4.8 FXRP）にデコードされる |
| TC-335 | P0 | `src/components/panels/wallet.rs` | FXRP Mint コンポーザは cap の余裕を超える金額を拒否（XRP が Core Vault で待機することになるため）。ウィンドウ超過（delayed）の金額は引き続きキューに積む |
| TC-336 | P2 | `src/components/panels/fxrp_direct_mint.rs` | FXRP パネルの `Max now`（ウィンドウ・large 閾値・cap 余裕の最小値）と `Cap used` 行。上限なし → 行なし |
| TC-337 | P1 | `src/flare.rs` | Redemption キューの要約（合計、完全な lot、エージェント別の順序、飽和加算、lot サイズなし → null）と `getSettings()` のワードピッカー（フィールド index、短い応答、想定外のオフセット）。Live スモーク: mainnet で `lazyxrp -x fxrp --tag N` が 4 × 20 チケットをページングして `next = 0` に到達（80 チケット、1,541,120 XRP）。tag 1 → `reserved: false`（コントラクトは `ERC721: invalid token ID` で revert）、tag 30 → `reserved: true` で owner / recipient が生の `eth_call` と一致、`pending_executor_change: null`。revert のみが未予約を意味し、トランスポートエラーは伝播する。レポートは Core Vault 保有量も表示: Core Vault + キューが `totalSupply` と約 7 XRP 以内で一致 |
| TC-338 | P2 | `src/components/panels/flare_ftso.rs` | FTSO フィード経過時間ラベル（`s` / `m` / `h`）と stale フラグ: 60 s は fresh、61 s は stale、現在より未来のタイムスタンプは `0s`、タイムスタンプ 0 = `never`（stale） |
| TC-339 | P1 | `src/flare.rs`, `src/components/panels/fxrp_direct_mint.rs` | Smart Account の vault ポジション: `fetch_smart_account_info`（ローカル JSON-RPC モック、TC-241 を拡張）が `getVaults()` の vault をシェア残高と ERC-4626 `convertToAssets` 付きで列挙し、シェア 0 の vault はスキップ、読み取りに失敗した vault もスキップ。FXRP パネルが `Vault N <Firelight / Upshift> <assets> FXRP` を描画。Live（`#[ignore]`）: mainnet の `getVaults()` が 3 vault（type 1, 2, 2）をデコードし、残高 0 のアカウントにはポジションなし。併せて、FTSO の live テストは全フィードのタイムスタンプが現在から 120 s 以内であることをアサート |
| TC-340 | P0 | `src/fdc.rs` | FDC 証明の取得: verifier の `abiEncodedRequest` が要求した tx / proof owner / `XRPPayment` type / source id と一致する必要がある（別 tx、別 owner、誤った type や source、誤った長さは拒否）。voting-round の算術、ネットワーク別のエンドポイント既定値、https のみの上書き、Songbird なし。Live（`#[ignore]`、`FDC_LIVE_TX`）: mainnet の registry 名、`firstVotingRoundStartTs` / 90 s epoch、finalize 済みの過去ラウンド、実在する Core Vault 支払いに対する verifier の回答、`getRequestFee` = 20 FLR |
| TC-341 | P0 | `src/flare_cmd.rs` | Redeem の検証（ゼロ、`minimumRedeemAmountUBA` 未満、残高超過、tag 幅）と tag アドレスのガード（ゼロ / 自身）。資金なしの使い捨てキーによる mainnet での CLI スモーク: `execute` なし → 拒否、`--yes` なし → 拒否、redeem 5 XRP → "exceeds the FXRP balance"、1 XRP → 最小値（5 XRP）未満、`fxrp-tag-reserve` → FLR が 100 FLR の手数料未満、`fxrp-tag-set 30` → 別アドレスが所有 |
| TC-342 | P0 | `src/flare_cmd.rs` | Vault 命令のレイアウト `[id \| wallet \| value(10) \| 2 ignored \| vaultId \| 16 ignored]`。`0x11–0x13` / `0x21–0x23` のみエンコードし、値ゼロは拒否。CLI スモーク: `sa-vault upshift deposit 1 …` → "vault 1 is type 1 … not 2"、`sa-vault firelight deposit 1 1000000` は memo と 0.5 XRP の手数料を表示し、mainnet の `--yes` ゲートで停止 |
| TC-343 | P0 | `src/flare.rs` / `src/lib.rs` | `check_network_pair`（`flare.rs` 内。TUI の C2 / Smart Account / C3 にも適用、TC-348 参照）（XRPL Mainnet ↔ Flare、Testnet ↔ Coston2。Songbird / Devnet / Xahau / 交差ペアは拒否）と `check_xrpl_tx_hash`（64 桁の hex、`0x` は任意。短い / 長い / 非 hex は拒否）。execute / key / `--yes` ゲートより前に実行される |
| TC-344 | P0 | `src/flare_cmd.rs` | upstream の `MemoInstructions.sol` に基づく memo 復元レイアウト: `0xE0` / `0xE1` 42 B、`0xE2` 50 B、`0xD0` 30 B、`0xD1` 10 B（`[op \| walletId 0 \| fee u64 BE \| body]`）。`0xE1` は `newNonce <= current` と `u32::MAX` を超えるジャンプを拒否（境界をテスト）、`--target` の欠落 / 不足、executor ゼロも拒否。op ごとの target 状態（`0xE0` / `0xE2` は `isTransactionIdUsed` が false の間のみ、`0xE1` は true になってからのみ。`check_target_state`） |
| TC-345 | P0 | `src/flare_cmd.rs` | `0xFE` ユーザー操作: `abi.encode(PackedUserOperation)` が同じ sender / nonce / `executeUserOp(Call[])` にデコードされ、memo は `[0xFE \| 0 \| fee u64 BE \| keccak256(userOp)]`（42 B）。call なし / 不正な target / 不正な hex は拒否。Coston2 live: `sa-call` → 支払いを検証 → `fxrp-execute --data <userOp>` → personal account の `FXRP.transfer` が実行された（recipient 5 → 6 FXRP、`getNonce` 3 → 4） |
| TC-346 | P0 | `src/flare.rs` | 書き込みのチェーンゲート: Flare ネットワークごとの `expected_chain_id`。`check_write_chain` は**報告された** `eth_chainId` から mainnet の `--yes` 要否を判定（不一致は拒否）。`tx_fee_cap` — `send_capped` は gas × max fee が `MAX_TX_FEE_WEI`（10 FLR）を超えると拒否。誤ったチェーンでの `fxrp-execute` は他のどのステップより前に拒否される |
| TC-347 | P0 | `src/signing.rs` | `eth_address_checksum_enforced_for_mixed_case`: 全小文字 / 全大文字は受理、EIP-55 チェックサムが誤った大小混在は拒否 |
| TC-348 | P0 | `src/xrpl/submit.rs` | `xrpl_writes_refuse_unpaired_flare_network`、`fxrp_execute_direct_mint_refuses_unpaired_networks`、`fxrp_execute_direct_mint_flare_mainnet_without_yes_is_rejected`: XRPL の書き込み（C2 / Smart Account / C3）は XRPL ↔ Flare の交差ペアと、`--yes` なしの Flare mainnet を拒否 |
| TC-349 | P0 | `src/lib.rs` | `direct_mint_payment_refused_when_paused_or_over_cap`: mint が pause 中、または金額が cap の余裕を超える場合、C2 Payment を拒否 |
| TC-350 | P0 | `src/xrpl/poll.rs` | `stale_write_refusal_on_network_switch`: ネットワーク A で組み立てた書き込みは、アクティブネットワークが変わった後は（送信せず）拒否される（すべての書き込みパラメータに組み立て時の `network` を含む） |
| TC-351 | P0 | `src/xrpl/client.rs` | `submit_signed_tx` は単一試行: submit の応答を失った場合は `tx(hash)` の照会で解決（適用成功 / 適用失敗 / 未検出 = 明示的な "unknown"）し、blob は決して再送しない（S-014） |
| TC-352 | P1 | `src/xrpl/client.rs` | `network_id_is_set_only_above_1024`: 保存された `network_id` が 1024 超の場合のみ、`NetworkID` を tx に注入（simulate と署名）。`NetworkID` 付き `simulate` を Xahau の live が受理するかは**未検証**（problems.md P-007 参照） |
| TC-353 | P1 | `src/xrpl/parse.rs` | `parse_submit_success_treats_terqueued_as_queued`: `terQUEUED` はキュー投入成功として扱う |
| TC-354 | P1 | `src/xrpl/ws.rs` | `ws_silent_socket_times_out_and_reconnects`: 30 s の読み取りタイムアウト（`cfg(test)` では短縮）で無応答ソケットを切断して再接続 |
| TC-355 | P1 | `src/xrpl/toml.rs` | ドメイン検証は base58 の `nH…` ノードキーと hex を受理し、デコード後のバイト列を比較 |
| TC-356 | P1 | `src/xrpl/dunl.rs` | dUNL VL のデコード: `field_header_matches_spec_vectors`（1 / 2 バイトのフィールド id）と `vl_length_matches_spec_boundaries`（1 / 2 / 3 バイト長） |
| TC-357 | P1 | `src/xrpl/txspec/mod.rs` | `parse_drops_is_strict`: 数字のみ、符号 / 空白 / 小数は不可、オーバーフローは拒否 |
| TC-358 | P1 | `src/xrpl/nft_image.rs` | `client_for_url_handles_ipv6_literals`: IPv6 リテラルの NFT URI が正しく解決 / ピン留めされる |
| TC-359 | P0 | `src/config.rs` | `keybinding_rejects_payload_and_submit_actions`（`is_bindable` による許可リストのアクションのみ。ロード時に拒否）、`tilde_paths_expand_to_home`（`config_dir` / `data_dir` / `LAZYXRP_*` 内の `~`）、`credential_precedence_matrix`（CLI > env > file。下位ソースのもう一方の種別は破棄。同一レベルで両方 = エラー） |
| TC-360 | P0 | `src/app.rs` | アクティブタブのみへのルーティング: `overview_t_key_does_not_open_hidden_composer`、`account_tab_filter_key_reaches_tx_history`、`composer_open_without_credential_stays_on_tab`。`startup_ends_when_server_info_errors`（`server_info` エラーでスプラッシュが終了） |
| TC-361 | P0 | `src/components/panels/wallet.rs` | フォームの安全性: `trust_set_default_form_cannot_submit`（TrustSet は空で開始）、`account_set_no_freeze_requires_confirm`（`NOFREEZE` の入力が必要）、`submit_guard_survives_local_errors_and_stale_results`、`stale_or_foreign_network_snapshots_refuse_to_submit`（300 s 超または別ネットワークのスナップショットは拒否）、`paste_inserts_only_while_editing_without_hotkeys`（bracketed paste は編集中のみ） |
| TC-362 | P2 | `src/components/panels/wallet.rs` / `wallet_keygen.rs` / `tabs/account_wallet.rs` | `keygen_works_unfunded_and_composer_is_gated`、`composer_popup_narrow_terminal_does_not_panic`、`keygen_popup_narrow_terminal_does_not_panic`、`keys_reach_focused_tx_history_pane` |
| TC-365 | P1 | `src/components/panels/tx_history.rs` | `paste_goes_to_filter_only_while_editing`: bracketed paste は Tx History のフィルタを編集している間のみフィルタに追記され（制御文字は除去）、それ以外は無視される |
| TC-363 | P0 | `src/flare.rs` | `direct_mint_terms_drift_is_refused`、`smart_account_terms_drift_is_refused`: 組み立てから署名までの間に手数料の引き上げ、Core Vault の変更、pause、operator の変更、命令手数料の変更があれば拒否 |
| TC-366 | P1 | `src/flare_cmd.rs`, `src/cli.rs` | `wait_finalized_polls_until_true_then_gives_up`: finalize 待ちはチェックが true になり次第返り、上限を過ぎると（エラーではなく）`false` を返し、読み取り失敗はエラーとして表面化する（`await_public_executor` はこれを stderr のヒントと exit 0 に変換）。`spec_subcommands_parse`: `sa-status` は読み取り専用（`--watch` 下でも拒否されない）、`sa-memo` は署名する。Live スモーク（読み取り専用、2026-10-24）: mainnet の `lazyxrp --watch -x sa-status <tx>` → `DirectMintingExecutedToSmartAccount` から得た最近の Smart Account mint id は `finalized`、未使用のハッシュは `not finalized`。Coston2 の `sa-status FB8556…`（docs/test.md "FXRP C2 + C3" の通常 C2 mint で、公開 executor が finalize 済み）→ `not finalized`、つまりこのビューは通常の C2 mint を対象としない |
| TC-367 | P0 | `src/components/panels/wallet.rs` | `fxrp_mint_composer_prefills_own_personal_account_only`: FXRP Mint コンポーザは署名者自身の personal account をプリフィルし、"your Smart Account personal account" の注記付きで全桁を表示する。別の XRPL アカウントの読み取り、または 300 s より古い読み取りの場合は recipient を空のままにする（R-019 / S-019） |
| TC-300 | P0 | `src/xrpl/txspec/identity.rs` | Identity ファミリーのフィールド検証テーブル: CredentialType 64/65 バイト、URI 128/129、DID フィールド 256/257、少なくとも 1 つ必要なルール、Oracle の id / provider / asset class / series 10/11、domain id（長さ、hex、ゼロハッシュ）、AcceptedCredentials 10/11 |
| TC-301 | P0 | `src/xrpl/txspec/mpt.rs` | MPT ファミリーのフィールド検証テーブル: TransferFee 50000/50001、AssetScale 255/256、MaximumAmount 0 / 1 / i64::MAX / +1、metadata 1024/1025、id 47/48/49、transfer-fee / domain / lock-unlock の組み合わせ |
| TC-302 | P0 | `src/xrpl/txspec/identity.rs` | identity 型ごとの未署名 `tx_json`: テキストフィールドは hex エンコード、`PriceDataSeries` → `PriceData` エントリ（10 進価格 → hex、scale）、`AcceptedCredentials` → `Credential` エントリ、id は大文字化、Fee / LastLedgerSequence なし |
| TC-303 | P0 | `src/xrpl/txspec/mpt.rs` | MPT 型ごとの未署名 `tx_json`: フラグビット、metadata の hex、id は大文字化、未対応のオプションフィールドは含まない |
| TC-304 | P0 | `src/xrpl/txspec/identity.rs` | identity 全 9 型の署名済み blob が、それぞれ異なる Sequence / Fee / LastLedgerSequence と署名付きでデコードされる |
| TC-305 | P0 | `src/xrpl/txspec/mpt.rs` | MPT 全 4 型の署名済み blob が、それぞれ異なる Sequence / Fee / LastLedgerSequence と署名付きでデコードされる |
| TC-306 | P0 | `src/xrpl/txspec/identity.rs`, `src/xrpl/txspec/mpt.rs` | 入力式の確認フレーズ: CredentialDelete / DIDDelete / OracleDelete / PermissionedDomainDelete は `DELETE`、MPTokenIssuanceDestroy は `DESTROY`。他の型にはなし |
| TC-307 | P1 | `src/xrpl/txspec/identity.rs`, `src/xrpl/txspec/mpt.rs` | ビルダーは crate のモデルルールを適用: series のない OracleSet は拒否。MPTokenIssuanceSet / MPTokenAuthorize の `Holder` = 署名者は拒否 |
| TC-308 | P1 | `src/xrpl/txspec/identity.rs` | フラットな文法: `BASE/QUOTE=price@scale`（delete 形式、u64 境界、scale 20/21、重複 / base = quote）と `rIssuer:Type` の credential（hex エンコードした type、重複、不正な issuer、サイズ） |
| TC-309 | P0 | `src/xrpl/txspec/identity.rs` | ローカル RPC モックに対する DIDSet の汎用 submit: 空のフォームは RPC に到達しない。`server_info` → `account_info` → `simulate` → `submit`、simulate リクエストと署名済み blob 内の URI は hex エンコード |
| TC-310 | P0 | `src/xrpl/txspec/mpt.rs` | ローカル RPC モックに対する MPTokenIssuanceCreate の汎用 submit: canTransfer なしの transfer fee は RPC に到達しない。simulate リクエストと署名済み blob に flags / metadata / Sequence / Fee / LastLedgerSequence を含む |
| TC-088 / 190 / 234 | P0 | （拡張） | TC-088 の mainnet ゲートテーブルに `generic_nft_mint` / `generic_nft_burn` を追加。TC-190 `spec_subcommands_parse`。TC-234 `--watch` は `tx` / `nft-mint` / `nft-burn` を拒否し、`coverage` は引き続き許可 |
| TC-106 | P1 | `src/xrpl/submit.rs`, `src/app.rs` | submit 失敗時にエラーとアカウント再同期を発行。ユーザー操作は dirty にマーク |
| TC-107 | P1 | `src/xrpl/submit.rs`, `src/app.rs` | クローズ済みのアクションチャネルはパニックせず失敗を返す。tick で再描画 |
| TC-108 | P1 | `src/app.rs` | Dirty 描画 — `run()` は `needs_draw` のとき、ループ 1 回につき 1 度描画する（`draw_frame`: dirty をクリアし、FPS にカウント）。自動テストなし（実端末が必要）: PTY 経由のスモーク — リサイズなしでスプラッシュとメイン UI が表示されること |
| TC-109 | P2 | `src/components/tabs/nft.rs` | URI を持つ NFT は `NftImageRequest` を発行 |
| TC-110 | P2 | `src/components/tabs/nft.rs` | URI のない NFT はプレビューリクエストを発行しない |
| TC-111 | P1 | `src/network.rs` | すべてのネットワークが、それぞれ異なる `https://`/`wss://` エンドポイントに解決される（xahau を含む） |
| TC-112 | P2 | `src/network.rs` | xahau の各バリアントを含む全ネットワークの serde ラウンドトリップ |
| TC-113-114 | P2 | `src/config.rs` | `[flare]` の `network`/`display` が設定ファイルから読み込まれ（coston2 プリセット URL）、既定は Flare/Full |
| TC-115 | P1 | `src/app.rs` | NetworkSwitchCycle が `app.network`、net_tx、StatusBar バッジのパディングを更新する |
| TC-116 | P2 | `src/components/tabs/overview.rs` | Overview Off は Flare 統合パネルを省略する |
| TC-117 | P2 | `src/components/tabs/overview.rs` | Overview Compact は FTSO/FXRP を 1 行サマリで表示する(44/56 分割は維持) |
| TC-118 | P2 | `src/components/tabs/market_oracle.rs` | Market Off は FTSO パネルを省略し、XRPL oracle は残す |
| TC-119 | P2 | `src/components/tabs/market_oracle.rs` | Market Compact は FTSO を 1 行サマリで表示する |
| TC-120 | P2 | `src/config.rs` | `[flare.wallet] address` の EVM 検証と正規化 |
| TC-121 | P2 | `src/components/panels/flare_wallet.rs` | 未設定のウォレットはセットアップ案内を表示する |
| TC-122 | P2 | `src/components/panels/flare_wallet.rs` | 設定済みウォレットは `FlareWalletBalance` から残高行を描画する |
| TC-123 | P2 | `src/xrpl/dunl.rs` | dUNL キャッシュ — TTL 内はヒット、クリア後はミス |
| TC-124 | P2 | `src/flare.rs` | Flare レジストリキャッシュ — アドレスは RPC URL ごとに保存される |
| TC-125 | P1 | `src/cli.rs` | CLI `--network` は `xahau` / `xahau-test` を受け付ける(clap デフォルトの `xahau-testnet` は拒否) |
| TC-126 | P1 | `src/cli.rs` | `--allow-insecure-rpc` はデフォルト off。フラグで TUI と `rp` が有効になる |
| TC-127 | P1 | `src/config.rs` | 設定ファイルの `network = "xahau-test"` は XahauTest として読み込まれる(TC-042 のテストに統合) |
| TC-128 | P2 | `src/components/shared/theme.rs` | DESIGN.md frontmatter の `colors.*` の hex 値が `theme.rs` の定数と一致する |
| TC-129 | P1 | `src/components/panels/wallet.rs` | Fxrp proof プレビューはマルチバイト JSON を文字単位で切り詰める(バイトスライスによる panic の回帰) |
| TC-130 | P1 | `src/components/panels/wallet_keys.rs` | `payment_edit_keys` の入力フィルタ — IOU 通貨行は 40 文字以下(3 文字コードまたは 40 桁 hex)、金額は数字と単一のドットのみ、宛先は表示可能 ASCII |
| TC-131 | P1 | `src/components/panels/wallet_keys.rs` | `account_set_edit_keys` — 行 0/1 はロック、domain は自由テキスト、tick/transfer は数字のみ |
| TC-132 | P2 | `src/components/panels/wallet_composer.rs` | Composer の行移動 — `[`/`]` は循環、編集中の Enter は次へ進む、`e` で編集を切り替える |
| TC-133 | P1 | `src/components/shared/selectable_table.rs` | 選択は `reset_len` でクランプされ、len 0 で選択解除され、端で止まる |
| TC-134 | P1 | `src/components/shared/tx_detail/mod.rs` | オーバーレイ表示中は Select キーを消費(スクロール、飽和)して true を返す。非表示なら false を返す |
| TC-135 | P2 | `src/components/shared/tx_detail/mod.rs` | `handle_panel_action` — TxDetailToggle はオーバーレイを開く。不明な tx または非表示なら false |
| TC-136 | P2 | `src/components/shared/status_bar.rs` | `freshness_label` の境界値 — 0s/59s/1m/59m/1h。None は `-` のパスを維持する |
| TC-137 | P1 | `src/xrpl/parse.rs` | `book_offer_best_price` — 文字列の quality、反転、非数値 → None、offers 欠落 → None |
| TC-139 | P2 | `src/xrpl/format.rs` | `format_ripple_time_utc` — 既知の値、epoch ゼロ、u64::MAX のクランプが panic しない |
| TC-140 | P2 | `src/xrpl/parse.rs` | 非数値の quality 行は `price = "-"` で描画され、数値行の描画には影響しない |
| TC-141 | P2 | `src/xrpl/format.rs` | `xrp_to_drops` の小数部最大境界 → u64::MAX。オーバーフロー → Err |
| TC-142 | P2 | `src/xrpl/toml.rs` | 一致するバリデータは attestation を返す(大文字小文字を区別しない) |
| TC-143 | P2 | `src/xrpl/toml.rs` | 不正な TOML → Err。VALIDATORS 欠落 → 件数 0 |
| TC-145 | P1 | `src/xrpl/poll.rs` | ポーリングのバックオフは 0→2→4→8 とエスカレートしウィンドウを設定する。成功で 0/None にリセットされる |
| TC-146 | P1 | `src/xrpl/nft_image.rs` | `fetch_resource` はリダイレクトを 5 回まで辿り、6 回目を拒否する。ホップごとにクライアントを再検証する |
| TC-147 | P1 | `src/xrpl/nft_image.rs` | `fetch_resource` は 4 MiB 超をヘッダとストリーム受信バイトの両方で拒否する |
| TC-148 | P2 | `src/xrpl/dunl.rs` | dUNL パースのエラー経路 — blob/base64/JSON/sequence/expiration の欠落 |
| TC-149 | P2 | `src/xrpl/dunl.rs` | dUNL expiration の固定 — ripple epoch の UTC 文字列が正確に一致する |
| TC-150 | P1 | `src/uninstall.rs` | `is_safe_uninstall_dir` の HOME ガード分岐と、`perform_self_uninstall` が安全なディレクトリを削除すること |
| TC-151 | P2 | `src/components/panels/ledger_objects.rs` | `LedgerObjectFilter::keep` が PayChannel/Escrow/Misc 行を分類する |
| TC-152 | P2 | `src/components/shared/tx_detail/format.rs` | `format_value` は長いオブジェクトを文字境界で切り詰める(マルチバイト安全) |
| TC-153 | P2 | `src/components/panels/tx_history.rs` | TxHistoryPanel — フィルタ入力は一致する行のみを描画する |
| TC-154 | P2 | `src/components/panels/tx_history.rs` | TxHistoryPanel — append はフィルタ済みキャッシュを保持する |
| TC-155 | P2 | `src/components/panels/tx_history.rs` | TxHistoryPanel — tx 詳細トグルは選択中の tx JSON を表示する |
| TC-156 | P2 | `src/components/panels/tx_history.rs` | TxHistoryPanel — 履歴全体の置換はキャッシュ済み行を無効化する |
| TC-157 | P1 | `src/xrpl/cli_exec.rs` | 全ての読み取り専用 CLI コマンドが決定的なローカル JSON-RPC サーバに対して成功する |
| TC-158 | P1 | `tests/cli_smoke.rs` | `lazyxrp` と `rp` のバイナリが argv をパースし、終了ステータスを返し、ローカル RPC の出力を描画する |
| TC-159 | P2 | `src/components/tabs/assets.rs` | Assets のフォーカスは NFT → objects → pay channels → escrows を両方向に循環する |
| TC-160 | P2 | `src/components/panels/account.rs`, `src/components/panels/book.rs` | Account パネルは loading と populated の状態を描画する。Book パネルは empty 状態も描画する |
| TC-161 | P2 | `src/components/panels/amm.rs` | AMM パネルは loading と populated のプールサマリを描画する |
| TC-162 | P2 | `src/components/panels/oracle.rs` | Oracle パネルは not-configured と価格テーブルの状態を描画する |
| TC-163 | P2 | `src/components/panels/trust_lines.rs` | Trust-lines パネルは loading、empty、populated の状態を描画する |
| TC-164 | P2 | `src/components/panels/path_find.rs` | Path-find パネルは loading と経路サマリの状態を描画する |
| TC-165 | P2 | `src/components/panels/server.rs` | Server パネルは loading とサーバメトリクスの状態を描画する |
| TC-166 | P1 | `src/xrpl/address.rs` | 送金先の解決 — 空/不正は拒否、X-address のタグは保持、classic にはタグなし。X-address ネットワークガードは test/production の不一致を拒否し、一致するネットワークは通す |
| TC-167 | P3 | `src/components/panels/server_dunl.rs` | `validator_row_label` — 収まる場合は domain を優先、長い domain は中央で切り詰め、domain なしの場合は短縮した公開鍵にフォールバックする |
| TC-168 | P3 | `src/components/panels/wallet_keygen.rs` | Keygen ポップアップ — keygen 結果がなければ何も描画しない。seed/address/pubkey とオフライン警告を表示する |
| TC-169 | P3 | `src/components/panels/server_detail.rs` | バリデータ詳細 — identity + dUNL + 取得中の状態、ローテーション済み master 付きの検証済み TOML + raw ダンプ、manifest 欠落 + TOML エラーの状態 |
| TC-170 | P3 | `src/components/tabs/account_wallet.rs` | ウォレット検出 — seed 設定済み、mnemonic 設定済み、署名設定なしの各分岐 |
| TC-171 | P3 | `src/components/shared/splash.rs` | Splash ヘルパー — `splash_art_column` の中央寄せ/幅の端、`splash_ascii_lines_for_height` の収まる/ぎりぎりのケース |
| TC-172 | P2 | `src/flare.rs` | FTSO フィード正規化 — trim、空要素の除去、`FXRP/USD` → `XRP/USD` へのマッピング、順序を保った重複排除。手数料を考慮したリトライ付きの `getFeedsById` バッチ取得と未知フィードの失敗は `#[ignore]` の live テストでのみカバーされる |
| TC-173 | P2 | `src/xrpl/dunl.rs`, `src/xrpl/poll.rs` | dUNL キャッシュ — stale-on-error は 60 s のリトライクールダウン中にキャッシュ済みサマリを返し、クリア後は返さない。タブゲート — account パネルは Account タブでのみポーリングする(market/assets のゲートは変更なし) |
| TC-174 | P2 | `src/components/shared/status_bar.rs` | ステータスバーのヘルス表示 — 鮮度対応の XRPL チップ（30 秒で STALE、`server_info` エラー = OFFLINE）、flare ドット（enabled/failed/stale）、dUNL データ経過時間マーカー |
| TC-175 | P1 | `src/lib.rs` | Flare RPC URL — `--allow-insecure-rpc` なしの `http://` は拒否。指定ありでも署名には使用不可 |
| TC-176 | P1 | `src/signing.rs` | ノードが選んだ Fee が `MAX_NODE_FEE_DROPS` 超なら署名前に拒否（上限値ちょうどは受理） |
| TC-177 | P1 | `src/xrpl/toml.rs`, `src/xrpl/nft_image.rs` | toml ドメインは単純なホスト名のみ受理。プライベート/予約済みリテラル（`100.64/10`、`198.18/15`、`240/4` を含む）はリクエストなしで拒否 |
| TC-178 | P1 | `src/components/shared/fmt.rs`, `src/xrpl/format.rs`, `src/flare.rs` | リモート由来のマルチバイト文字列 / NaN 金額 / 過大な uint256 で表示ヘルパーやソートが panic しない |
| TC-364 | P1 | `src/xrpl/nft_image.rs`, `src/components/tabs/nft.rs` | `ipfs_spellings_normalize_to_one_gateway_path`（`ipfs://`、`IPFS://`、`ipfs://ipfs/`、`/ipfs/`、素の CIDv0/v1 → 1 つのゲートウェイパス）、`data_uri_decodes_base64_only`、`display_uri_converts_ipfs_to_public_gateway`（`https://ipfs.io/ipfs/…` として表示、他の URI は変更なし）。NFT タブの URI 列は幅対応（広い端末は URI 全体を表示、狭い端末は `…` で終わる、TC-178 のマルチバイトケース）および `draw_strips_control_and_bidi_chars_from_uri` |
| TC-179 | P1 | `src/xrpl/client.rs` | 16 MiB 超のチャンク本文はストリーミング中に拒否 |
| TC-180 | P2 | `src/xrpl/parse.rs` | submit エラーテキストと tx ハッシュから制御文字を除去 |
| TC-181 | P2 | `src/config.rs` | グループ/ワールド読み取り可能な認証情報 config は（stderr に）警告を出す。0600 は出さない |
| TC-182 | — | `install.sh`, `.github/workflows/cd.yml` | 手動: トークンが argv に出ない（stub-curl で確認）。`cargo package` はトークンなし、その後 `publish --no-verify` |
| TC-183 | P2 | `src/xrpl/parse.rs` | `account_offers` 行 — 小文字キーをマップ。u32 の `seq` がないエントリはスキップ |
| TC-184 | P1 | `src/signing.rs` | OfferCancel の simulate JSON と署名済み blob が `OfferSequence`/`Sequence` を持つ |
| TC-185 | P1 | `src/components/panels/wallet.rs`, `src/xrpl/submit.rs` | OfferCancel コンポーザ — `8` でオファー取得、`j`/`s` で選択した seq をキューに積む、未選択エラー、取得エラーを表示。`--yes` なしのメインネットは拒否（TC-088 は EscrowCreate / EscrowFinish·Cancel も対象） |
| TC-186 | P1 | `src/signing.rs` | Payment SendMax — 通貨間では設定（`Paths` なし）、XRP→XRP の SendMax は拒否 |
| TC-187 | P1 | `src/signing.rs`, `src/components/panels/wallet.rs`, `wallet_keys.rs` | `send_max_xrp_drops`（XRP→drops、IOU→0、不正→Err）。コンポーザの Send max 行の編集、trim 済み/空→`None`、プレビュー `spend ≤` |
| TC-188 | P1 | `src/xrpl/cli_exec.rs` | CLI `send` — ローカル JSON-RPC が受け取るのは厳密に `server_info → account_info → simulate → submit`（ネットワーク確認が先、未 simulate の署名なし） |
| TC-189 | P1 | `src/lib.rs` | コマンド解決 — 引数なし → TUI は `--account` を保持。サブコマンド単体は `lazyxrp -x <name>` を示してエラー。`-x` 単体はエラー。`-x info` → Info |
| TC-190 | P1 | `src/cli.rs` | DEX/faucet CLI のパース — `offers <ADDR>`、`offer-cancel <SEQ>`、`offer-create <GETS> <PAYS>`、`faucet [ADDR]`。数値でない seq は拒否 |
| TC-191 | P1 | `src/xrpl/cli_exec.rs` | DEX CLI — `offers` は `account_offers` を読む。鍵なしの `offer-cancel` は RPC 前に失敗。不正な `offer-create` 指定は RPC 前に失敗（`TakerGets:`）。鍵ありなら両方 `server_info → account_info → simulate → submit` を実行 |
| TC-192 | P1 | `src/xrpl/format.rs` | Quote のソース通貨 — Send max の prefix が優先、IOU の Amount は XRP が既定、XRP の Amount は非 XRP の prefix が必要、IOU の prefix には issuer が必要 |
| TC-193 | P1 | `src/xrpl/format.rs` | Quote → Send max 指定 — 最安の代替、スリッページは切り上げ（drops / 有効数字 6 桁）、パスなし → Err |
| TC-194 | P1 | `src/components/panels/wallet.rs` | Payment `p` — 不正なフォームは flash。有効なフォームは `PaymentQuote` をキューに積む（既定 100 bps）。QuoteOk で Send max を埋める。QuoteErr は flash |
| TC-195 | P2 | `src/config.rs` | ユーザーの `[keybindings.Splash]` はキーを追加し既定を上書き。未変更の既定は保持 |
| TC-196 | P0 | `src/xrpl/cli_exec.rs` | CLI `send` — submit が拒否（`tefPAST_SEQ`）されたら Err を返す → 終了コード非 0 |
| TC-197 | P1 | `src/signing.rs` | `validate_payment_input` — 宛先のチェックサム / X-address、正の金額、IOU の通貨+issuer、SendMax 指定（XRP→XRP は拒否、空 = なし） |
| TC-198 | P1 | `src/components/panels/wallet.rs` | Payment プレビューと `s` の検証共有 — チェックサム不正 / Send max 不正はオレンジのままで、submit は flash 付きで拒否 |
| TC-199 | P1 | `src/xrpl/submit.rs`, `src/test_support.rs` | ローカル JSON-RPC に対する poll 書き込み経路 — Payment / OfferCancel は `server_info → account_info → simulate → submit` → Ok。quote は `source_currencies` を送り、スリッページ指定を返す |
| TC-200 | P0 | `src/xrpl/parse.rs` | `LastLedgerSequence` なしの `simulate`（rippled 3.4.1 の形）→ `ledger_index + 20` で補完し署名を続行。サーバー値は保持 |
| TC-201 | P1 | `src/xrpl/cli_exec.rs` | `faucet` — testnet/devnet の URL のみ（mainnet/xahau は拒否）。`destination` を POST し、金額 + ハッシュを報告。別アドレスを示すレスポンス → Err |
| TC-202 | P1 | `src/xrpl/cli_exec.rs` | CLI 書き込み待機 — validated になるまで `tx` をポーリング（`tes` → Ok、`tec` → コード付き Err）。validated ledger が LLS を超えても見つからない → Err（期限切れ）。進まない / `validated_ledger` を返さないノード → Err（タイムアウト、上限 5 分） |
| TC-203 | P0 | `src/network.rs`, `src/xrpl/cli_exec.rs`, `src/xrpl/submit.rs` | エンドポイントの `network_id` は署名前に選択ネットワークと一致必須（メインネットは省略 = 0）。`--network testnet` 下のメインネットノードは `server_info` のみで拒否（CLI）。testnet ノード上の devnet セッションは拒否（TUI） |
| TC-204 | P0 | `src/xrpl/parse.rs`, `src/xrpl/submit.rs` | simulate の `Sequence` がリクエストと不一致、または `LastLedgerSequence` が `ledger_index + 100` 以下 / 超過 → 署名前に拒否（全書き込み経路） |
| TC-205 | P0 | `src/flare.rs` | FXRP C2 Core Vault: `expected_core_vault` と完全一致の場合のみ署名。不一致 / 未設定 → 拒否 |
| TC-206 | P1 | `src/xrpl/format.rs` | Quote は要求された Send max の通貨/issuer の代替のみ採用。不一致 → Err。XRP スリッページのオーバーフロー → Err（TC-193 の拡張） |
| TC-207 | P1 | `src/signing.rs`, `src/xrpl/submit.rs` | 送信 tx ハッシュ = SHA-512Half(`TXN\0` ‖ blob)、独立したベクタで検証。ノード報告のハッシュは無視 |
| TC-208 | P1 | `src/xrpl/parse.rs` | simulate の `engine_result(_message)` から制御文字を除去 |
| TC-209 | P1 | `src/xrpl/cli_exec.rs` | CLI 待機: ledger が LLS を過ぎた後に validated となった tx は 1 回再確認 → Ok、期限切れ扱いにしない（TC-202 の拡張） |
| TC-210 | P1 | `src/xrpl/client.rs` | 307 を返す JSON-RPC エンドポイント → Err。リダイレクト先へはリクエストを再 POST しない |
| TC-211 | P1 | `src/signing.rs` | `XRP:<drops>:<tail>` は拒否（SendMax と OfferCreate の指定） |
| TC-212 | P1 | `src/xrpl/cli_exec.rs` | `-x send 0` は RPC 前に `validate_payment_input` で拒否 |
| TC-213 | P1 | `src/components/panels/wallet_keygen.rs`, `src/config.rs` | Keygen ポップアップは NetworkChange でクリア、ラベルは "Esc to dismiss"。`Config::new` は Flare EVM 鍵の env を取得して削除（TC-049 の拡張） |
| TC-214 | P1 | `src/xrpl/submit.rs` | TUI の submit 結果 = `<local hash> · fee N drops`（署名した simulate の fee） |
| TC-215 | P1 | `src/signing.rs` | Escrow の期間 `90s`/`30m`/`2h`/`7d` → 秒。空、単位なし、ゼロ、負、小数、未知の単位、u32 オーバーフローは拒否 |
| TC-216 | P1 | `src/signing.rs` | EscrowCreate 入力 — classic の Destination、XRP > 0、FinishAfter 必須、CancelAfter は任意で厳密に後 |
| TC-217 | P1 | `src/signing.rs` | EscrowCreate / EscrowFinish / EscrowCancel の simulate JSON と署名済み blob が Amount/Destination/FinishAfter/CancelAfter および Owner/OfferSequence を持つ |
| TC-218 | P1 | `src/xrpl/parse.rs` | `account_objects` の Escrow 行（owner、destination、時刻、condition、オブジェクトの `Sequence`）。作成 tx → `Sequence`、0 の場合は `TicketSequence`（API v1 と v2 の形） |
| TC-219 | P1 | `src/components/panels/wallet.rs` | EscrowCreate コンポーザ — `9` で開く。不正なフォームは flash。有効なフォームは trim 済みパラメータをキューに積む。Esc → ピッカー行 9 |
| TC-220 | P1 | `src/components/panels/wallet.rs` | Escrows コンポーザ — `0` で取得。`s` で選択を finish / `c` で cancel。未選択、seq 未解決、condition 付き escrow の finish は flash 付きで拒否 |
| TC-221 | P1 | `src/xrpl/submit.rs` | ローカル JSON-RPC に対する Escrow 経路 — Create は署名する絶対 FinishAfter（now + 1h）/ CancelAfter を simulate。Finish は Owner/OfferSequence を持つ。`account_escrows` は `tx` 経由で seq を解決 |
| TC-222 | P1 | `src/signing.rs` | AMM 入力テーブル — deposit: 同一アセット / ゼロ金額は拒否。withdraw: 同一アセット、issuer 欠落 / 不正、アセット欄に金額、LP `0` / 非数値は拒否。LP 空 = 全額 withdraw は OK |
| TC-223 | P1 | `src/signing.rs` | AMM tx JSON と署名済み blob — deposit は `tfTwoAsset` で Asset/Asset2 を金額から設定。全額 withdraw は `tfWithdrawAll`。LP は `tfLPToken` + `LPTokenIn` |
| TC-224 | P2 | `src/xrpl/parse.rs` | `amm_info` の LP トークン識別（`lp_token` または `LPToken`）→ `(currency, AMM account)` |
| TC-225 | P1 | `src/components/panels/wallet.rs` | AMM コンポーザ — `d` / `w` で開く。不正なフォームは `AMM ·` を flash。有効なフォームは trim 済みパラメータをキューに積む。LP 空 = 全額 withdraw。Esc → ピッカー行 10 / 11 |
| TC-226 | P1 | `src/xrpl/submit.rs` | ローカル JSON-RPC に対する AMM 経路 — deposit は `tfTwoAsset` を simulate。LP 指定の withdraw は simulate 前に `amm_info` を呼び、その通貨 / issuer で `LPTokenIn` に署名 |

## トレーサビリティ

| TC | FR / NFR | Architecture § | Design |
|----|----------|------------------|--------|
| TC-111–112 | FR-12 | §6.1 presets | §6 `Ctrl-n` |
| TC-113–114 | FR-15 | §4 Flare config | §4 Flare layouts |
| TC-115 | FR-12 | §6.1 session switch, §6.5 badge | §5 Status bar, §6 `Ctrl-n` |
| TC-116–119 | FR-15 | §6.7 | §4 Flare layouts |
| TC-120–122 | FR-16 | §6.8 Flare wallet panel | §4 Overview wallet strip |
| TC-123–124 | — | §3 public-network politeness | — |
| TC-125–127 | FR-12 | §6.1 presets, §4 CLI | — |
| TC-128 | — | — | §2 Color + YAML frontmatter |
| TC-129–135 | FR-16 | §2 wallet composer, §5.2 shared behaviors | §6 wallet/composer keys |
| TC-136 | — | §6.5 status bar | §5 Status bar |
| TC-137, 139–141 | — | §5 read APIs | §5 Table / TX row |
| TC-142–143 | — | §3 public-network politeness | — |
| TC-144–145 | — | §3 poll loop | — |
| TC-146–147 | NFR (SSRF) | §3 SSRF fetch guards | — |
| TC-248–256 | FR-18 | §2 Transaction composer, §2.1 txspec + coverage | §6 composer `m`, NFT `x` |
| TC-148–149 | — | §3 public-network politeness | — |
| TC-150 | — | §1 self-uninstall | — |
| TC-151–152 | — | §5.4 ledger objects, §5.2 tx detail | §4 Assets / tx detail |
| TC-153–165 | — | §5.1 Assets panels, §5.2 tx history, §4 CLI | §4 Assets tx history |
| TC-166–171 | — | §2 address guards, §3 dUNL | §5 (splash) |
| TC-172–174 | — | §3 public-network politeness | §5 Status bar |
| TC-239–247 | FR-17 | §6.9 Flare Smart Accounts, §2 wallet composer | §4 Overview FXRP rows, §6 composer `a` |

## リスクカバレッジ

| リスク | カバーするもの | 根拠 |
|------|------------|----------|
| R-006 mainnet 書き込みガードの回避 | TC-088 `mainnet_submit_without_yes_is_rejected` | `src/xrpl/submit.rs` |
| R-007 config マージの優先順位 | TC-033-035/TC-092 `config_source_precedence`, TC-036 | `src/config.rs` |
| R-001 seed 優先順位チェーン | TC-045/046/049 `credential_from_secrets` + 環境変数による上書き | `src/signing.rs`, `src/config.rs` |
| R-016 エンドポイントのネットワーク ≠ 選択中のネットワーク | TC-203 `endpoint_network_id_must_match` + CLI/TUI での拒否 | `src/network.rs`, `src/xrpl/cli_exec.rs`, `src/xrpl/submit.rs` |
| R-017 Smart Account の operator / fee が 1 回の RPC 応答に由来する | TC-240, TC-243（`check_operator`, `smart_account_instruction_refusals_never_reach_rpc`）。mainnet ゲート TC-088 | `src/flare.rs`, `src/xrpl/submit.rs` |
| R-019 C2 の recipient が RPC から供給された personal account でプリフィルされる | TC-367（署名者自身の、新しい read の場合のみプリフィル。全文表示）— 残存リスクは受容、security-ja.md S-019 | `src/components/panels/wallet_composer.rs` |

## Testnet E2E

**自動（オプトイン）:** `mise run e2e-testnet`（`tests/testnet_e2e.rs`、`#[ignore]`、CI では実行しない）: `-x faucet` ×2 → `send` → `offer-create` → `offers` → `offer-cancel` → `offers` = `[]`、すべての書き込みが `Validated: tesSUCCESS`。リリースごとに実行する。2026-10-11 に成功（≈ 27 秒）。

**NFT mint → burn（オプトイン、同じファイルの `testnet_nft_mint_burn_validate`）:** faucet アカウント → `nft-mint --taxon 4242 --flags burnable,transferable --transfer-fee 500` → `nfts` に表示される → `nft-burn <id>` → `nfts` が空、すべて `Validated: tesSUCCESS`。下の実行メモを参照。

**実行メモ（2026-10-16）:** testnet で、12 回の試行がすべて最初の書き込みの simulate で `[XRPL-11] Fee of … exceeds the requested tx limit of 100` により停止した。原因は計測済みで lazyxrp には起因しない: `fee` RPC は `open_ledger_fee` 6509–27343 drops（base 10、`load_factor` ≈ 501、ledger は 5651 tx に対し想定 4953）を報告し、lazyxrp の外から送った AccountSet の素の `simulate` も同じ失敗をする。`simulate` は `fee_mult_max` を無視するため、明示的な `Fee` を指定したときだけ通る。**devnet**（空いている、open-ledger fee 1 drop）で `LAZYXRP_E2E_NETWORK=devnet mise run e2e-testnet` により再実行した。並列実行すると mint が `telCAN_NOT_QUEUE` を 1 回受けたため、タスクは `--test-threads=1` を渡すようにした。その後タスク全体が成功した（2 passed、40 秒）: send / offer-create / offer-cancel と NFT mint → `nfts` → burn → `nfts` が空、すべての書き込みが `Validated: tesSUCCESS`。それ以前の単独テスト実行: mint `75786BFA29DC04B4834F15546D3EB7112CE5D6B09882E7B85078E7769C1FEB62`、burn `B3DC83C16DFFC498704D90204251590B3D9D6A02C2187E717C06F754338C41D9`。ライブ検証済みなのは NFT の Mint/Burn のみで、他の spec ファミリーはオフラインテストのみ。既知のギャップ: 手数料が大きくエスカレートしている状況では、アプリは simulate できない（`simulate` に `Fee` を渡していない）。

**AMM（手動、2026-10-11）:** XRP/TST プールは使い捨ての xrpl.js スクリプトで testnet 上に作成した（lazyxrp には AMMCreate がない）。CLI `-x amm-deposit XRP:1000000 TST:<iss>:10` と `-x amm-withdraw XRP TST:<iss> --lp 1000`、続いて TUI コンポーザーの `d`（deposit。Amount2 により XRP 側は 0.5 XRP に制限された）と、LP を空にした `w`（全額 withdraw。唯一の LP の withdraw でプールが削除された）: すべて `tesSUCCESS`。

**FXRP C2 + C3（手動、2026-10-11、XRPL testnet + Coston2）:** config は `[flare] network = "coston2"`、`[flare.fassets] execute = true`、`expected_core_vault = "rDhpmiPq4BVBDWMVdSrmkgt8thKyRzGV1p"`、`FLARE_EVM_KEY` = executor `0xeD50…2C98`。

| ステップ | 結果 | Hash |
|------|--------|------|
| TUI コンポーザー `6` FXRP Mint、10 XRP、recipient = executor | 最初の試行は simulate で失敗（`MemoFormat` が null → TC-230）。修正後は `tesSUCCESS`。公開 executor が数分以内に確定させた → FXRP 9.8 | `FB8556096C0BF6AE25F858DB9AF65CC0381E51FF100505672229EAACCE5A1A78` |
| その payment の proof での TUI `7` C3 | revert `PaymentAlreadyConfirmed` (0x18dce79f) — すでに公開 executor が mint 済み | — |
| 48 バイトの memo 付き 5 XRP の payment（executor = 自分たち、使い捨ての xrpl.js）→ proof（Flare 公開の testnet キーによる verifier `prepareRequest`、`FdcHub.requestAttestation`、DA `proof-by-request-round`。使い捨ての viem スクリプト）→ TUI `7` C3 | `executeDirectMinting` が成功。FXRP 9.8 → 14.7（4.9 = 5 − 0.1 最小 fee。executor fee は自分自身へ） | XRPL `C0FC83E57C37BC4782B48E4CB4EB0123829137B52A4C1CC8B6F57E38B548AE6A`、Flare `0x1460dc9ae36bbe58beb4a8fd1a6d6bd358d4fd3f5a315583646cd31d111e5f02` |

修正前の C3 はコントラクトに存在しないセレクタ（`0x548cddd4`、`bytes data`）を呼んでいた。デプロイ済みのものは `0x78d0299e`（デコード結果は `IXRPPayment.Response`）→ TC-231。

**Mainnet executor の観測（read-only、2026-10-24）:** 公開 RPC では `eth_getLogs` が 30 ブロック未満に制限されているため、AssetManagerFXRP（`0x2a3f…b6a8`、registry 由来）のログは Flare エクスプローラー API（`module=logs`、約 240 万ブロック ≈ 50 日。1 クエリあたり 1000 件）から取得した。`DirectMintingExecuted`: 905 イベント、executor は 10 種類、905 件すべてで executor ≠ recipient。`DirectMintingExecutedToSmartAccount`: 1000 件（クエリ上限のため ≥ 1000）、executor は 8 種類。最新イベントは 1 時間未満前。XRPL validated → Flare executed: 118, 163, 95, 117, 154, 104, 177, 120 秒（Smart Account イベント 8 件、XRPL `tx` の日時と Flare のブロック時刻の比較）。**これが示さないこと:** executor が lazyxrp の低い `--fee` ／ プロトコルの最小 executor fee で payment を確定させること（executor の fee 下限は未計測）、これらの executor を誰が運用しているか、Smart Account の `0xFE` payment が第三者によって確定されること（user-operation のバイト列がないため）。mainnet では何も送信していない。

**AccountDelete（手動、2026-10-11、testnet）:** 新規の `-x faucet` アカウント 2 つ（ledger 21,447,697 で入金）、宛先 = アカウント B `rLo2…dnJ`。

| ステップ | 結果 | Hash |
|------|--------|------|
| CLI、入金から 12 分後、宛先 = 自分自身 / 宛先 = B | `simulate temDST_IS_SRC` / `simulate tecTOO_SOON`（署名なし） | — |
| TUI コンポーザー `x`、B のタグ 777 付き X-address → `DELETE` | ledger 21,447,957 で `tesSUCCESS`。tx は `DestinationTag: 777`、`Fee: 200000`（owner reserve、simulate が補完）を持つ。その後ウォレットパネルは "account not found on this network" を表示する | `1A92B4DF3AF48E11359443FF5F2A7EF0A5C96DD3A912A0717A07D9E6882958AE` |
| CLI `-x account-delete <B>` | `Validated: tesSUCCESS`。削除したアドレスへの `-x account` は `Account not found` | `6B596EBA18F7BF35C6D8C04E78D79F90E2040A1B9E5E6FBEC2882DB62A9E421E` |

**2026-10-10 CLI（手動、0.3.0）** — testnet rippled 3.4.1、faucet アカウント `r15nczQYUNYBVLws8M9zkFE2SaSkU4kkk`。すべての hash は `validated: true`、`tesSUCCESS`。

| ステップ | 結果 | Tx hash |
|------|--------|---------|
| `lazyxrp -x send rpvyTEMx… --amount 1.5` | exit 0 | `8F3647ED55D574932C956EB81DEA093260C33F320D40C07DFEF9F1FF28CA8962` |
| `lazyxrp -x offers r15ncz…` | seq 21430112 を列挙 | — |
| `lazyxrp -x offer-cancel 21430112` | exit 0。`offers` → `[]` | `4BA39A804683036C1D9B61478D3596555C1DF68FE31187577C8057AA3C8AE64D` |
| すでに削除された offer に対する `offer-cancel` | exit 0（XRPL の no-op `tesSUCCESS`） | `0620179B9177C20E333E7C2D85093F642BA1454C0B329D49F3BC6C62CC404173` |
| チェックサム不正なアドレスへの `send` | exit 1、RPC の書き込みなし | — |

最初の実行ではすべての書き込みが `simulate tx_json missing LastLedgerSequence` で失敗した（モックは LLS を返したが、ライブの simulate は返さない）→ 修正済み、TC-200。

**2026-10-11 TUI（tmux 経由で操作）** — 送信者 `rwJXoAen3drVz6tep2CzXz7jFwS7G8pKnz`、issuer B `rwENtdso…`（DefaultRipple、100 USD を 100 XRP で売る offer）、宛先 D は B を trust している。すべて `validated: true`、`tesSUCCESS`。

| ステップ | 結果 | Tx hash |
|------|--------|---------|
| コンポーザー `4` OfferCreate `XRP:1000000` → `USD:B:1` | B の offer と即座に約定した（A は 1 USD を受領）。OfferCancel の一覧は空 — 正しい | `FB22BA340747D44D4A39805081302F930BA0EEA808C2C60C6E8D4859930C7465` |
| コンポーザー `1` Payment IOU 1 USD.B → D、`p` | Send max が `XRP:1010000`（1 XRP + 1 %）で埋まった | — |
| `s` 通貨をまたぐ Payment | 1 USD が配達された | `2FE724657854C59A5E87E0AD1D843B115D197E9F9CCEF2FC020ED18E6023401D` |
| コンポーザー `4` OfferCreate `XRP:1000000` → `USD:B:100`（約定しない） | offer `#21432418` が `8` に一覧表示される | `3FDB2522AC0E8A698C01D5C49BF962BFC0F8C958AE8A91E5B668215D519C6F6E` |
| コンポーザー `8` → `s` | offer が消える（`(no open offers)`） | `DB7A79BDFCB81373FB98BDD40886A9593A986D2FC3F11D0EDE6833297CAF4EA8` |

注記: 最初の quote は "no payment path found" を返した — issuer に DefaultRipple がなかったため正しい挙動（`simulate` = `tecPATH_DRY`）。ここで **S-017** が見つかった: シェルの mainnet `XRPL_RPC_SERVER` が `--network testnet` を気づかないうちに上書きした（read が mainnet に向いた）→ TC-203。ライブ確認: mainnet エンドポイント + `--network testnet` は拒否された（exit 1）、testnet エンドポイントは `868477854FBAD38A1B22629659BA4A98DC945E994422728DE99805BBF43B6209` で検証された。

**2026-10-11 TUI escrow（PTY 操作、pyte screen）** — owner `rGyZL1VDvkzBUWY7rwJ4BaiXRaxH85wHAR`、宛先 `rLo2GiHSRivGN17CZhJGFpBbYGXEACpdnJ`（どちらも `-x faucet`）。Tx History ではすべて `tesSUCCESS`。

| ステップ | 結果 | Tx hash |
|------|--------|---------|
| コンポーザー `9` EscrowCreate 1 XRP、FinishAfter `90s` | owner 1、一覧 `#21443113`（seq 解決済み） | `4A7D8FE1756DB468A7B996A8F28E471D8E5E8A655D2E7993B142685CA31812DA` |
| コンポーザー `9` EscrowCreate 1 XRP、`60s` / CancelAfter `120s` | finish と cancel の時刻付きで一覧 `#21443114` | `40ADC91F1B566A615761EFD8C10293765D3C1461D8852EA677797850DDFB7A8A` |
| FinishAfter より前の `0` → `s` | `simulate tecNO_PERMISSION` の flash、署名なし | — |
| CancelAfter 後に `#21443114` で `0` → `c` | 返金、owner 2 → 1 | `4B0A8C61AA9AB183A3D7E2F7CF61C112116DF762F8DF10B73BD8C2E2C45A8603` |
| `#21443113` で `0` → `s` | finish 済み、owner 0、一覧 `(no escrows)` | `D29C54F24D0AF9DBCE735475FA0270578476C313BE4D0B0A81A303D88FAF1B54` |

残りのギャップ（設計上未実装 — 優先度低）:

- **R-002** submit の channel-close の可観測性: `send_action` は warn ログのみを出す。永続化は未実装。
- **R-003** `ArcValue` 共有 JSON の不変性: 規約のみ。違反は視覚的な破損で検知される。
