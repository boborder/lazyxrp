#!/usr/bin/env bash
# benchmark.sh — lazyxrp build & performance benchmark suite
# Usage: ./benchmark.sh [--json] [--ci] [--fast]
#   --json   Emit results as JSON to stdout (last line)
#   --ci     Strict mode: fail on any step timeout/error
#   --fast   Skip the clean release build (use when only checking warm builds)
#
# Tools: hyperfine (startup stats), cargo-bloat (size breakdown), cargo --timings (build profile)
#   Install: cargo install hyperfine cargo-bloat

set -euo pipefail

cd "$(dirname "$0")"

# ── Config ─────────────────────────────────────────────────────────
readonly TIMEOUT_BUILD="600"
readonly TIMEOUT_CHECK="180"
readonly TIMEOUT_TEST="300"
readonly TIMEOUT_CLIPPY="120"
readonly TIMEOUT_DOC="120"
readonly TIMEOUT_STARTUP="15"
readonly TIMEOUT_INCREMENTAL="180"

readonly CHECKS=(
  "clean_release_build"
  "warm_release_build"
  "incremental_build"
  "cargo_check"
  "cargo_test"
  "release_binary_size"
  "cargo_bloat"
  "help_startup"
  "version_startup"
  "clippy_check"
  "doc_build"
  "cargo_timings"
)

# ── State ──────────────────────────────────────────────────────────
declare -A RESULTS
declare -A VALUES
declare -A UNITS
declare -A MESSAGES
FAILED=0
JSON_MODE=false
CI_MODE=false
FAST_MODE=false

# ── Helpers ────────────────────────────────────────────────────────
log() { printf '%s\n' "$*"; }
hr()  { printf '─%.0s' $(seq 1 70); printf '\n'; }

# Run a command with timeout, capturing elapsed time (seconds.millis)
run_timed() {
  local name="$1" timeout_sec="$2" desc="$3"
  shift 3
  log "▶ $desc …"

  local start end elapsed tmpout rc
  start=$(date +%s%N)
  tmpout=$(mktemp)

  # shellcheck disable=SC2086
  timeout --foreground "$timeout_sec" "$@" >"$tmpout" 2>&1 || rc=$?
  end=$(date +%s%N)
  elapsed=$(echo "scale=3; ($end - $start) / 1000000000" | bc | sed 's/^\./0./')

  if [[ "${rc:-0}" -eq 124 ]]; then
    RESULTS[$name]="TIMEOUT"
    VALUES[$name]="$timeout_sec"
    MESSAGES[$name]="exceeded ${timeout_sec}s timeout"
    log "  ⚠ TIMEOUT ($elapsed s)"
    FAILED=$((FAILED + 1))
  elif [[ "${rc:-0}" -ne 0 ]]; then
    RESULTS[$name]="FAIL"
    VALUES[$name]="$elapsed"
    MESSAGES[$name]=$(head -n3 "$tmpout" | tr '\n' '; ')
    log "  ✗ FAIL ($elapsed s)"
    tail -n5 "$tmpout" >&2 || true
    FAILED=$((FAILED + 1))
  else
    RESULTS[$name]="PASS"
    VALUES[$name]="$elapsed"
    MESSAGES[$name]="ok"
    log "  ✓ PASS ($elapsed s)"
  fi
  rm -f "$tmpout"
}

