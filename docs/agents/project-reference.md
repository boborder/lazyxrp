# Project reference

## Directory conventions

- See `docs/directory.md` for directory structure and ownership.

## Important configuration files

- `Cargo.toml`: Defines dependencies, features, and profiles. State the rationale clearly when adding dependencies.
- `src/config.rs`: Handles default config loading and merge behavior. If key names change, update related docs at the same time.
- `docs/*.md`: Source of truth for specifications. Keep related docs synchronized when behavior changes.

## Deployment procedure

- See `README.md` for install and distribution steps (`./install.sh`, optional [`mise run install`](https://mise.jdx.dev/) via `.mise.toml`).
- Release automation is tracked in `docs/tasks.md`.

## Troubleshooting

- For unresolved `critical_section` symbols, verify that the `critical-section` crate has the `std` feature enabled.
- For unused code warnings, refer to W-001 in `docs/problems.md`.
- For XRPL crate-specific issues (lifetime bounds, currency types, error normalization), load the `xrpl-rust` skill.
