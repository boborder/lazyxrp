---
name: ratatui-tui
description: |
  Build terminal UIs with ratatui (v0.30+) and crossterm using 2026 Rust patterns.
  WHEN: (1) new TUI / templates, (2) Layout StatefulWidget widgets, (3) keyboard +
  focus routing, (4) ratatui-image / sixel / kitty, (5) async EventStream + tokio
  select!, (6) release LTO/strip/panic=abort and portable deps.
  KEYWORDS: ratatui, ratatui-image, TUI, crossterm, EventStream, raw mode,
  alternate screen, panic hook, color-eyre, Stylize, Layout Constraint Fill,
  StatefulWidget, chafa-static, Elm TEA Model-Message-Update, tokio, select!.
---
# Ratatui TUI Development

## Reference contract (read before coding)

Pick **mandatory** references by what you are changing. Skipping them wastes tokens on
wrong API and ships broken terminals.

| You are doing | MUST read (full file) | Do **not** load unless needed |
|---------------|------------------------|--------------------------------|
| Any styling / theme / `Stylize` / colors | [`references/style-guide.md`](references/style-guide.md) | — |
| App shape: TEA, components, actions, modals, focus | [`references/architecture-patterns.md`](references/architecture-patterns.md) | — |
| `EventStream`, `select!`, ticks, background tasks, async I/O | [`references/async-patterns.md`](references/async-patterns.md) | — |
| `ratatui-image`, `Picker`, sixel/kitty, resize | [`references/image-integration.md`](references/image-integration.md) | — |
| Sync-only loop, tiny tool, no async | `architecture-patterns` + `style-guide` | `async-patterns.md`, `image-integration.md` |

**Rule:** If the task mentions **images** / `ratatui-image` / terminal graphics → read
`image-integration.md` end-to-end. If it does **not**, do not pull image content into
context.

## Expert mental model (think first)

- **Terminal is shared global state.** Raw mode + alternate screen + cursor/hide are
  *leases*: exit paths (OK, `Err`, panic) must **restore** or the user's shell stays
  unusable. Prefer `ratatui::init()` / `ratatui::restore()` *or* mirror the same steps
  in a panic hook + `Drop`.
- **Render is cheap; encoding images is not.** Query terminal protocol **once**;
  heavy work off the hot path. Wrong layering starves input or pegs CPU.
- **TEA fits TUI:** one `Message` type, pure `update`, dumb `view`. Keeps redraws
  predictable and tests easier. Full patterns → `architecture-patterns.md`.

## NEVER (hard stops) + why

| NEVER | Why it bites |
|------|----------------|
| `Style::default().fg(Color::White)` / `Black` for “normal” text | Breaks light/dark themes; users get unreadable contrast. |
| Long `Style::new().add_modifier(...)` when `Stylize` works | Noise + drift from project style; `.bold().cyan()` stays consistent. |
| `Picker::from_query_stdio()` or `new_protocol` **every frame** | Stall + flicker; protocol query is startup-only. |
| `Image` widget for a **persistent** panel | Re-encodes each draw → CPU. Use `StatefulImage` + cached protocol. |
| Async loop with **only** `events.next()` in `select!` and no tick/channel arms | Resize and background work can starve or never run; structure `select!`. |
| Leave raw mode / alt screen on panic or early `return` without restore | Terminal stays “broken” until `reset`. Always pair with panic hook + `restore`. |
| `unwrap()` in TUI hot path on user/network I/O | Turns recoverable issues into crash + broken tty. |

Expanded style bullets → [`references/style-guide.md`](references/style-guide.md).
Async starvation / `select!` → [`references/async-patterns.md`](references/async-patterns.md).

## Quick start

1. **Copy template** into the project:
   ```bash
   cp -r ~/.agents/skills/ratatui-tui/assets/templates/<template>/* .
   ```
2. **Run:** `cargo run`

## Template selection

| Complexity | Template | Use case |
|------------|----------|----------|
| Minimal | `hello-world` | Learning, demos |
| Simple | `simple-app` | Single-screen CLI tools |
| Async | `async-app` | Network / timers / background work |
| Full | `component-app` | Multi-panel, config, tracing |

**Decision tree:** async or network → `async-app`; several panels/routes →
`component-app`; small sync tool → `simple-app`; first contact → `hello-world`.

