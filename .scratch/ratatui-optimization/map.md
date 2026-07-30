# Map: Ratatui 活用・最適化

Labels: wayfinder:map

## Destination

実装に渡せる **方針マップ** を固める — 共有ウィジェット → 描画パフォーマンス → テーマ & Stylize → 巨大パネル分割の優先順で、境界・完了条件・フェーズ分けを決定し、**最終実装計画1本**にまとめる。この map では本番コード変更はしない。

**STATUS: destination reached** — 実装は [implementation-plan.md](implementation-plan.md) を後続セッションで実行。

## Notes

- Domain: lazyxrp TUI (`ratatui` 0.30 + TEA `Component` / `Action`)
- Skills: `.agents/skills/ratatui-tui`（`style-guide`, `architecture-patterns`, `async-patterns`）, `/grilling`, `/domain-modeling`
- Priority locked: **1 shared widgets → 2 draw perf → 3 theme → 4 panel split**
- Behavior: 機能改善も計画に含めてよい（ただし destination は方針決定；実装は後続）
- Out of this map's *execution*: no production code edits while charting/working tickets unless a `task` ticket explicitly requires a throwaway inventory artifact under `.scratch/`
- Tracker: local markdown (`.scratch/ratatui-optimization/`)

## Decisions so far

- [Research ratatui 0.30 render and table patterns](issues/02-research-ratatui-render-patterns.md) — 常時 Render interval vs `needs_render`+min-frame の根拠と、stateful table は clone 禁止。
- [Inventory shared-table duplication and draw hotspots](issues/01-inventory-shared-table-hotspots.md) — tx helper が金型；book/trust/ledger/path/server-dUNL/nft が同型コピー；毎 draw で Row 再生成；wallet/server の分割見出しを [assets/01…](assets/01-inventory-shared-table-hotspots.md) に記録。
- [Decide shared selectable-table helper contract](issues/03-shared-selectable-table-contract.md) — `selectable_table.rs` に関数 `render_*`；Phase-1=trust_lines/ledger_objects/path_find/nft+tx統一；併置UIは外；キー/選択は現状互換。
- [Decide dirty-render / frame policy](issues/04-dirty-render-policy.md) — `needs_render`+min-frame；入力/リサイズ常時dirty、Tick・データは変化時のみ；60維持；受け入れは主観+既存FPS。
- [Decide theme and Stylize policy](issues/05-theme-stylize-policy.md) — 現行RGB維持；白HIGHLIGHT例外；色はtheme経由；完了は段階移行。
- [Decide functional-improvement intake for the plan](issues/06-functional-improvement-intake.md) — UX一貫性のみ同梱；新RPC/TX/設定は除外；触るフェーズに紐づけ；候補は基準でfog卒業。
- [Decide wallet and server panel split boundaries](issues/07-wallet-server-split-boundaries.md) — 同階層ファイル分割；wallet=composer/keys/keygen/shell；server=detail/dUNL/metrics/shell；分割は構造のみ；DoD=ビルド+テスト+ファサード。
- [Enumerate UX improvement candidates for the plan](issues/09-enumerate-ux-improvement-candidates.md) — thumb=accent統一；空状態/枠はtheme；Escは触る範囲；発見性はshared；Rowキャッシュは注記のみ。
- [Draft final phased implementation plan](issues/08-final-implementation-plan.md) — 成果物 [implementation-plan.md](implementation-plan.md)；Phase-2=book→dUNL；FPSは変化時dirty；単一PR。**Destination 達成。**

## Not yet specified

- （Destination ブロッカーなし）実装時細部: 兄弟ファイル名、scrollbar 常時 vs 閾値、dirty フラグ置き場、oracle/FTSO を後で寄せるか — 詳細は [implementation-plan.md](implementation-plan.md) Remaining fog

## Out of scope

- 画像統合（`ratatui-image` / 端末グラフィックス）
- TEA / `Action` の全面書き換え（部分改善の方針検討は可）
- 新パネル追加・新 TX タイプ追加
- 現行 working tree の AGENTS / test / 無関係差分の整理（別effort）
