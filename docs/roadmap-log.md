# Roadmap log — 進路変更記録・計画上の注意点

**Role:** [`ROADMAP.md`](../ROADMAP.md) の補助記録。目標の階層と優先順位は ROADMAP、ここには「なぜ方向が変わったか」と「計画上の注意点」を置く。コードレベルの落とし穴・既知問題は [`problems.md`](problems.md) が SSOT。

## Cautions（計画上の注意点）

- **セキュリティは非交渉。** mainnet write は simulate + `--yes`、seed/mnemonic は `credential_from_secrets` 経由のみ、config の world-readable 警告を外さない。採用（人気）追求のために緩めない。詳細は [`security.md`](security.md)。
- Mainnet C3 execute smoke は FDC verifier API key + 実資金が必要（外部依存。入手できるまで中期に置きっぱなしで可）。
- Keymap customization は `src/config.rs` を触る → AGENTS.md 契約により config キーに言及する全 `docs/` を同一変更内で同期。組み込みデフォルトは `config.json5`。
- OfferCancel（旧 F-02）は DEX trading に統合済み。個別タスクとしては追わない。

## Pivot log（進路変更記録）

- **2026-10-11** — 中期 #4 AMM Deposit/Withdraw を実施。判断（ユーザー選択）: deposit は two-asset（`tfTwoAsset`）のみ、withdraw は全額（`tfWithdrawAll`）か LP 量（`tfLPToken`）、プールは composer に直接入力、CLI `-x amm-deposit` / `-x amm-withdraw` も追加。AMMCreate は作らず、testnet 検証用プールは使い捨て xrpl.js スクリプトで作成（ユーザー選択）。そのため `tests/testnet_e2e.rs` には AMM を入れていない（プールを自前で作れない）。注意: `LPTokenIn` の currency / issuer は `amm_info` から引く（AMM アカウントは計算でなく問い合わせ）。picker が 12 行になり wallet パネル（40 行端末で約 16 行）に収まらないため、選択行が見えるようスクロールさせた。
- **2026-10-11** — 0.3.1 公開後、中期 #1 `poll.rs` 分割（書き込み経路を `xrpl/submit.rs` へ、挙動不変）と #3 EscrowCreate を実施。docs は「signing/poll 既存・UI 未配線」としていたが、実際は signing も無く poll の catch-all が `EscrowCreateSubmit` を黙って捨てていた（catch-all を外して再発防止）。判断（ユーザー選択）: 時間は相対指定（`90s`/`30m`/`2h`/`7d`）、CancelAfter は任意、長期 F-03 EscrowFinish/Cancel を前倒しして lazyxrp 内で完結、CLI `-x escrow-*` は作らず TUI のみ。注意: Escrow ledger object には `Sequence` が無いことがあるので、Finish/Cancel の `OfferSequence` は作成 tx（`PreviousTxnID`）から引く。tmux が無い環境では PTY + pyte で TUI E2E を実施。
- **2026-10-11** — 0.3.1 の前に OSS パッケージとしての標準形を整えた: `CHANGELOG.md`（Keep a Changelog。`mise run release` が `[Unreleased]` を確定）、`SECURITY.md` と private vulnerability reporting、MSRV の CI 検証（宣言だけで未検証だった 1.94.1 を実測）、crate から内部 docs を除外。判断: library は binary 共有用で公開 API は `run` / `run_rp` のみ（docs.rs は利用案内の入口にとどめる）。CLI の validated 待ちに 5 分の上限を追加（ノード停止でスクリプトが止まらないように）。
- **2026-10-11** — 0.3.1 の 4 項目（faucet・offer-create・validated 待ち・opt-in E2E）を実装し、TUI E2E を tmux で実施。E2E 中に **S-017** を発見して P0 で追加・修正した: シェルの mainnet `XRPL_RPC_SERVER` が `--network testnet` の preset を上書きし、`--yes` ガードを外したまま mainnet へ署名・送信し得た。判断: env の優先順位（§6.2）は変えず、署名前に `server_info.network_id` を照合（ユーザー選択）。CLI write は既定で validated まで待つ（ユーザー選択。終了コードが最終結果を表すので、スクリプトで正しく使える）。`--no-wait` で従来動作。注意: 手動確認でも env に endpoint や seed が残っていないか先に確かめる（read が別ネットワークへ向いて「空」に見える）。
- **2026-10-11** — ROADMAP に「次期 0.3.1: live 検証 + testnet DX」を新設。0.3.0 の E2E では faucet・OfferCreate・validated 確認に外部 xrpl.js が必要で、mock と実ノードの差で全 write の失敗も見逃した。この 2 点を優先するため、`-x faucet` を中期 P2 → 次期 P1 に上げ、opt-in の testnet E2E 自動化・`-x offer-create`・submit 後の validated 確認を追加。中期は「0.4.0: write 基盤整理 + TX 拡張」とし、`poll.rs` 分割を P1（EscrowCreate / AMM の前提）に、EscrowCreate / AMM を P3 → P2 に、C3 mainnet smoke を外部ブロックのため P3 に変更。0.3.0 には TUI E2E（P0）と GitHub Release 本文の移行注記（P1）を残した。simulate の mock（TC-188/191/199）を実応答形（LLS なし + `ledger_index`）に揃えた。
- **2026-10-10** — testnet CLI E2E で、live の `simulate`（rippled 3.4.1）が LastLedgerSequence を返さず、TUI・CLI のすべての write が失敗していたことが判明。mock RPC が LLS を返していたためテストで見逃していた。`parse_simulate_result` で補完して修正（TC-200）。教訓: write 系の mock は実ノードの応答形を写すこと。リリース前に testnet E2E を省略しない。
- **2026-10-10** — 短期を「0.3.0 リリース」に再編。CLI の破壊的変更が未リリースのため、出す前に検証系（終了コード修正・検証一本化・poll write fixture・testnet E2E）を先行。`poll.rs` 分割を EscrowCreate / AMM 追加の前提として中期 P2 に昇格。security.md の構造課題・残余リスクを中期 / 長期に明示行として移した。
- **2026-10-10** — 短期 3 件（DEX trading / CLI redesign / keymap）を完了扱いに。判断: Payment 見積りは Send max 欄の通貨で `ripple_path_find`（自動最安選択は通貨間比較不可のため不採用）、`--seed`/`--mnemonic` は FR-13 維持＋警告、keymap はグローバルキーの user merge で完了（パネル内キーの設定化は長期）、`rc` alias は install.sh に入れない（シェル rc を書き換えない）。
- **2026-09-21** — ROADMAP を horizon 型（最終ゴール / 長期 / 中期 / 短期）へ再構成。最終ゴールを「XRPL エコシステム（dev/user）で人気の CLI/TUI ツール + セキュリティ維持」と明文化。短期 = DEX trading (P1) / CLI redesign (P2) / keymap customization (P3)。F-02 OfferCancel → DEX trading に統合、F-04 AMM を Later→P3。
- **2026-09-17** — 新機能より安定化を優先: Flare read-path 最適化・XRPL poll 効率化・status bar health（0.2.11）。
- **2026-09-12** — multichain 拡大（Xahau + Flare）と docs 階層再設計。`docs/` → `openspec/` の一方通行リンク規則を確立。