## Project setup

**Minimal** (`edition` may be `2021` / `2024` per your repo):

```toml
[dependencies]
ratatui = "0.30"
crossterm = "0.29"
color-eyre = "0.6"
```

**Async + component-style** (enable stream driver):

```toml
crossterm = { version = "0.29", features = ["event-stream"] }
tokio = { version = "1", features = ["full"] }
futures = "0.3"
```

**Optional images** (portable binary → static chafa):

```toml
ratatui-image = { version = "5", features = ["chafa-static"] }
image = "0.25"
```

## Core architecture (TEA + loop)

Flow: **Model → Message → update → view → draw**. Keep `update` free of I/O; push side
effects to commands / async tasks / channels.

Worked examples, component traits, modal priority, action channels →
[`references/architecture-patterns.md`](references/architecture-patterns.md).

## Styling (high signal)

- Import `Stylize` and style spans/lines with `"text".bold().cyan()` etc.
- Semantic colors: primary `.cyan()` / `.green()`, errors `.red()`, muted `.dim()`.
- Wrap with `textwrap` + map to `Line` for paragraphs.

Full cheatsheet + tables → [`references/style-guide.md`](references/style-guide.md).

## Widgets and layout (ratatui 0.30)

- **Stateful:** `ListState` / `TableState` / `ScrollbarState` with
  `render_stateful_widget`.
- **Layout:** `Layout::vertical([...]).areas(rect)` / `horizontal` — prefer
  `Constraint::Length` + `Fill(1)` over deep `%` stacks unless you need proportional
  splits.

Custom `StatefulWidget` + `Component` trait examples → `architecture-patterns.md`.

## Async input loop

When using `EventStream`, **also** read [`references/async-patterns.md`](references/async-patterns.md)
(top to bottom). Minimum sketch:

```rust
use crossterm::event::{EventStream, Event, KeyCode};
use futures::StreamExt;
use tokio::select;

async fn run(mut app: App) -> color_eyre::Result<()> {
    let mut events = EventStream::new();
    loop {
        terminal.draw(|f| app.view(f))?;
        select! {
            Some(Ok(event)) = events.next() => {
                if let Event::Key(key) = event {
                    // map KeyCode -> Message
                }
            }
            // tick, background rx, shutdown — see async-patterns.md
        }
        if app.should_quit { break; }
    }
    Ok(())
}
```

## Images (`ratatui-image`)

Only when the feature is in scope: read [`references/image-integration.md`](references/image-integration.md).

**Invariants:** query protocol once; encode off-thread or `spawn_blocking`; cache
`StatefulProtocol`; re-encode on terminal resize when the image area changes.

## Error handling and terminal restore

```rust
use color_eyre::eyre::Result;

fn main() -> Result<()> {
    color_eyre::install()?;
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen
        );
        original_hook(info);
    }));
    run()
}
```

Use `?` + `.wrap_err("context")` on recoverable paths.

## Release binary

```toml
[profile.release]
lto = true
codegen-units = 1
panic = "abort"
strip = true
# opt-level = "z"   # optional: smaller binary over speed
```

```bash
cargo build --release
```

Artifact: `target/release/<name>`.

## Templates (overview)

| Template | ~lines | Notes |
|----------|--------|-------|
| `hello-world` | ~25 | `ratatui::run()` demo |
| `simple-app` | ~80 | sync loop |
| `async-app` | ~120 | tokio + `EventStream` |
| `component-app` | ~300 | `app` / `event` / `ui` / `tui` / `config` / `logging` |

## Common snippets

**Centered popup**

```rust
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let [_, center, _] = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ]).areas(area);
    let [_, center, _] = Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ]).areas(center);
    center
}
```

**Key help line**

```rust
let help = Line::from(vec![
    " q ".bold().cyan(),
    "quit ".dim(),
    " ↑↓ ".bold().cyan(),
    "navigate ".dim(),
]);
```

## Ship checklist

- [ ] `cargo fmt`
- [ ] `cargo clippy --all-features`
- [ ] No `unwrap()` outside tests / truly infallible spots
- [ ] Panic path restores terminal; happy path calls `restore`
- [ ] `cargo build --release`
- [ ] Smoke-test in target terminals (light + dark)
