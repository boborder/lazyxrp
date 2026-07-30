# Soft Spec — Session Intent (2026-07-30)

This is the originating intent for the **uncommitted working tree vs `main`**, not a formal PRD.

## Goals

### A. AGENTS.md progressive disclosure
1. Slim root `AGENTS.md` to essentials only (<50 lines): project blurb, commands, execution contract, critical overrides, links.
2. Keep detailed guidance in existing `docs/agent/*` (do not invent a new `.claude/` tree if `docs/agent/` already holds it).
3. Resolve documented contradictions against code (especially tab count / I-9).
4. Preserve unique workflow policy (Conventional Commits, GitHub Flow, Never-rules) by moving into `CHANGE_GUIDE.md` when removed from root.
5. All links from `AGENTS.md` must resolve; no dead skill paths.

### B. Test quality cleanup
1. Remove or replace tests that only mirror implementation (identity asserts, magic counts that drift with product).
2. Fix mislabeled / colliding TC IDs in code + `docs/test.md` (e.g. TC-014 stuck on Payment deserialize; TxDetail colliding with TC-070/071/073; Config env merge colliding with TC-075).
3. Strengthen weak asserts with real values/behavior (tab construction, TabPrev wrap, account_objects field details, date epoch formatting, truncate length contract).
4. Keep `docs/test.md` catalog consistent: unique headings, matching counts, expected outputs matching actual `PollCommand` / 4-tab behavior.

### C. Agent docs drift fixes tied to the above
1. `INVARIANTS.md` I-9 / UA-1: 4 tabs + assert (not stale “5 / no assert”).
2. `DESIGN_ISSUES.md` / `DEPENDENCY_RULES.md` aligned with resolved tab-panel guard.
3. Do not contradict `docs/` when changing behavior.

## Explicit non-goals (this session)
- Full ratatui widget refactor / shared-table extraction / panel splits (discussed, not required as delivered scope unless present in the diff).
- Committing or releasing.
- Rewriting product FRs in `docs/requirements.md` unless the diff claims a feature change.

## Acceptance signals
- `AGENTS.md` short, linked, no dead links.
- Tests assert behavior, not implementation mirrors; TC catalog IDs unique and synced.
- Invariants/docs match 4-tab reality.
- `cargo test` relevant suites still pass for touched tests.
