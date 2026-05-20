# Research notes: ratatui 0.30 render + stateful tables (lazyxrp)

Sources:

- `.agents/skills/ratatui-tui/references/async-patterns.md` — Rate Limiting Renders; select! footguns
- `.agents/skills/ratatui-tui/references/architecture-patterns.md` — Layout footguns; Component `&mut self` for stateful widgets
- `src/tui.rs` — always-on `tick_interval` + `render_interval` → `Event::Tick` / `Event::Render`

## Always-on Render interval (current lazyxrp)

`Tui::event_loop` fans out crossterm + tick + render on intervals (default frame_rate 60 → ~16ms). App draws on `Event::Render` regardless of whether model changed.

**Pros:** simple; animations/FPS counter stay live; no missed redraw if a code path forgets to set dirty.

**Cons:** skill explicitly warns against busy/ungated draws (CPU); idle watch still paints full tree (tabs + active panels + status) at 60Hz.

## needs_render + MIN_FRAME_DURATION (skill recommendation)

Pattern: draw only when `needs_render && last_render.elapsed() >= ~16ms`; set dirty from input / tick / background results that actually change view.

**Pros:** idle CPU drop; still caps at ~60fps when busy.

**Cons:** every state-changing path must mark dirty (XRPL Actions, keymap, resize, help overlay, composer typing); FPS/splash animations need an explicit dirty or a cheap always-on path; easy to ship “stuck UI” bugs.

## Stateful table footguns

- Never pass a **cloned** `ListState`/`TableState` into `render_stateful_widget` — selection breaks; keep one authoritative state, render with `&mut`.
- Component `draw`/`render` takes `&mut self` for that reason.
- Zero-sized `Rect` / collapsing `Fill` chains: skip or fallback when width/height is 0.

## Implication for later tickets (not decided here)

- Dirty-render policy ticket should choose: keep interval Render vs gate draws; if gating, define dirty sources (`Action::Xrpl*`, keys, resize, tick-driven UI).
- Shared table helper must take `&mut SelectableTableState` (or inner `TableState`), never clone-for-render.
