# Ratatui 活用・最適化 — 最終実装計画

Destination 成果物（Wayfinder map）。**本番コードはまだ変えない。** 実装は後続セッション。

参照決定: [map.md](map.md) Decisions so far（01–07, 09）+ 本チケットで確定した Phase-2 順 / FPS·Splash / PR 分割。

---

## Goal

1. 共有 selectable-table ヘルパで表+Scrollbar の複製を潰す  
2. `needs_render` + min-frame でアイドル描画を抑える  
3. テーマは現行 RGB + `theme::` 経由へ段階移行  
4. `wallet.rs` / `server.rs` を同階層ファイル分割  

優先順（固定）: **shared → perf → theme → split**

---

## Phases

### Phase 1 — Shared selectable-table helper

**Do**

- `components/shared/selectable_table.rs` に関数 `render_*`（表本体+縦 Scrollbar）。併置 UI（Book BarChart 等）は呼び出し側。
- Phase-1 移行: `trust_lines` / `ledger_objects` / `path_find` / `nft` + 既存 `render_tx_scroll_table` を同契約へ統一。
- UX（09）: 選択テーブルの Scrollbar **thumb = `accent`** に統一（tx helper の secondary を合わせる）；発見性はヘルパ側で一貫（既存キー/選択互換を優先）。
- `SelectableTableState` は clone せず `&mut` のみ。

**DoD**

- 上記パネルがヘルパ経由で描画；j/k・選択・Enter 詳細が現状互換
- `cargo test` 関連緑；thumb が accent に揃っている

**Touch (initial)**

- `src/components/shared/selectable_table.rs`
- `src/components/shared/widgets.rs`（tx helper 寄せ）
- `src/components/panels/{trust_lines,ledger_objects,path_find}.rs`
- `src/components/tabs/nft.rs`

**Don't**

- book / server dUNL（→ Phase 2）
- oracle / FTSO（ステートレス；任意フォロー）
- 本番ロジック変更を装ったリファクタ混入

---

### Phase 2 — Shared helper expansion

**Do**

- 移行順: **book → server dUNL**
- Book: ヘルパは表のみ；BarChart は現状どおり呼び出し側
- server dUNL: 同ヘルパ；detail overlay の Scrollbar は secondary のままでも可（選択テーブル規則の外）

**DoD**

- book / dUNL がヘルパ利用；Book チャート併置が壊れない

**Touch**

- `panels/book.rs`, `panels/server.rs`（dUNL 描画部）

---

### Phase 3 — Dirty-render / frame policy

**Do**

- 常時 `Event::Render` 依存をやめ、`needs_render` + min-frame（~16ms）へ。
- Dirty 規則: **キー・リサイズ = 常時**；**Tick / XRPL データ = 見た目が変わったときのみ**。
- FPS / Splash / spinner: **見た目変化時のみ dirty**（常時 60 描画はしない）。
- `frame_rate` 名目 60 は維持（上限キャップ）；スロットルは min-frame。
- Row キャッシュ: **やらない**（dirty 後も痛ければ別判断 — 注記のみ）。

**DoD**

- アイドルで無駄なフルツリー描画が止まる感覚；操作時は主観スムーズ；既存 FPS 表示で確認
- 「画面が更新されない」経路がない（主要 Action が dirty を立てる）

**Touch**

- `src/tui.rs`, `src/app.rs`（および dirty 伝播に必要な最小限）

**Risk note:** dirty 付け忘れ → stuck UI。緩和: 入力/リサイズは無条件 dirty；データ更新は `update` の Ok 変化で dirty。

---

### Phase 4 — Theme & Stylize

**Do**

- 現行 Royal Blue RGB パレット維持。
- `HIGHLIGHT_FG` 白はブランド例外として残す。
- 色は必ず `theme::` 経由；`Stylize` は可。
- 完了定義 = **新規 + 触った箇所の段階移行**（一括寄せしない）。
- UX（09）: 空状態・ローディング・枠（`titled_block` / `panel_block` / 直書き Block）を触った範囲で揃え。

**DoD**

