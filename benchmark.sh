#!/usr/bin/env bash
# benchmark.sh — lazyxrp build & performance benchmark suite
# Usage: ./benchmark.sh [--json] [--ci] [--fast] [--perf-only] [--full]
#   --json        Write results JSON to benchmark-summary.json (stdout stays human-readable)
#   --ci          Strict mode: fail on any step timeout/error (implies --perf-only unless --full)
#   --fast        Skip the clean release build
#   --perf-only   Build/startup/size only (skip check, test, clippy, doc — CI default)
#   --full        Full suite including quality gates (local `mise run bench`)
#
# Tools: hyperfine (startup stats), cargo-bloat (size breakdown), cargo --timings (on warm build)
#   Local: mise install  (hyperfine + cargo-bloat provisioned via mise.toml [tools])

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
readonly JSON_FILE="benchmark-summary.json"

readonly CHECKS_PERF=(
  "clean_release_build"
  "warm_release_build"
  "incremental_build"
  "release_binary_size"
  "cargo_bloat"
  "help_startup"
  "version_startup"
)

readonly CHECKS_FULL=(
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
PERF_ONLY_MODE=false
FULL_MODE=false
ACTIVE_CHECKS=()

# ── Helpers ────────────────────────────────────────────────────────
log() { printf '%s\n' "$*"; }
hr()  { printf '─%.0s' $(seq 1 70); printf '\n'; }

# Portable ISO-8601 timestamp (GNU date -Iseconds or BSD date)
date_iso() {
  if date -Iseconds >/dev/null 2>&1; then
    date -Iseconds
  else
    date -u +"%Y-%m-%dT%H:%M:%S%z"
  fi
}

# Portable timeout: GNU timeout, macOS gtimeout, or no timeout (warn once)
TIMEOUT_CMD=""
timeout_cmd() {
  if [[ -n "$TIMEOUT_CMD" ]]; then
    "$TIMEOUT_CMD" "$@"
    return
  fi
  if command -v timeout >/dev/null 2>&1; then
    TIMEOUT_CMD="timeout"
    "$TIMEOUT_CMD" "$@"
  elif command -v gtimeout >/dev/null 2>&1; then
    TIMEOUT_CMD="gtimeout"
    "$TIMEOUT_CMD" "$@"
  else
    log "  ⚠ timeout not available — running without time limit"
    shift 2
    "$@"
  fi
}

check_dependencies() {
  local missing=()
  command -v bc >/dev/null 2>&1 || missing+=("bc")
  if [[ ${#missing[@]} -gt 0 ]]; then
    log "Missing required tools: ${missing[*]}"
    log "  macOS: brew install bc"
    log "  Ubuntu: sudo apt-get install -y bc"
    exit 1
  fi
  if ! command -v jq >/dev/null 2>&1; then
    log "⚠ jq not found — hyperfine startup benchmarks will SKIP"
    log "  macOS: brew install jq | Ubuntu: sudo apt-get install -y jq"
  fi
}

json_escape() {
  local s="$1"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  s="${s//$'\n'/\\n}"
  s="${s//$'\r'/\\r}"
  s="${s//$'\t'/\\t}"
  printf '%s' "$s"
}

run_timed() {
  local name="$1" timeout_sec="$2" desc="$3"
  shift 3
  log "▶ $desc …"

  local start end elapsed tmpout rc
  start=$(date +%s%N)
  tmpout=$(mktemp)

  timeout_cmd --foreground "$timeout_sec" "$@" >"$tmpout" 2>&1 || rc=$?
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

  timeout_cmd --foreground "$timeout_sec" hyperfine \
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
    mean=$(jq -r '.results[0].mean // empty' "$tmpjson" 2>/dev/null || true)
    stdev=$(jq -r '.results[0].stddev // 0' "$tmpjson" 2>/dev/null || true)
    if [[ -z "$mean" ]] || ! echo "$mean" | grep -qE '^[0-9]+([.][0-9]+)?$'; then
      RESULTS[$name]="FAIL"
      VALUES[$name]="$elapsed"
      MESSAGES[$name]="invalid hyperfine JSON output"
      log "  ✗ FAIL — invalid hyperfine JSON"
      FAILED=$((FAILED + 1))
      rm -f "$tmpout" "$tmpjson"
      return
    fi
    mean=$(printf '%.4f' "$mean")
    stdev=$(printf '%.4f' "${stdev:-0}")
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
  mkdir -p target
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

copy_timings_report() {
  local html_path
  html_path=$(ls target/cargo-timings/*.html 2>/dev/null | head -n1)
  if [[ -n "$html_path" && -f "$html_path" ]]; then
    mkdir -p target
    cp "$html_path" target/timings-report.html
    log "  ✓ Timing report → target/timings-report.html"
  else
    log "  ⊘ No cargo-timings HTML report (non-fatal)"
  fi
}

bench_clean_release() {
  cargo clean >/dev/null 2>&1
  run_timed "clean_release_build" "$TIMEOUT_BUILD" \
    "Clean release build" \
    cargo build --locked --release
}

bench_warm_release() {
  rm -rf target/cargo-timings
  run_timed "warm_release_build" "$TIMEOUT_BUILD" \
    "Warm release build (cached deps, --timings)" \
    cargo build --locked --release --timings
  if [[ "${RESULTS[warm_release_build]:-}" == "PASS" ]]; then
    copy_timings_report || true
  fi
}

bench_incremental() {
  local main_rs="src/main.rs"
  local saved_mtime=""
  if [[ -f "$main_rs" ]]; then
    saved_mtime=$(stat -c %Y "$main_rs" 2>/dev/null || stat -f %m "$main_rs")
  fi
  touch "$main_rs"
  run_timed "incremental_build" "$TIMEOUT_INCREMENTAL" \
    "Incremental release build (touch main.rs)" \
    cargo build --locked --release
  if [[ -n "$saved_mtime" ]]; then
    if touch -d "@${saved_mtime}" "$main_rs" 2>/dev/null; then
      :
    else
      touch -t "$(date -r "$saved_mtime" +%Y%m%d%H%M.%S 2>/dev/null || true)" "$main_rs" 2>/dev/null || true
    fi
  fi
}

bench_cargo_check() {
  run_timed "cargo_check" "$TIMEOUT_CHECK" "cargo check" \
    cargo check --locked --all-features --workspace
}

bench_cargo_test() {
  run_timed "cargo_test" "$TIMEOUT_TEST" "cargo test" \
    cargo test --locked --all-features --workspace
}

bench_binary_size() {
  log "▶ Release binary size …"
  run_size "release_binary_size" "target/release/lazyxrp"
}

bench_help_startup() {
  if [[ ! -x target/release/lazyxrp ]]; then
    RESULTS["help_startup"]="SKIP"
    VALUES["help_startup"]="0"
    MESSAGES["help_startup"]="release binary missing"
    log "  ⊘ SKIP (release binary missing)"
    return
  fi
  run_hyperfine "help_startup" "$TIMEOUT_STARTUP" \
    "Startup time --help (hyperfine ×3)" \
    "target/release/lazyxrp --help"
}

bench_version_startup() {
  if [[ ! -x target/release/lazyxrp ]]; then
    RESULTS["version_startup"]="SKIP"
    VALUES["version_startup"]="0"
    MESSAGES["version_startup"]="release binary missing"
    log "  ⊘ SKIP (release binary missing)"
    return
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

write_json_summary() {
  local total_pass="$1" total_fail="$2" total_skip="$3"
  local saved="${4:-0}" pct="${5:-0}"
  local json_items=()
  local check res val unit msg json_body cache_json

  for check in "${ACTIVE_CHECKS[@]}"; do
    res="${RESULTS[$check]:-SKIP}"
    val="${VALUES[$check]:-0}"
    unit="${UNITS[$check]:-s}"
    msg="$(json_escape "${MESSAGES[$check]:-}")"
    json_items+=("{\"name\":\"$check\",\"result\":\"$res\",\"value\":$val,\"unit\":\"$unit\",\"message\":\"$msg\"}")
  done

  json_body=$(printf '%s,' "${json_items[@]}" | sed 's/,$//')
  cache_json="null"
  if [[ "${RESULTS[clean_release_build]:-}" == "PASS" && "${RESULTS[warm_release_build]:-}" == "PASS" ]]; then
    cache_json="{\"cold\":${VALUES[clean_release_build]},\"warm\":${VALUES[warm_release_build]},\"saved_seconds\":$saved,\"saved_percent\":$pct}"
  fi

  printf '%s\n' "{\"benchmarks\":[$json_body],\"summary\":{\"pass\":$total_pass,\"fail\":$total_fail,\"skip\":$total_skip,\"timestamp\":\"$(date_iso)\"},\"cache_comparison\":$cache_json}" \
    >"$JSON_FILE"
  log ""
  log "JSON summary → $JSON_FILE"
}

main() {
  for arg in "$@"; do
    case "$arg" in
      --json)       JSON_MODE=true ;;
      --ci)         CI_MODE=true ;;
      --fast)       FAST_MODE=true ;;
      --perf-only)  PERF_ONLY_MODE=true ;;
      --full)       FULL_MODE=true ;;
    esac
  done

  if $CI_MODE && ! $FULL_MODE; then
    PERF_ONLY_MODE=true
  fi

  if $PERF_ONLY_MODE; then
    ACTIVE_CHECKS=("${CHECKS_PERF[@]}")
  else
    ACTIVE_CHECKS=("${CHECKS_FULL[@]}")
  fi

  check_dependencies
  mkdir -p target

  log ""
  log "╔════════════════════════════════════════════════════════════════════╗"
  log "║           lazyxrp Benchmark Suite                                  ║"
  log "╚════════════════════════════════════════════════════════════════════╝"
  log ""
  log "Workspace : $(pwd)"
  log "Toolchain : $(rustc --version 2>/dev/null || echo 'unknown')"
  log "Date      : $(date_iso)"
  log "Mode      : $( $PERF_ONLY_MODE && echo 'perf-only' || echo 'full' )$( $FAST_MODE && echo ' + fast' || true )$( $CI_MODE && echo ' + ci' || true )"
  log ""
  hr

  if $FAST_MODE; then
    log "▶ Fast mode: skipping clean_release_build"
    RESULTS["clean_release_build"]="SKIP"
    VALUES["clean_release_build"]="0"
    MESSAGES["clean_release_build"]="skipped (--fast)"
  else
    bench_clean_release || true
  fi

  bench_warm_release      || true
  bench_incremental       || true

  if ! $PERF_ONLY_MODE; then
    bench_cargo_check     || true
    bench_cargo_test      || true
  fi

  bench_binary_size       || true
  run_cargo_bloat         || true
  bench_help_startup      || true
  bench_version_startup   || true

  if ! $PERF_ONLY_MODE; then
    bench_clippy          || true
    bench_doc             || true
  fi

  hr
  log ""
  log "📋 Benchmark Checklist"
  log ""

  local total_pass=0 total_fail=0 total_skip=0
  printf '  %-28s %-10s %12s  %s\n' "CHECK" "RESULT" "VALUE" "NOTE"
  printf '  %s\n' "$(hr | head -c 64)"

  for check in "${ACTIVE_CHECKS[@]}"; do
    local res="${RESULTS[$check]:-SKIP}"
    local val="${VALUES[$check]:-0}"
    local unit="${UNITS[$check]:-s}"
    local msg="${MESSAGES[$check]:-}"

    case "$res" in
      PASS)    icon="✓"; total_pass=$((total_pass + 1)) ;;
      FAIL)    icon="✗"; total_fail=$((total_fail + 1)) ;;
      TIMEOUT) icon="⚠"; total_fail=$((total_fail + 1)) ;;
      SKIP)    icon="⊘"; total_skip=$((total_skip + 1)) ;;
    esac

    if [[ "$check" == "release_binary_size" || "$check" == "cargo_bloat" ]]; then
      printf '  %-28s %-10s %12s  %s\n' "$check" "$icon $res" "${MESSAGES[$check]}" ""
    else
      printf '  %-28s %-10s %12s  %s\n' "$check" "$icon $res" "${val}${unit}" "$msg"
    fi
  done

  log ""
  log "Summary: $total_pass passed, $total_fail failed, $total_skip skipped"

  local saved=0 pct=0
  if [[ "${RESULTS[clean_release_build]:-}" == "PASS" && "${RESULTS[warm_release_build]:-}" == "PASS" ]]; then
    local cold warm
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
    write_json_summary "$total_pass" "$total_fail" "$total_skip" "$saved" "$pct"
  fi

  if $CI_MODE && [[ "$FAILED" -gt 0 ]]; then
    log ""
    log "❌ CI mode: $FAILED benchmark(s) failed — exiting with error"
    exit 1
  fi

  exit 0
}

main "$@"
