# Execution contract (ambiguous requests)

Supersedes nothing in root [`AGENTS.md`](../../AGENTS.md); that file holds the short checklist and **Prohibitions**.

**Decision flow for every request:**

1. Check if the request lacks target files, reproduction conditions, or completion criteria.
2. If any are missing → ask 1-2 focused clarifying questions before implementation.
3. If sufficient → implement the change.
4. After implementation → run `cargo check`.
5. After `cargo check` → synchronize related `docs/`.

## Rules

- If a request lacks target files, reproduction conditions, or completion criteria, ask 1-2 focused clarifying questions before implementation.
- If immediate action is required and assumptions are unavoidable, state those assumptions explicitly and get agreement before proceeding.
  - "Immediate action" means the user explicitly uses urgency cues (e.g., "ASAP", "urgent", "blocking") or declines to answer clarifying questions.
  - When in doubt, always prefer asking clarifying questions over making assumptions.
- If you change key names or configuration behavior in `src/config.rs`, identify and synchronize related descriptions in `docs/`. "Related" means any doc file that mentions the changed key or describes the affected behavior.
- For implementation changes, run at least `cargo check`, and add additional tests when needed.

## Missing information template

Adapt the three items below to the request type. Skip any item that does not apply to the current request:

- **Target**: Which file, feature, or configuration key should be changed? (If the user cannot reasonably know this, e.g., for crashes, focus on reproduction instead.)
- **Reproduction**: What input/steps reproduce the issue, and what is expected vs actual behavior? (For new features: how will the feature be verified or tested? For changes where reproduction does not apply, such as config renames, skip this item.)
- **Completion criteria**: What defines done (tests, output, behavior)?
- To close gaps in a single round-trip, ask 1-2 focused questions total. One question is acceptable if it covers the most critical missing item. Combine multiple missing-information items into a single compound question when needed. A compound question counts as one question regardless of how many items it covers.
- If the user prioritizes immediate handling, explicitly mark missing items as assumptions before implementation.
- Proactively mention `docs/` synchronization and `cargo check` when the topic is clearly relevant (e.g., config changes), even before implementation begins. For docs sync, name specific files when possible; otherwise note that "related docs" will be reviewed.