- 触ったファイルに生 `Color::Rgb` / 裸 `Style::new().fg` の色指定が残らない（theme 経由）
- 選択ハイライトの白例外が維持

**Touch**

- `shared/theme.rs`（必要ならヘルパ追加）
- Phase 1–2 / 5 で触るパネルに限定

---

### Phase 5 — Wallet / Server panel split

**Do**

- **ファイル分割のみ**（`panels/` 同階層の兄弟ファイル）。サブモジュールツリーや独立 Component 化はしない。
- wallet: `composer` / `keys` / `keygen` / `shell`（`WalletPanel` ファサード維持）
- server: `detail` / `dUNL` / `metrics` / `shell`（`ServerPanel` ファサード維持）
- **構造移動のみ**。UX/Esc 改善はこの Phase に混ぜない（07）。

**DoD**

- コンパイル成功；既存テスト緑；外部からの公開面はファサード維持
- 厳密な行数目標なし（殻が明らかに薄くなればよい）

**Touch**

- `panels/wallet.rs` → 例: `wallet_composer.rs`, `wallet_keys.rs`, `wallet_keygen.rs` + 殻
- `panels/server.rs` → 例: `server_detail.rs`, `server_dunl.rs`, `server_metrics.rs` + 殻
- `panels/mod.rs` の mod 宣言

---

## Cross-cutting UX（フェーズに紐づけ）

| 項目 | フェーズ |
|------|----------|
| thumb = accent / 発見性 | 1（+2 で book/dUNL） |
| 空状態・枠揃え | 4（触った箇所） |
| フォーカス/Esc 一貫性 | 触るパネルの作業に小さな別コミット可。**Phase 5 には混ぜない** |
| Row キャッシュ | 計画外（注記） |

除外: 新 RPC / 新 TX フロー / 設定スキーマ / 新パネル / 画像 / TEA 全面書き換え / 現行無関係 dirty tree 整理

---

## PR / 変更分割

**決定: 単一 PR**（ユーザー選択）。

緩和:

- PR 内は Phase 順のコミットに分ける（レビュー・revert 容易）
- コミットメッセージで Phase 番号を明示
- 巨大化したら実装中に PR 分割を再検討してよい（計画変更として記録）

---

## Risks & mitigations

| Risk | Mitigation |
|------|------------|
| 選択ロスト / TableState clone | ヘルパは `&mut SelectableTableState` のみ；スキルどおり clone 禁止 |
| stuck UI after dirty-render | 入力・リサイズ常時 dirty；主要データ Action で dirty；手動で全タブ操作確認 |
| 端末テーマ差 | RGB ブランド維持；白ハイライト例外をドキュメント化 |
| 単一 PR の巨大 diff | Phase 順コミット；レビューは Phase 単位 |
| 分割時のテスト迷子 | テストはファサード近傍に残す / 既存パスを維持 |
| 無関係リファクタ混入 | CHANGE_GUIDE: 関心ごとに分ける；この PR は ratatui 計画のみ |

---

## Verification

各 Phase コミット後（および PR 前）:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

最低限フォーカスするテスト（存在するもの）:

- テーブル選択サイクル / tx history / trust_lines・ledger・path・nft 近傍
- wallet payment validate / composer 系
- server / dUNL 近傍があれば

受け入れ（perf）: 主観スムーズさ + 既存 FPS 表示。数値ベンチは必須にしない。

---

## Out of scope（再掲）

- 画像統合（`ratatui-image`）
- TEA / `Action` 全面書き換え
- 新パネル・新 TX タイプ
- 現行 working tree の AGENTS/test/無関係差分整理（別 effort）
- Row キャッシュ実装（dirty 後の任意フォロー）

---

## Remaining fog（実装時に決めてよい細部）

- 兄弟ファイルの最終ファイル名（`wallet_composer.rs` 等は例）
- スクロールバーを「常時表示」か「行数閾値」か（既存互換優先で実装時判断）
- oracle / FTSO をヘルパ対象にするか（デフォルト: しない）
- dirty フラグの置き場（`App` vs `Tui`）の Exact 型

これらは Destination ブロッカーではない。
