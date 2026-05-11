# Development policy

## Coding style

- Follow Rust 2024 conventions.
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
