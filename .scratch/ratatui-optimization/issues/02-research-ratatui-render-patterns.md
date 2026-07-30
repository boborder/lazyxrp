# Research ratatui 0.30 render and table patterns

Type: research
Status: resolved
Labels: wayfinder:research
Blocked by:

## Question

公式 / スキル資料から、lazyxrp に効く **描画スロットリング** と **stateful table** の推奨パターンを摘む。

読む場所（優先）:

- `.agents/skills/ratatui-tui/references/async-patterns.md`（Rate Limiting Renders / select!）
- `.agents/skills/ratatui-tui/references/architecture-patterns.md`（Component / layout footguns）
- 必要なら ratatui 0.30 公式の Frame / stateful widget ガイド

答えに含めること:

- `needs_render` + min frame duration と、現状の常時 `Event::Render` interval のトレードオフ
- stateful table で避けるべきこと（state clone 等）
- lazyxrp の `tui.rs` / `Component` に当てはめるときの注意点（結論は次チケット、ここでは根拠）

## Answer

Asset: [`.scratch/ratatui-optimization/assets/02-ratatui-render-patterns.md`](../assets/02-ratatui-render-patterns.md)

**Tradeoff:** lazyxrp は `tui.rs` で常時 `Event::Render` interval（既定 ~60Hz）— 単純だがアイドルでもフルツリー描画。スキル推奨は `needs_render` + `MIN_FRAME_DURATION`（~16ms）でゲート — CPU は下がるが、dirty 付け忘れで UI 固着のリスク。FPS/Splash は明示的 dirty か別経路が必要。

**Stateful tables:** `render_stateful_widget` に **clone した** `TableState`/`ListState` を渡さない。`Component::draw(&mut self)` で同一 state を `&mut` する。0×0 `Rect` / `Fill` 崩壊に注意。

**lazyxrp note:** 方針決定は [Decide dirty-render / frame policy](04-dirty-render-policy.md) へ。共有テーブル API は state を clone しない契約にすること。
