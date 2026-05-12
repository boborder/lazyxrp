# Development policy

## Coding style

- Follow Rust 2024 conventions.
- Rust channel is set in `rust-toolchain.toml` (CI/CD and local `rustup` use the same channel; currently `stable`). `Cargo.toml` has `rust-version` as the MSRV hint for dependents.
- Commit `Cargo.lock` and use `cargo … --locked` in CI so dependency resolution matches across machines.
- Run `cargo fmt` for formatting and `cargo check` for static verification.
- Use `tokio` for async processing.
- Keep UI layer responsibilities clearly separated. Detailed TUI patterns (poll loop, mpsc, component traits) are in the `xrpl-rust` skill.

## Commit message convention

- Use Conventional Commits.
- Example: `fix(xrpl): avoid tokio spawn lifetime issue in poll loop`

## Branching strategy

- Follow GitHub Flow: branch from `main` and merge via pull requests.

## Version control

- Use Git for version control.
- Do not run destructive operations (such as `reset --hard`) without explicit agreement.
