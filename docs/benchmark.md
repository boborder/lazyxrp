# Benchmark Suite

## Overview

The benchmark suite measures build, check, test, startup performance, binary size, and build-profile timings for `lazyxrp`. It is designed to run both locally and in CI, with per-step timeouts and independent failure handling.

**Tools used:**
- `hyperfine` — statistical startup-time benchmarks (mean + σ over 3 runs)
- `cargo-bloat` — per-crate binary size breakdown
- `cargo build --timings` — HTML build-profile report per crate

Install them locally via `cargo install`:

```bash
cargo install hyperfine cargo-bloat
```

## Running Locally

```bash
# Full suite (interactive, non-strict)
./benchmark.sh

# With JSON output (last line is parseable JSON)
./benchmark.sh --json

# CI mode — fail on any timeout or error
./benchmark.sh --ci

# Fast mode — skip clean release build
./benchmark.sh --fast

# Combined
./benchmark.sh --json --ci
```

Or via **mise**:

```bash
mise run bench       # full suite (~10 min)
mise run bench-fast  # skips clean build (~3 min)
```

## Measured Checks

| Check | What it measures | Tool | Timeout |
|-------|------------------|------|---------|
| `clean_release_build` | `cargo build --locked --release` from `cargo clean` | bash | 10 min |
| `warm_release_build` | `cargo build --locked --release` with warm cache | bash | 10 min |
| `incremental_build` | `cargo build --locked --release` after `touch src/main.rs` | bash | 3 min |
| `cargo_check` | `cargo check --locked --all-features --workspace` | bash | 2 min |
| `cargo_test` | `cargo test --locked --all-features --workspace` | bash | 5 min |
| `release_binary_size` | Size of `target/release/lazyxrp` in bytes | bash | — |
| `cargo_bloat` | Top 20 size contributors by crate / function | cargo-bloat | — |
| `help_startup` | `lazyxrp --help` startup time (mean of 3 runs) | hyperfine | 15 s |
| `version_startup` | `lazyxrp --version` startup time (mean of 3 runs) | hyperfine | 15 s |
| `clippy_check` | `cargo clippy --locked --all-targets --all-features --workspace -- -D warnings` | bash | 2 min |
| `doc_build` | `cargo doc --locked --no-deps --document-private-items --all-features --workspace` | bash | 2 min |
| `cargo_timings` | `cargo build --release --timings` HTML report | cargo | 10 min |

Each check runs independently — a timeout or failure in one does not block the others unless `--ci` is passed.

### Cache Comparison

If both `clean_release_build` and `warm_release_build` pass, the script prints a **cold vs warm** comparison showing how much time dependency caching saves:

```
📦 Cache Comparison

  Cold build (clean):      430.309s
  Warm build (cached):       1.028s
  Time saved:            429.281s (90.0%)
```

This is especially useful on CI when evaluating `Swatinem/rust-cache` effectiveness.

### Build Profile (`cargo --timings`)

The `cargo_timings` check generates an HTML report (`target/timings-report.html`) that visualises compile time per crate. Open it in a browser to spot slow dependencies.

### Size Breakdown (`cargo-bloat`)

The `cargo_bloat` check writes a text report (`target/bloat-report.txt`) listing the top 20 size contributors by crate and function. Use it to identify unexpectedly large dependencies.

## CI Integration

The `.github/workflows/benchmark.yml` workflow runs on:

- After a **successful CD** run (`workflow_run` on `CD # Continuous Deployment`)
- Manual dispatch via **Actions → Benchmark → Run workflow**

It no longer runs on every `main` push or pull request (those were noisy relative to release signal). CD-chained runs check out the CD head SHA so numbers match the released commit.

Results are rendered as a job summary (viewable on the workflow run page) and uploaded as an artifact (`benchmark-results`) that includes:

- `benchmark.json` — structured results
- `target/bloat-report.txt` — size breakdown
- `target/timings-report.html` — build-profile visualisation

### Strict Mode

When triggered manually with **strict = true**, the workflow passes `--ci` to the script and fails the job if any benchmark step times out or errors. This is useful for gating releases or large refactors.

### Interpreting Results

- **Build times** are the primary signal for refactoring impact. Compare `clean_release_build` and `incremental_build` before/after a change.
- **Startup times** (`help_startup`, `version_startup`) show mean ± σ from hyperfine — catch binary bloat or runtime initialisation regressions.
- **Binary size** is reported in human-readable form (e.g., `9.4MiB`) and raw bytes. Pair it with `cargo_bloat` for root-cause analysis.
- **Test / check / clippy / doc** times help identify toolchain or dependency slowdowns.
- **cargo_timings** HTML report reveals which crate is the compilation bottleneck.

## JSON Schema

The `--json` output is a single JSON object:

```json
{
  "benchmarks": [
    {
      "name": "clean_release_build",
      "result": "PASS",
      "value": 45.123,
      "unit": "s",
      "message": "ok"
    }
  ],
  "summary": {
    "pass": 10,
    "fail": 0,
    "skip": 0,
    "timestamp": "2026-05-14T10:00:00+09:00"
  },
  "cache_comparison": {
    "cold": 430.309,
    "warm": 1.028,
    "saved_seconds": 429.281,
    "saved_percent": 90.0
  }
}
```

`result` is one of `PASS`, `FAIL`, `TIMEOUT`, or `SKIP`.
