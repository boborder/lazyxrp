# Release checklist

Use this before tagging a public release or publishing to crates.io / GitHub Releases.

## Build & tests

```bash
cargo fmt --check
cargo clippy --locked -- -D warnings   # optional but recommended
cargo check --locked
cargo test --locked
```

Expected: all tests pass (ignored tests are network/seed-dependent).

## Secrets & local files

- [ ] `.env` is **not** tracked (`git check-ignore -v .env` should match)
- [ ] No seeds, private keys, or RPC tokens in committed files
- [ ] `.env.example` contains placeholders only (no real `XRPL_SEED`)
- [ ] Rotate any seed that was ever committed or shared in chat/logs

## Documentation sync

After behavior changes, update:

| Change type | Update |
|-------------|--------|
| UX / tabs / panels | `README.md`, `docs/design.md`, `docs/directory.md` |
| Config keys | `docs/tech.md`, `docs/design.md`, every doc that mentions the key |
| New `Action` | `docs/agent/ARCHITECTURE.md`, root `AGENTS.md` if contract changes |
| Security | `docs/security.md`, `docs/agent/RISK_REGISTER.md` |

Index: [`docs/README.md`](README.md).

## Repository hygiene

- [ ] `LICENSE` present (MIT)
- [ ] `Cargo.lock` committed; CI uses `--locked`
- [ ] `graphify-out/` not committed (gitignored); run `graphify update .` after large refactors if agents rely on the graph
- [ ] Remove dead code (e.g. orphan `src/utils.rs` if it reappears — logic lives in `xrpl/client.rs`)

## Distribution

- [ ] `install.sh` tested on a clean machine or CI artifact
- [ ] Version bumped in `Cargo.toml` before tag
- [ ] GitHub Release notes mention breaking changes and mainnet `--yes` requirement

### Automated release (Cargo.toml version bump)

When a push to `main` bumps `Cargo.toml` `version` and CI (`test`, `rustfmt`, `clippy`, `docs`) is green, `auto-tag` creates `v<version>` and [`.github/workflows/cd.yml`](../.github/workflows/cd.yml) builds binaries + publishes GitHub Release / crates.io.

`src/`, `Cargo.lock`, docs, and workflow changes are all allowed — the gate is **version string change only** (plus CI green / tag not already present).

**Note:** A tag push made with the default `GITHUB_TOKEN` does **not** start other workflows. `auto-tag` therefore also `workflow_dispatch`es `CD` on `v<version>` after pushing the tag.

**Force / initial push skip:** If `github.event.before` is all-zero (`0000…0`) — first push of a branch history or a **force-push** — `auto-tag` **silently skips** (it cannot diff `Cargo.toml` against a previous SHA). A version bump that lands only via force-push will **not** create `v<version>`. Use a normal fast-forward push to `main`, or fall back to `mise run tag-push`.

Typical flow:

1. Bump `version` in `Cargo.toml` (and sync docs / lockfile as needed).
2. Merge / push to `main` (**fast-forward**; avoid force-pushing the bump commit).
3. CI runs, then `auto-tag` pushes `v<version>` and dispatches CD.
4. CD builds binaries + publishes GitHub Release / crates.io.

Fallback if CI is red, auto-tag skipped (force/initial push), or you need to retag carefully: `mise run tag-push` (reads `Cargo.toml`, creates and pushes that single tag with **your** credentials so the natural `push.tags` CD trigger fires).

Troubleshooting CI/CD jobs when `gh run view --log` is forbidden: use `gh api repos/{owner}/{repo}/actions/runs/{id}/jobs`.

## Post-release

- [ ] Tag matches `Cargo.toml` version
- [ ] `cargo install lazyxrp --locked` smoke test (optional)
