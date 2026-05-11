# AGENTS.md

## [CRITICAL] Execution Contract (Ambiguous Requests)

**Decision flow for every request:**
1. Check if the request lacks target files, reproduction conditions, or completion criteria.
2. If any are missing → ask 1-2 focused clarifying questions before implementation.
3. If sufficient → implement the change.
4. After implementation → run `cargo check`.
5. After `cargo check` → synchronize related `docs/`.

### Rules
- If a request lacks target files, reproduction conditions, or completion criteria, ask 1-2 focused clarifying questions before implementation.
- If immediate action is required and assumptions are unavoidable, state those assumptions explicitly and get agreement before proceeding.
  - "Immediate action" means the user explicitly uses urgency cues (e.g., "ASAP", "urgent", "blocking") or declines to answer clarifying questions.
  - When in doubt, always prefer asking clarifying questions over making assumptions.
- If you change key names or configuration behavior in `src/config.rs`, identify and synchronize related descriptions in `docs/`. "Related" means any doc file that mentions the changed key or describes the affected behavior.
- For implementation changes, run at least `cargo check`, and add additional tests when needed.

### Missing Information Template
Adapt the three items below to the request type. Skip any item that does not apply to the current request:
- Target: Which file, feature, or configuration key should be changed? (If the user cannot reasonably know this, e.g., for crashes, focus on reproduction instead.)
- Reproduction: What input/steps reproduce the issue, and what is expected vs actual behavior? (For new features: how will the feature be verified or tested? For changes where reproduction does not apply, such as config renames, skip this item.)
- Completion criteria: What defines done (tests, output, behavior)?
- To close gaps in a single round-trip, ask 1-2 focused questions total. One question is acceptable if it covers the most critical missing item. Combine multiple Missing Information items into a single compound question when needed. A compound question counts as one question regardless of how many items it covers.
- If the user prioritizes immediate handling, explicitly mark missing items as assumptions before implementation.
- Proactively mention docs/ synchronization and `cargo check` when the topic is clearly relevant (e.g., config changes), even before implementation begins. For docs sync, name specific files when possible; otherwise note that "related docs" will be reviewed.

## [REQUIRED] Prohibitions

- Do not include unrelated refactors or renames in the same change.
- Do not remove features just to bypass errors.
- Do not leave behavior that contradicts `docs/`.

## [REQUIRED] Development Policy

### Coding Style
- Follow Rust 2024 conventions.
- Run `cargo fmt` for formatting and `cargo check` for static verification.
- Use `tokio` for async processing.
- Keep UI layer responsibilities clearly separated. Detailed TUI patterns (poll loop, mpsc, component traits) are in the `xrpl-rust` skill.

### Commit Message Convention
- Use Conventional Commits.
- Example: `fix(xrpl): avoid tokio spawn lifetime issue in poll loop`

### Branching Strategy
- Follow GitHub Flow: branch from `main` and merge via pull requests.

### Version Control
- Use Git for version control.
- Do not run destructive operations (such as `reset --hard`) without explicit agreement.

## [REQUIRED] Test-Driven Development (TDD) Rules

- Add tests at the smallest practical unit for important logic.
- Run at least `cargo check` for each change.
- See `docs/test.md` for test policy, TC-ID case list, and TDD roadmap.

## [INFO] Directory Conventions

- See `docs/directory.md` for directory structure and ownership.

## [INFO] Important Configuration Files

- `Cargo.toml`: Defines dependencies, features, and profiles. State the rationale clearly when adding dependencies.
- `src/config.rs`: Handles default config loading and merge behavior. If key names change, update related docs at the same time.
- `docs/*.md`: Source of truth for specifications. Keep related docs synchronized when behavior changes.

## [INFO] Deployment Procedure

- See `README.md` for install and distribution steps.
- Release automation is tracked in `docs/tasks.md`.

## [INFO] Troubleshooting

- For unresolved `critical_section` symbols, verify that the `critical-section` crate has the `std` feature enabled.
- For unused code warnings, refer to W-001 in `docs/problems.md`.
- For XRPL crate-specific issues (lifetime bounds, currency types, error normalization), load the `xrpl-rust` skill.
