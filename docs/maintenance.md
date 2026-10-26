# maintenance.md

[日本語](maintenance-ja.md)

**Role:** Operations — release and publishing flow. Development commands are in [`tech.md`](tech.md) §5, app behavior in [`architecture.md`](architecture.md), threats in [`security.md`](security.md).

## 1. Publishing (release) flow

Publishing is a tag-driven automated release. `version` in `Cargo.toml` is the SSOT.

**`Cargo.toml` version detection by CI ([ci.yml](../.github/workflows/ci.yml), two places):**

- `release-gate` (Version & Lock Consistency): on every push / PR, checks that `version` in `Cargo.toml` and `Cargo.lock` match, and fails fast before the full test matrix runs if they differ. **Push a version bump in the same commit as the `Cargo.lock` sync.**
- `auto-tag` (Auto Tag Release): on a `main` push, after all of test / msrv / rustfmt / clippy / docs / security-advisories / package / check-os are green, compares `version` in `Cargo.toml` with the latest `v*` tag and, if it is higher, creates and pushes the `v<version>` tag. A tag pushed with GITHUB_TOKEN does not trigger other workflows, so it then dispatches CD explicitly with `gh workflow run CD --ref v<version>` (known behavior, noted in a comment in the workflow).
- `package`: `cargo package --locked` (verifies at PR time that the crates.io distribution builds from the `include` allowlist alone). `check-os`: `cargo check --locked --all-targets` on macOS / Windows (compile check for the release target OSes). Both are `needs` of auto-tag.
- Workflows keep top-level `permissions` minimal (`contents: read`; cd.yml also `actions: read`), and only the jobs that need to write (auto-tag / publish-github-release / trigger-benchmark) escalate individually. Checkouts that do not push use `persist-credentials: false`.
- `msrv`: `cargo +1.94.1 check --locked --all-targets`. When raising `rust-version` in `Cargo.toml`, update this job at the same time.

**CD (publishing, [cd.yml](../.github/workflows/cd.yml) — triggered by tag push / workflow_dispatch):**

| Job | What it does |
|---|---|
| `ci-gate` | Watches the CI run of the tagged commit until it completes; aborts the release if it is not green |
| `build-binaries` | Builds 6 targets (macOS x86_64/arm64, Linux x86_64/aarch64/i686, Windows x86_64 MSVC) with `--locked --release`. `lazyxrp-<tag>-<os>-<arch>.tar.gz` + `.sha256` (bundles `rp`) |
| `publish-github-release` | Attaches the full set of tarballs to the GitHub Release. Body = commit subjects since the previous tag (`git log`) + GitHub-generated notes (PR label categories in `.github/release.yml`) |
| `publish-cargo` | `cargo package --locked` (verification without the token) → `cargo publish --locked --no-verify` (crates.io, `CARGO_REGISTRY_TOKEN`). The archive contents are determined by `include` in `Cargo.toml` (the crates.io allowlist) |
| `trigger-benchmark` | After a successful release, dispatches the Benchmark workflow with `gh workflow run Benchmark -R "$GITHUB_REPOSITORY" --ref v<version>` (`-R` is explicit because this job has no checkout). It does not wait on `workflow_run`: it failed to fire 5 times in a row (v0.2.3–v0.2.7) for a GITHUB_TOKEN-originated CD |

**Manual fallback:** only when the automated path is broken, `mise run tag-push` (the 4 gates lint / test / doc / audit → create and push the `v<version>` tag; the tag push itself starts `cd.yml`).

**Before a release (required if you changed the write path):** `mise run e2e-testnet` (`tests/testnet_e2e.rs`; confirms send / offer-create / offer-cancel / NFT mint → burn through to validated using the public faucet + testnet; the 2 tests run serially with `--test-threads=1`, about 40 s). If the testnet open-ledger fee spikes, simulate fails with `exceeds the requested tx limit`. Check with the `fee` RPC (fetch via `ctx_execute`) and substitute `LAZYXRP_E2E_NETWORK=devnet mise run e2e-testnet` (run note in docs/test.md). Not run in CI. In 0.3.0, every write was broken by a difference between mock and real-node `simulate` responses (TC-200).

**Benchmark:** `benchmark.sh` (`--json` / `--ci` / `--fast`; `mise run bench` / `bench-fast` / `bench-ci`) runs build, check, test, clippy, doc, startup time (hyperfine), binary size (cargo-bloat) and `cargo --timings` independently, each with its own timeout (one failure stops the others only with `--ci`). `.github/workflows/benchmark.yml` is `workflow_dispatch` only (not run on main push / PR). `strict=true` on a manual dispatch passes `--ci`. Artifact `benchmark-results` (`benchmark-summary.json`, `benchmark.log`, `target/bloat-report.txt`, `target/timings-report.html`). `--json` is `{benchmarks:[{name,result,value,unit,message}], summary:{pass,fail,skip,timestamp}, cache_comparison}`, with `result` one of `PASS` / `FAIL` / `TIMEOUT` / `SKIP`.

**Changelog:** for user-visible changes, add an entry under `## [Unreleased]` in `CHANGELOG.md` in the same commit as the change.

**Docs are bilingual:** every `docs/*.md` and `ROADMAP.md` has a Japanese twin `*-ja.md`; the English file is canonical for agents. Any change to one must be made to the other in the same change, and the ID sets (TC/FR/NFR/S/R/I/P/F) must match. There is no automated check; compare the ID sets of `X.md` and `X-ja.md` when editing (e.g. `grep -o "TC-[0-9]*"` on both).

**Normal release procedure:** `mise run release <x.y.z>` (checks main, clean worktree, unused tag, and that `[Unreleased]` has entries → `Cargo.toml` bump + `Cargo.lock` sync + finalizes `[Unreleased]` in `CHANGELOG.md` as `[x.y.z] - date` and updates the compare links → lint / test → one `chore: release <x.y.z>` commit. It does not push) → `git push` → all CI green → auto-tag creates the tag → CD publishes (rule of thumb: about 11 minutes from push to crates.io; CI about 1 min, build 4–6.5 min, crates.io publish about 3.5 min). A manual tag is generally unnecessary because auto-tag detects the duplicate and skips.

**crates.io package:** `include` in `Cargo.toml` is the allowlist (`src/`, `tests/`, `build.rs`, `config.json5`, `DESIGN.md` (read by TC-128), `CHANGELOG.md`, `SECURITY.md`, `LICENSE`, `README.md`, `ROADMAP.md` (read by TC-248), `.env.example`). `docs/` is on GitHub only. Check: `cargo package --list` (must show 0 warnings).

**Scheduled CI:** runs the whole CI every Monday 00:00 UTC to catch yanked crates and new advisories before a push (the yanked `yoke-derive` on 2026-10-03 first failed at push time). auto-tag runs only on push.

**Dependency updates:** [`.github/dependabot.yml`](../.github/dependabot.yml) opens PRs weekly. GitHub Actions are pinned to commit SHAs (with `# vN` comments), so their updates also come through these PRs. Cargo gets one PR grouping minor/patch; majors are separate. Exception: `tokio-tungstenite >=0.25` is ignored by dependabot and also fails via `[bans] deny` in `deny.toml` (`tungstenite` / `tokio-tungstenite` `>=0.25`), which prevents duplicating the xrpl-rust 0.24 tree. Remove both once xrpl-rust moves up.
