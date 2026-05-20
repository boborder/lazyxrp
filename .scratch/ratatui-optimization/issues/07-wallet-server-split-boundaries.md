# Decide wallet and server panel split boundaries

Type: grilling
Status: resolved
Labels: wayfinder:grilling
Blocked by: 01, 06

## Question

巨大パネル分割: `wallet.rs` / `server.rs` をどう切るか？

決めること:

1. 分割の粒度（ファイル分割のみ / モジュール + 公開 API / タブ内コンポーネント抽出）
2. wallet の論理ブロック境界（composer / validate / preview / submit / draw）
3. server の論理ブロック境界（dUNL table / validator detail / gauges）
4. 分割と同時に計画へ載せる機能改善（06 の基準に合うものだけ）の扱い
5. 「分割完了」の定義（コンパイル境界 / 行数目標 / テスト移動）

## Answer

1. **粒度**: ファイル分割のみ — `panels/` 同階層に兄弟ファイル（例: `wallet_composer.rs`）。サブモジュールツリーや独立 Component 化はしない。
2. **wallet 境界**: composer / keys / keygen / shell（`WalletPanel` ファサード）。
3. **server 境界**: detail / dUNL / metrics / shell（`ServerPanel` ファサード）。
4. **UX**: 分割フェーズは構造移動のみ。UX 改善は別項目（混ぜない）。
5. **DoD**: コンパイル + 既存テスト緑 + 公開ファサード維持。厳密な行数目標なし。

下地: [assets/01-inventory-shared-table-hotspots.md](../assets/01-inventory-shared-table-hotspots.md) §3。