# Run hyperfine (startup / micro benchmarks) for statistical accuracy
run_hyperfine() {
  local name="$1" timeout_sec="$2" desc="$3"
  shift 3
  log "▶ $desc …"

  if ! command -v hyperfine >/dev/null 2>&1; then
    RESULTS[$name]="SKIP"
    VALUES[$name]="0"
    MESSAGES[$name]="hyperfine not installed"
    log "  ⊘ SKIP (hyperfine not installed)"
    return
  fi

  local tmpout rc start end elapsed tmpjson
  tmpout=$(mktemp)
  tmpjson=$(mktemp)
  start=$(date +%s%N)

  # shellcheck disable=SC2086
  timeout --foreground "$timeout_sec" hyperfine \
    --warmup 1 --runs 3 \
    --export-json "$tmpjson" \
    "$@" >"$tmpout" 2>&1 || rc=$?

  end=$(date +%s%N)
  elapsed=$(echo "scale=3; ($end - $start) / 1000000000" | bc | sed 's/^\./0./')

  if [[ "${rc:-0}" -eq 124 ]]; then
    RESULTS[$name]="TIMEOUT"
    VALUES[$name]="$timeout_sec"
    MESSAGES[$name]="exceeded ${timeout_sec}s timeout"
    log "  ⚠ TIMEOUT ($elapsed s)"
    FAILED=$((FAILED + 1))
  elif [[ "${rc:-0}" -ne 0 ]]; then
    RESULTS[$name]="FAIL"
    VALUES[$name]="$elapsed"
    MESSAGES[$name]=$(head -n3 "$tmpout" | tr '\n' '; ')
    log "  ✗ FAIL ($elapsed s)"
    tail -n5 "$tmpout" >&2 || true
    FAILED=$((FAILED + 1))
  else
    local mean stdev
    mean=$(jq -r '.results[0].mean // 0' "$tmpjson")
    stdev=$(jq -r '.results[0].stddev // 0' "$tmpjson")
    mean=$(printf '%.4f' "$mean")
    stdev=$(printf '%.4f' "$stdev")
    RESULTS[$name]="PASS"
    VALUES[$name]="$mean"
    MESSAGES[$name]="σ=${stdev}s (3 runs)"
    log "  ✓ PASS (avg ${mean}s, σ=${stdev}s)"
  fi
  rm -f "$tmpout" "$tmpjson"
}

run_size() {
  local name="$1" path="$2"
  if [[ -f "$path" ]]; then
    local bytes
    bytes=$(stat -c%s "$path" 2>/dev/null || stat -f%z "$path" 2>/dev/null)
    RESULTS[$name]="PASS"
    VALUES[$name]="$bytes"
    UNITS[$name]="bytes"
    MESSAGES[$name]="$(numfmt --to=iec-i --suffix=B "$bytes" 2>/dev/null || echo "${bytes} B")"
    log "  ✓ ${MESSAGES[$name]}"
  else
    RESULTS[$name]="FAIL"
    VALUES[$name]="0"
    UNITS[$name]="bytes"
    MESSAGES[$name]="binary not found: $path"
    log "  ✗ binary not found: $path"
    FAILED=$((FAILED + 1))
  fi
}

run_cargo_bloat() {
  log "▶ cargo bloat --release -n 20 …"
  if ! command -v cargo-bloat >/dev/null 2>&1; then
    RESULTS["cargo_bloat"]="SKIP"
    VALUES["cargo_bloat"]="0"
    MESSAGES["cargo_bloat"]="cargo-bloat not installed"
    log "  ⊘ SKIP (cargo-bloat not installed)"
    return
  fi
  local tmpout
  tmpout=$(mktemp)
  if cargo bloat --release -n 20 >"$tmpout" 2>&1; then
    local lines
    lines=$(wc -l < "$tmpout" | tr -d ' ')
    cp "$tmpout" target/bloat-report.txt
    RESULTS["cargo_bloat"]="PASS"
    VALUES["cargo_bloat"]="$lines"
    UNITS["cargo_bloat"]="lines"
    MESSAGES["cargo_bloat"]="target/bloat-report.txt"
    log "  ✓ $lines lines → target/bloat-report.txt"
  else
    RESULTS["cargo_bloat"]="FAIL"
    VALUES["cargo_bloat"]="0"
    MESSAGES["cargo_bloat"]=$(head -n2 "$tmpout" | tr '\n' '; ')
    log "  ✗ FAIL"
    tail -n5 "$tmpout" >&2 || true
    FAILED=$((FAILED + 1))
  fi
  rm -f "$tmpout"
}

