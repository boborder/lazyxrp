# Dependency Rules

> **Read**: `REPO_INVENTORY.md`, `ARCHITECTURE.md`, `DATA_MODEL.md`, `INVARIANTS.md`. **Scope**: full repository. **Confidence**: medium-high. **Generated**: 2026-05-14 (Pass 4).

## Observed Dependency Rules

### DR-1: Unidirectional message flow
`Action` flows top-down: background tasks → `action_rx` → `App` → components. Components never call into `xrpl/` directly.
**Enforcement**: Architectural — components receive data via `Action` variants, never hold `RpcClient`.
**Status**: ✅ Observed.

### DR-2: Components depend on action + config + xrpl/types
`components/` imports from `action`, `config` (for `Arc<Config>`), and `xrpl/types` (for row structs). Never from `xrpl/client`, `xrpl/poll`, or `app`.
**Status**: ✅ Observed.

### DR-3: xrpl/ is self-contained except for action + network + signing
`xrpl/` modules import `action::Action`, `network::Network`, `signing`. `types.rs` has zero internal dependencies.
**Status**: ✅ Observed.

### DR-4: config.rs depends on action + network
`config.rs` imports `Action` enum variants (for keybinding deserialization) and `Mode` (from `app`). This creates a loose coupling between config and app.
**Status**: ⚠️ Config depends on `app::Mode` — if `Mode` variants change, config deserialization breaks.

### DR-5: signing.rs has no UI dependencies
`signing.rs` depends only on `network`, `secrecy`, and `xrpl-rust` SDK. Clean separation from UI.
**Status**: ✅ Observed.

## Recommended Dependency Rules

### R-1: app::Mode should not be in config.rs dependency chain
**Current**: `config.rs` imports `crate::app::Mode` for keybinding scoping.
**Recommendation**: Move `Mode` to its own file or to `action.rs` so config doesn't depend on app internals.
**Priority**: Low (stable code, unlikely to change).

### R-2: poll.rs submit logic should be extractable
**Current**: ~800 lines of submit logic in `poll.rs`.
**Recommendation**: Consider extracting into `xrpl/submit.rs` if more transaction types are added.
**Priority**: Medium (growing complexity).

### R-3: No component should hold state derived from config after init
**Current**: Components receive `Arc<Config>` at init. Some may read config values during `draw()`.
**Recommendation**: Extract config-derived values once during `init()` and store locally.
**Priority**: Low (config is read-only at runtime).

## Violations

| Rule | Location | Severity | Description |
|------|----------|----------|-------------|
| DR-4 | `config.rs` → `app::Mode` | Low | Config depends on app internals for keybinding scoping |
| R-1 | same | Low | Fix would require refactoring Mode location |

## Coupling Hotspots

### Hotspot 1: `action.rs` — universal dependency hub
**Impact**: 70+ `Action` variants. Every module that sends/receives actions depends on this file. Adding a variant means touching `action.rs` + `app.rs` + potentially `poll.rs` + receiving component.
**Mitigation**: This is inherent to TEA architecture. Consider grouping actions into sub-enums if the enum exceeds ~100 variants.

### Hotspot 2: `poll.rs:submit_*` functions
**Impact**: Each submit function (~150-200 lines) duplicates: validation → simulate → extract → sign → submit → dispatch result. Code patterns converge but are not unified.
**Mitigation**: Extract common submit pipeline (validation + simulate + sign + submit) into a generic helper.

### Hotspot 3: `tx_detail/parsers.rs` — 29 transaction type parsers
**Impact**: Each parser function follows the `try_*_detail_lines()` → `*_detail_lines()` pattern. Adding a new transaction type requires new parser + new entry in `detail_lines_for()`.
**Mitigation**: Consider macro-based generation if the pattern is mechanically uniform.

## Shared/Common Risk Areas

### `src/components/shared/`
**Risk**: Low. Well-scoped shared utilities (theme, widgets, fps, splash, status_bar, help_overlay). No domain logic leakage.

### `src/components/shared/tx_detail/`
**Risk**: Low-Medium. Transaction detail rendering is centralized here. parsers.rs is ~29 functions with similar patterns. Risk of copy-paste errors in parser implementations.

### `src/xrpl/json_util.rs`
**Risk**: Low. Small utility file (`json_str`, `extract_json_u32`). Used across the codebase but unlikely to grow.

## Drift Risks

| Risk | Description | Likelihood |
|------|-------------|------------|
| Config drift | `config.rs` ~1000 lines — adding new config keys without updating precedence logic | Medium |
| Action bloat | Adding `Action` variants without considering filtering/routing impact | Low (70 variants stable) |
| Parser drift | Adding TX types to `tx_detail/parsers.rs` without updating `detail_lines_for()` matcher | Low (compile error) |
| Panel-tab mismatch | Adding panel without corresponding tab wiring | Medium (no runtime check) |

## Suggested Future Architecture Tests

1. **Module dependency test**: Use `cargo-modules` or similar to assert `components/` never imports `xrpl/client` or `xrpl/poll`.
2. **Config precedence test**: Assert exact config merge order for each config key (built-in → file → env → CLI).
3. **Tab-panel consistency test**: Assert `TAB_TITLES.len() == panels.len()` at runtime.
