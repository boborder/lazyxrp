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

## Post-release

- [ ] Tag matches `Cargo.toml` version
- [ ] `cargo install lazyxrp --locked` smoke test (optional)