run_cargo_timings() {
  log "▶ cargo build --timings …"
  rm -rf target/cargo-timings
  local tmpout rc
  tmpout=$(mktemp)
  # Use run_timed under the hood but also capture timings HTML
  local start end elapsed
  start=$(date +%s%N)
  timeout --foreground "$TIMEOUT_BUILD" cargo build --locked --release --timings >"$tmpout" 2>&1 || rc=$?
  end=$(date +%s%N)
  elapsed=$(echo "scale=3; ($end - $start) / 1000000000" | bc | sed 's/^\./0./')

  if [[ "${rc:-0}" -eq 124 ]]; then
    RESULTS["cargo_timings"]="TIMEOUT"
    VALUES["cargo_timings"]="$TIMEOUT_BUILD"
    MESSAGES["cargo_timings"]="exceeded ${TIMEOUT_BUILD}s timeout"
    log "  ⚠ TIMEOUT ($elapsed s)"
    FAILED=$((FAILED + 1))
  elif [[ "${rc:-0}" -ne 0 ]]; then
    RESULTS["cargo_timings"]="FAIL"
    VALUES["cargo_timings"]="$elapsed"
    MESSAGES["cargo_timings"]=$(head -n3 "$tmpout" | tr '\n' '; ')
    log "  ✗ FAIL ($elapsed s)"
    tail -n5 "$tmpout" >&2 || true
    FAILED=$((FAILED + 1))
  else
    local html_path
    html_path=$(ls target/cargo-timings/*.html 2>/dev/null | head -n1)
    if [[ -n "$html_path" && -f "$html_path" ]]; then
      cp "$html_path" target/timings-report.html
      RESULTS["cargo_timings"]="PASS"
      VALUES["cargo_timings"]="$elapsed"
      MESSAGES["cargo_timings"]="target/timings-report.html"
      log "  ✓ PASS ($elapsed s) → target/timings-report.html"
    else
      RESULTS["cargo_timings"]="FAIL"
      VALUES["cargo_timings"]="$elapsed"
      MESSAGES["cargo_timings"]="HTML report not found"
      log "  ✗ FAIL ($elapsed s) — HTML report missing"
      FAILED=$((FAILED + 1))
    fi
  fi
  rm -f "$tmpout"
}

# ── Benchmark steps ────────────────────────────────────────────────

bench_clean_release() {
  cargo clean >/dev/null 2>&1
  run_timed "clean_release_build" "$TIMEOUT_BUILD" \
    "Clean release build" \
    cargo build --locked --release
}

bench_warm_release() {
  run_timed "warm_release_build" "$TIMEOUT_BUILD" \
    "Warm release build (cached deps)" \
    cargo build --locked --release
}

bench_incremental() {
  touch src/main.rs
  run_timed "incremental_build" "$TIMEOUT_INCREMENTAL" \
    "Incremental release build (touch main.rs)" \
    cargo build --locked --release
}

bench_cargo_check() { run_timed "cargo_check" "$TIMEOUT_CHECK" "cargo check" cargo check --locked --all-features --workspace; }

bench_cargo_test()  { run_timed "cargo_test"  "$TIMEOUT_TEST"  "cargo test"  cargo test --locked --all-features --workspace; }

bench_binary_size() {
  log "▶ Release binary size …"
  run_size "release_binary_size" "target/release/lazyxrp"
}

bench_help_startup() {
  if [[ ! -x target/release/lazyxrp ]]; then
    RESULTS["help_startup"]="SKIP"; VALUES["help_startup"]="0"; MESSAGES["help_startup"]="release binary missing"
    log "  ⊘ SKIP (release binary missing)"; return
  fi
  run_hyperfine "help_startup" "$TIMEOUT_STARTUP" \
    "Startup time --help (hyperfine ×3)" \
    "target/release/lazyxrp --help"
}

bench_version_startup() {
  if [[ ! -x target/release/lazyxrp ]]; then
    RESULTS["version_startup"]="SKIP"; VALUES["version_startup"]="0"; MESSAGES["version_startup"]="release binary missing"
    log "  ⊘ SKIP (release binary missing)"; return
  fi
  run_hyperfine "version_startup" "$TIMEOUT_STARTUP" \
    "Startup time --version (hyperfine ×3)" \
    "target/release/lazyxrp --version"
}

bench_clippy() {
  run_timed "clippy_check" "$TIMEOUT_CLIPPY" \
    "cargo clippy" \
    cargo clippy --locked --all-targets --all-features --workspace -- -D warnings
}

bench_doc() {
  run_timed "doc_build" "$TIMEOUT_DOC" \
    "cargo doc" \
    cargo doc --locked --no-deps --document-private-items --all-features --workspace
}

# ── Main ───────────────────────────────────────────────────────────

main() {
  for arg in "$@"; do
    case "$arg" in
      --json) JSON_MODE=true ;;
      --ci)   CI_MODE=true ;;
      --fast) FAST_MODE=true ;;
    esac
  done

  log ""
  log "╔════════════════════════════════════════════════════════════════════╗"
  log "║           lazyxrp Benchmark Suite                                  ║"
  log "╚════════════════════════════════════════════════════════════════════╝"
  log ""
  log "Workspace : $(pwd)"
  log "Toolchain : $(rustc --version 2>/dev/null || echo 'unknown')"
  log "Date      : $(date -Iseconds)"
  log ""
  hr

  if $FAST_MODE; then
    log "▶ Fast mode: skipping clean_release_build"
    RESULTS["clean_release_build"]="SKIP"; VALUES["clean_release_build"]="0"; MESSAGES["clean_release_build"]="skipped (--fast)"
  else
    bench_clean_release   || true
  fi
  bench_warm_release    || true
  bench_incremental     || true
  bench_cargo_check     || true
  bench_cargo_test      || true
  bench_binary_size     || true
  run_cargo_bloat       || true
  bench_help_startup    || true
  bench_version_startup || true
  bench_clippy          || true
  bench_doc             || true
  run_cargo_timings     || true

  hr
  log ""
  log "📋 Benchmark Checklist"
  log ""

  local total_pass=0 total_fail=0 total_skip=0
  printf '  %-28s %-10s %12s  %s\n' "CHECK" "RESULT" "VALUE" "NOTE"
  printf '  %s\n' "$(hr | head -c 64)"

  for check in "${CHECKS[@]}"; do
    local res="${RESULTS[$check]:-SKIP}"
    local val="${VALUES[$check]:-0}"
    local unit="${UNITS[$check]:-s}"
    local msg="${MESSAGES[$check]:-}"]

    case "$res" in
      PASS)  icon="✓"; total_pass=$((total_pass + 1)) ;;
      FAIL)  icon="✗"; total_fail=$((total_fail + 1)) ;;
      TIMEOUT) icon="⚠"; total_fail=$((total_fail + 1)) ;;
      SKIP)  icon="⊘"; total_skip=$((total_skip + 1)) ;;
    esac

    if [[ "$check" == "release_binary_size" || "$check" == "cargo_bloat" ]]; then
      printf '  %-28s %-10s %12s  %s\n' "$check" "$icon $res" "${MESSAGES[$check]}" ""
    else
      printf '  %-28s %-10s %12s  %s\n' "$check" "$icon $res" "${val}${unit}" "$msg"
    fi
  done

  log ""
  log "Summary: $total_pass passed, $total_fail failed, $total_skip skipped"

  # ── Cache comparison ────────────────────────────────────────────
  if [[ "${RESULTS[clean_release_build]:-}" == "PASS" && "${RESULTS[warm_release_build]:-}" == "PASS" ]]; then
    local cold warm saved pct
    cold="${VALUES[clean_release_build]}"
    warm="${VALUES[warm_release_build]}"
    saved=$(echo "scale=3; $cold - $warm" | bc)
    pct=$(echo "scale=1; ($saved / $cold) * 100" | bc)
    log ""
    log "📦 Cache Comparison"
    log ""
    printf '  %-20s %12s\n' "Cold build (clean):" "${cold}s"
    printf '  %-20s %12s\n' "Warm build (cached):" "${warm}s"
    printf '  %-20s %12s\n' "Time saved:" "${saved}s (${pct}%)"
  fi

  if $JSON_MODE; then
    local json_items=()
    for check in "${CHECKS[@]}"; do
      local res="${RESULTS[$check]:-SKIP}"
      local val="${VALUES[$check]:-0}"
      local unit="${UNITS[$check]:-s}"
      local msg="${MESSAGES[$check]:-}"
      msg="${msg//\\/\\\\}"
      msg="${msg//\"/\\\"}"
      json_items+=("{\"name\":\"$check\",\"result\":\"$res\",\"value\":$val,\"unit\":\"$unit\",\"message\":\"$msg\"}")
    done
    local json_body
    json_body=$(printf '%s,' "${json_items[@]}" | sed 's/,$//')
    local cache_json="null"
    if [[ "${RESULTS[clean_release_build]:-}" == "PASS" && "${RESULTS[warm_release_build]:-}" == "PASS" ]]; then
      cache_json="{\"cold\":${VALUES[clean_release_build]},\"warm\":${VALUES[warm_release_build]},\"saved_seconds\":$saved,\"saved_percent\":$pct}"
    fi
    echo "{\"benchmarks\":[$json_body],\"summary\":{\"pass\":$total_pass,\"fail\":$total_fail,\"skip\":$total_skip,\"timestamp\":\"$(date -Iseconds)\"},\"cache_comparison\":$cache_json}"
  fi

  if $CI_MODE && [[ "$FAILED" -gt 0 ]]; then
    log ""
    log "❌ CI mode: $FAILED benchmark(s) failed — exiting with error"
    exit 1
  fi

  exit 0
}

main "$@"
