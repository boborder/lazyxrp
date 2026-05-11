#!/usr/bin/env bash
# Resolve Rust toolchain for GitHub Actions and local sanity checks:
# - If TOOLCHAIN_INPUT is non-empty, use it.
# - Else read `channel = "..."` from RUST_TOOLCHAIN_FILE (default: rust-toolchain.toml).
# Writes `toolchain=<value>` to GITHUB_OUTPUT when set; always logs chosen toolchain to stderr.
set -euo pipefail

toolchain_file="${RUST_TOOLCHAIN_FILE:-rust-toolchain.toml}"
tc="${TOOLCHAIN_INPUT:-}"

if [[ -z "$tc" ]]; then
  if [[ ! -f "$toolchain_file" ]]; then
    echo "error: toolchain file not found: ${toolchain_file}" >&2
    exit 1
  fi
  tc=$(sed -n 's/^channel = "\(.*\)"/\1/p' "$toolchain_file" | head -1)
fi

if [[ -z "$tc" ]]; then
  echo "error: could not resolve Rust toolchain (empty input and no channel in ${toolchain_file})" >&2
  exit 1
fi

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    echo "toolchain=${tc}"
  } >>"${GITHUB_OUTPUT}"
fi

echo "Using Rust toolchain: ${tc}" >&2
