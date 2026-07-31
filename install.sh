#!/bin/bash
# install.sh — lazyxrp interactive installer
#
# Parse CLI first (see usage with --help). Environment overrides:
#   INSTALL_DIR     Installation bin directory (default: ~/.local/bin)
#   BINARY_INSTALL  Set to 1 to skip source build even when Cargo.toml is present
#   VERSION         Specific version for binary install (default: latest release tag)
#   NO_VERIFY       Set to 1 to skip checksum verification (binary install only)
#   CI              When set, force non-interactive defaults (same as -q)
#
# GitHub Releases API (`releases/latest`): optional auth to reduce rate-limit failures —
# set GITHUB_TOKEN or GITHUB_API_TOKEN (fine-grained: read repo metadata / contents).

set -eu

# ── Globals for EXIT cleanup (avoid nested trap overwriting main cleanup) ───
TMP_DOWNLOAD_DIR=""
PARTIAL_BIN_DEST=""
PARTIAL_RP_DEST=""

# ── Configuration ────────────────────────────────────────────────────────────
UA="lazyxrp-installer/2.1"
CURL_RETRY=(--retry 3 --retry-delay 1 --connect-timeout 15 --max-time 300)
REPO="boborder/lazyxrp"
BIN_NAME="lazyxrp"

VERSION="${VERSION:-}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
NO_VERIFY="${NO_VERIFY:-0}"
BINARY_INSTALL="${BINARY_INSTALL:-0}"
GITHUB_API="https://api.github.com/repos/${REPO}"
GITHUB_RELEASES="https://github.com/${REPO}/releases/download"

QUIET=0
CLI_INSTALL_RUST=""
CLI_INSTALL_MISE=""
CLI_METHOD=""

usage() {
    cat <<'EOF'
lazyxrp installer — https://github.com/boborder/lazyxrp

Usage:
  ./install.sh [options]

Options:
  -h, --help             Show this help and exit
  --uninstall-help       Show manual uninstall commands (no removal)
  -q, --quiet            Non-interactive: minimal output; defaults for prompts

  --install-rust         If cargo is missing, install Rust via rustup (TTY: skip prompt)
  --no-install-rust      If cargo is missing, do not install rust (use binary path)

  --install-mise         If mise is missing, install it (TTY: skip prompt)
  --no-install-mise      Skip mise install / offer

  --method cargo         Build from source (requires this repo + Cargo.toml on disk)
  --method binary        Download prebuilt release archive

Environment:
  INSTALL_DIR, BINARY_INSTALL, VERSION, NO_VERIFY, CI (see script header)
  NO_VERIFY=1            Skip checksum verification (binary install only). ⚠️  Increases MITM risk.
  CI=1                   Non-interactive (same as -q); auto-updates shell PATH when needed
  Optional: GITHUB_TOKEN or GITHUB_API_TOKEN — authenticated GitHub REST (release / commit lookups)

Examples:
  ./install.sh
  ./install.sh -q --method binary
  ./install.sh --no-install-mise
  curl -fsSL https://raw.githubusercontent.com/boborder/lazyxrp/main/install.sh | sh -s -- -q --no-install-rust

Manual uninstall (this script does not remove lazyxrp for you):

  If you still have lazyxrp on PATH (same binary you want gone):
    lazyxrp --self-uninstall
    lazyxrp --self-uninstall --yes   # skip "type yes" confirmation

  Prebuilt/binary install — remove the executable (and backup if present):
    rm -f INSTALL_DIR/lazyxrp INSTALL_DIR/lazyxrp.bak INSTALL_DIR/rp
    Replace INSTALL_DIR with your target (default: ~/.local/bin).

  Source install via this script into ~/.local/bin (cargo install --root ~/.local):
    cargo uninstall lazyxrp --root "$HOME/.local"

  Globally via cargo default prefix (~/.cargo/bin, no custom --root):
    cargo uninstall lazyxrp

  One-off: cargo install --path . — same as default-prefix case:
    cargo uninstall lazyxrp

  Optional: user config + app data (matches src/config.rs + directories::ProjectDirs; skip if you use custom paths):

    Linux / *BSD (typical XDG — no LAZYXRP_* overrides):
      rm -rf ~/.config/lazyxrp ~/.local/share/com.kdheepak.lazyxrp

    macOS:
      rm -rf "$HOME/Library/Application Support/lazyxrp" \
             "$HOME/Library/Application Support/com.kdheepak.lazyxrp"

    If you set LAZYXRP_CONFIG or LAZYXRP_DATA, remove those directories instead.
    If config.toml sets data_dir / config_dir, remove those paths too.

  Optional: revert rustup/mise only if you installed them for lazyxrp and want them gone —
    rustup self uninstall
    https://mise.jdx.dev/ (follow upstream uninstall docs)
EOF
}

uninstall_help() {
    cat <<'EOF'
lazyxrp — manual uninstall (nothing is executed by this script)

0) If lazyxrp is still on your PATH (removes that binary + resolved config/data; see src/uninstall.rs):
   lazyxrp --self-uninstall
   lazyxrp --self-uninstall --yes

1) Binary / GitHub-release install (installer copied lazyxrp to INSTALL_DIR):

   rm -f INSTALL_DIR/lazyxrp INSTALL_DIR/lazyxrp.bak INSTALL_DIR/rp

   Default INSTALL_DIR is ~/.local/bin. Use the same directory you chose at install.

2) Source install with this installer into ~/.local/bin (cargo used --root "$HOME/.local"):

   cargo uninstall lazyxrp --root "$HOME/.local"

3) Cargo default install prefix (~/.cargo/bin), including `cargo install --path .`:

   cargo uninstall lazyxrp

4) Optional — remove user config and data (nothing runs this for you):

   Linux / *BSD (typical XDG — no LAZYXRP_* overrides):
     rm -rf ~/.config/lazyxrp ~/.local/share/com.kdheepak.lazyxrp

   macOS:
     rm -rf "$HOME/Library/Application Support/lazyxrp" \
            "$HOME/Library/Application Support/com.kdheepak.lazyxrp"

   If you use LAZYXRP_CONFIG or LAZYXRP_DATA, delete those paths. If `config.toml` sets
   data_dir / config_dir, remove those directories too.

This does NOT remove Rust, rustup, or mise unless you uninstall those tools separately.

See also: ./install.sh --help
EOF
}

parse_args() {
    while [ $# -gt 0 ]; do
        case "$1" in
            -h|--help)
                usage
                exit 0
                ;;
            --uninstall-help)
                uninstall_help
                exit 0
                ;;
            -q|--quiet)
                QUIET=1
                shift
                ;;
            --install-rust)
                CLI_INSTALL_RUST=yes
                shift
                ;;
            --no-install-rust)
                CLI_INSTALL_RUST=no
                shift
                ;;
            --install-mise)
                CLI_INSTALL_MISE=yes
                shift
                ;;
            --no-install-mise)
                CLI_INSTALL_MISE=no
                shift
                ;;
            --method)
                CLI_METHOD="${2:-}"
                shift 2
                ;;
            --method=*)
                CLI_METHOD="${1#*=}"
                shift
                ;;
            *)
                printf 'install.sh: unknown option %q (try --help)\n' "$1" >&2
                exit 1
                ;;
        esac
    done
}

parse_args "$@"

# CI / automation: non-interactive defaults (CodeRabbit-style)
if [ -n "${CI:-}" ]; then
    QUIET=1
fi

# Expand leading ~ in INSTALL_DIR
case "$INSTALL_DIR" in
    "~")   INSTALL_DIR="$HOME" ;;
    "~/"*) INSTALL_DIR="$HOME/${INSTALL_DIR#~/}" ;;
esac

# ── Colours ──────────────────────────────────────────────────────────────────
IS_TTY=0
if [ -t 1 ] && [ "$QUIET" = 0 ]; then
    IS_TTY=1
fi

if [ "$IS_TTY" = 1 ]; then
    BOLD="\033[1m"
    DIM="\033[2m"
    ITALIC="\033[3m"
    GREEN="\033[32m"
    YELLOW="\033[33m"
    RED="\033[31m"
    CYAN="\033[36m"
    MAGENTA="\033[35m"
    BLUE="\033[34m"
    WHITE="\033[97m"
    BG_BLUE="\033[44m"
    BG_GREEN="\033[42m"
    BG_RED="\033[41m"
    RESET="\033[0m"
    CLEAR_LINE="\033[2K\r"
    HIDE_CURSOR="\033[?25l"
    SHOW_CURSOR="\033[?25h"
else
    BOLD="" DIM="" ITALIC=""
    GREEN="" YELLOW="" RED="" CYAN="" MAGENTA="" BLUE="" WHITE=""
    BG_BLUE="" BG_GREEN="" BG_RED=""
    RESET="" CLEAR_LINE="" HIDE_CURSOR="" SHOW_CURSOR=""
fi

# ── Logging ──────────────────────────────────────────────────────────────────
info()  { printf '  %b●%b  %s\n'  "$CYAN"   "$RESET" "$*"; }
ok()    { printf '  %b✔%b  %s\n'  "$GREEN"  "$RESET" "$*"; }
warn()  { printf '  %b⚠%b  %s\n' "$YELLOW" "$RESET" "$*" >&2; }
die()   {
    printf '  %b✖%b  %s\n' "$RED" "$RESET" "$*" >&2
    [ "$IS_TTY" = 1 ] && printf '%b' "$SHOW_CURSOR"
    exit 1
}
step()  { printf '\n  %b%b▸ %s%b\n' "$BOLD" "$WHITE" "$*" "$RESET"; }

# ── Helpers ──────────────────────────────────────────────────────────────────
has() { command -v "$1" > /dev/null 2>&1; }

ensure_downloader() {
    if has curl || has wget; then
        return 0
    fi
    die "curl or wget is required. Please install one of them first."
}

# Prefer curl; fall back to wget (CodeRabbit-style).
fetch() {
    local url="$1"
    if has curl; then
        curl -fsSL "${CURL_RETRY[@]}" -H "User-Agent: ${UA}" "$url"
    else
        wget -q --user-agent="${UA}" -O - "$url"
    fi
}

fetch_soft() {
    local url="$1"
    if has curl; then
        curl -sSL "${CURL_RETRY[@]}" \
            -H "Accept: application/vnd.github+json" \
            -H "User-Agent: ${UA}" \
            "$url"
    else
        wget -q --user-agent="${UA}" \
            --header="Accept: application/vnd.github+json" \
            -O - "$url"
    fi
}

# Authenticated GitHub REST (optional; lowers risk of REST rate-limit on NAT/CI-ish IPs).
curl_github_rest() {
    local url="$1"
    local token="${GITHUB_TOKEN:-${GITHUB_API_TOKEN:-}}"
    if [ -n "$token" ]; then
        if has curl; then
            curl -sSL "${CURL_RETRY[@]}" \
                -H "Accept: application/vnd.github+json" \
                -H "Authorization: Bearer ${token}" \
                -H "User-Agent: ${UA}" \
                -H "X-GitHub-Api-Version: 2022-11-28" \
                "$url"
        else
            wget -q --user-agent="${UA}" \
                --header="Accept: application/vnd.github+json" \
                --header="Authorization: Bearer ${token}" \
                --header="X-GitHub-Api-Version: 2022-11-28" \
                -O - "$url"
        fi
    else
        fetch_soft "$url"
    fi
}

fetch_file() {
    local url="$1" dest="$2"
    if has curl; then
        if [ "$IS_TTY" = 1 ]; then
            curl -fsSL "${CURL_RETRY[@]}" --progress-bar -o "$dest" "$url"
        else
            curl -fsSL "${CURL_RETRY[@]}" -o "$dest" "$url"
        fi
    else
        wget -q --user-agent="${UA}" -O "$dest" "$url"
    fi
}

# ── Tool Installers ─────────────────────────────────────────────────────────
# Toolchain pinned in repo (fallback when stdin/pipe install cannot read sibling files).
read_pinned_toolchain_channel() {
    local root="$1"
    local ch=""
    if [ -n "$root" ] && [ -f "${root}/rust-toolchain.toml" ]; then
        ch=$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "${root}/rust-toolchain.toml" | head -1)
    fi
    if [ -n "$ch" ]; then
        printf '%s\n' "$ch"
    else
        printf '%s\n' "stable"
    fi
}

offer_install_rustup() {
    if has cargo; then return 0; fi

    local do_install=0
    if [ -n "$CLI_INSTALL_RUST" ]; then
        case "$CLI_INSTALL_RUST" in
            yes) do_install=1 ;;
            *)
                warn "cargo not found — skipping rustup (--no-install-rust); source builds unavailable"
                return 1
                ;;
        esac
    elif [ "$IS_TTY" = 0 ]; then
        info "cargo not found — installing Rust via rustup (non-interactive default)"
        do_install=1
    elif ! prompt_yn "Rust (cargo) not found. Install via rustup?" "y"; then
        warn "cargo not available — source builds disabled unless you install Rust separately"
        return 1
    else
        do_install=1
    fi

    [ "$do_install" = 1 ] || return 1

    [ "$IS_TTY" = 1 ] && step "Installing Rust via rustup" || info "Installing Rust via rustup..."
    local rustup_init
    rustup_init=$(mktemp)
    spinner_start "Downloading rustup-init..."
    if has curl; then
        curl -sSf "${CURL_RETRY[@]}" -o "$rustup_init" https://sh.rustup.rs
    else
        wget -q -O "$rustup_init" https://sh.rustup.rs
    fi
    spinner_stop

    local default_tc
    default_tc=$(read_pinned_toolchain_channel "$(script_dir)")

    sh "$rustup_init" -y --default-toolchain "$default_tc" --no-modify-path 2>&1 \
        | while IFS= read -r line; do
            printf '%b  %b│%b  %s\n' "$CLEAR_LINE" "$DIM" "$RESET" "$line"
        done
    rm -f "$rustup_init"

    # Source cargo env for this session
    local cargo_env="${CARGO_HOME:-$HOME/.cargo}/env"
    if [ -f "$cargo_env" ]; then
        # shellcheck disable=SC1090
        . "$cargo_env"
    fi

    if has cargo; then
        ok "Rust installed: $(rustc --version 2>/dev/null || echo 'unknown')"
        return 0
    else
        warn "rustup finished but cargo is not on PATH yet"
        warn "Re-run in a new shell or: source ~/.cargo/env"
        return 1
    fi
}

offer_install_mise() {
    if has mise; then return 0; fi
    if [ "${CLI_INSTALL_MISE:-}" = "no" ]; then
        return 1
    fi

    local do_install=0
    if [ -n "$CLI_INSTALL_MISE" ]; then
        case "$CLI_INSTALL_MISE" in
            yes) do_install=1 ;;
            *) return 1 ;;
        esac
    elif [ "$IS_TTY" = 0 ]; then
        info "mise not found — skipping (non-interactive default: no)"
        return 1
    elif ! prompt_yn "mise not found. Install mise?" "n"; then
        return 1
    else
        do_install=1
    fi

    [ "$do_install" = 1 ] || return 1

    step "Installing mise"
    spinner_start "Downloading mise..."
    local mise_script
    mise_script=$(curl -fsSL "${CURL_RETRY[@]}" https://mise.run)
    spinner_stop

    echo "$mise_script" | sh 2>&1 \
        | while IFS= read -r line; do
            printf '%b  %b│%b  %s\n' "$CLEAR_LINE" "$DIM" "$RESET" "$line"
        done

    # Activate mise for this session
    if [ -f "$HOME/.local/bin/mise" ]; then
        export PATH="$HOME/.local/bin:$PATH"
    fi

    if has mise; then
        ok "mise installed: $(mise --version 2>/dev/null || echo 'unknown')"
    else
        warn "mise install finished but mise is not on PATH"
    fi
}

sleep_tick() {
    if [ "$IS_TTY" = 1 ]; then
        sleep "${1:-0.06}"
    fi
}

# ── ASCII Art Banner ─────────────────────────────────────────────────────────
print_banner() {
    [ "$IS_TTY" = 0 ] && return
    printf '\n'
    printf '%b' "$CYAN"
    cat <<'BANNER'
         _                   __  ______  ____
        | |    __ _ _____   _\ \/ /  _ \|  _ \
        | |   / _` |_  / | | |\  /| |_) | |_) |
        | |__| (_| |/ /| |_| |/  \|  _ <|  __/
        |_____\__,_/___|\__, /_/\_\_| \_\_|
                        |___/
BANNER
    printf '%b' "$RESET"
    printf '\n'
}

# ── Animated Banner ─────────────────────────────────────────────────────────
print_animated_banner() {
    [ "$IS_TTY" = 0 ] && { print_banner; return; }

    printf '%b' "$HIDE_CURSOR"

    local logo_lines
    logo_lines=(
        '         _                   __  ______  ____'
        '        | |    __ _ _____   _\ \/ /  _ \|  _ \'
        '        | |   / _` |_  / | | |\  /| |_) | |_) |'
        '        | |__| (_| |/ /| |_| |/  \|  _ <|  __/'
        '        |_____\__,_/___|\__, /_/\_\_| \_\_|'
        '                        |___/'
    )

    printf '\n'

    local row
    for ((row = 0; row < ${#logo_lines[@]}; row++)); do
        printf '%b  %s%b\n' "$CYAN" "${logo_lines[$row]}" "$RESET"
        sleep_tick 0.05
    done

    printf '\n'
    printf '%b' "$SHOW_CURSOR"
}

# ── Subtitle ─────────────────────────────────────────────────────────────────
print_subtitle() {
    [ "$IS_TTY" = 0 ] && return
    printf '  %b%bXRP Ledger TUI%b  %b·%b  %bhttps://github.com/%s%b\n' \
        "$BOLD" "$WHITE" "$RESET" "$DIM" "$RESET" "$DIM" "$REPO" "$RESET"
    printf '  %b━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━%b\n' "$DIM" "$RESET"
}

# ── Spinner ──────────────────────────────────────────────────────────────────
SPIN_FRAMES=(⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏)
SPIN_PID=""

spinner_start() {
    [ "$IS_TTY" = 0 ] && return
    local msg="$1"
    printf '%b' "$HIDE_CURSOR"
    (
        local i=0 n=${#SPIN_FRAMES[@]}
        while true; do
            local frame=${SPIN_FRAMES[$((i % n))]}
            printf '%b  %b%s%b  %s' "$CLEAR_LINE" "$CYAN" "$frame" "$RESET" "$msg"
            i=$((i + 1))
            sleep 0.08
        done
    ) &
    SPIN_PID=$!
}

spinner_stop() {
    [ "$IS_TTY" = 0 ] && return
    if [ -n "$SPIN_PID" ]; then
        kill "$SPIN_PID" 2>/dev/null || true
        wait "$SPIN_PID" 2>/dev/null || true
        SPIN_PID=""
        printf '%b' "$CLEAR_LINE"
    fi
    printf '%b' "$SHOW_CURSOR"
}

# ── Progress Bar ─────────────────────────────────────────────────────────────
progress_bar() {
    [ "$IS_TTY" = 0 ] && return
    local current="$1" total="$2" label="${3:-}"
    local width=30
    local filled=$(( current * width / total ))
    local empty=$(( width - filled ))
    local pct=$(( current * 100 / total ))

    printf '%b  ' "$CLEAR_LINE"
    printf '%b' "$CYAN"
    local i
    for ((i = 0; i < filled; i++)); do printf '█'; done
    for ((i = 0; i < empty; i++)); do printf '░'; done
    printf '%b %3d%%' "$RESET" "$pct"
    [ -n "$label" ] && printf '  %b%s%b' "$DIM" "$label" "$RESET"
}

# ── Interactive Prompt ───────────────────────────────────────────────────────
prompt_yn() {
    local msg="$1" default="${2:-y}"
    if [ "$IS_TTY" = 0 ]; then
        [ "$default" = "y" ] && return 0 || return 1
    fi
    local hint
    [ "$default" = "y" ] && hint="Y/n" || hint="y/N"
    printf '\n  %b?%b  %s %b[%s]%b ' "$MAGENTA" "$RESET" "$msg" "$DIM" "$hint" "$RESET"
    read -r ans </dev/tty || ans=""
    ans=$(echo "$ans" | tr '[:upper:]' '[:lower:]')
    case "$ans" in
        y|yes) return 0 ;;
        n|no)  return 1 ;;
        "")    [ "$default" = "y" ] && return 0 || return 1 ;;
        *)     [ "$default" = "y" ] && return 0 || return 1 ;;
    esac
}

prompt_choice() {
    local title="$1"; shift
    local options=("$@")
    local count=${#options[@]}

    if [ "$IS_TTY" = 0 ]; then
        echo 1
        return
    fi

    printf '\n  %b?%b  %s\n' "$MAGENTA" "$RESET" "$title"
    local i
    for ((i = 0; i < count; i++)); do
        local num=$(( i + 1 ))
        if [ "$i" -eq 0 ]; then
            printf '     %b%b▸ %d)%b %s %b(recommended)%b\n' "$BOLD" "$CYAN" "$num" "$RESET" "${options[$i]}" "$DIM" "$RESET"
        else
            printf '       %b%d)%b %s\n' "$DIM" "$num" "$RESET" "${options[$i]}"
        fi
    done
    printf '\n     %bChoice [1-%d]:%b ' "$WHITE" "$count" "$RESET"
    read -r choice </dev/tty || choice=""
    case "$choice" in
        [1-9]) echo "$choice" ;;
        "")    echo 1 ;;
        *)     echo 1 ;;
    esac
}

# ── Platform Detection ───────────────────────────────────────────────────────
script_dir() {
    local sp
    sp="${BASH_SOURCE[0]:-${0}}"
    case "$sp" in
        "" | - | stdin | bash | */bash | /dev/stdin | /dev/fd/[0-9]* | /dev/fd/* | /proc/self/fd/[0-9]* | /proc/self/fd/*)
            printf '' ;;
        *)
            ( cd "$(dirname "$sp")" && pwd ) || printf '' ;;
    esac
}

detect_platform() {
    local os arch
    os=$(uname -s)
    arch=$(uname -m)

    case "$os" in
        Darwin) OS_NAME="macos"; OS_DISPLAY="macOS" ;;
        Linux)  OS_NAME="linux"; OS_DISPLAY="Linux" ;;
        *)      die "Unsupported OS: ${os}" ;;
    esac

    case "$arch" in
        x86_64 | amd64)    ARCH_NAME="x86_64";  ARCH_DISPLAY="x86_64"  ;;
        aarch64 | arm64)   ARCH_NAME="arm64";    ARCH_DISPLAY="ARM64"   ;;
        i386 | i686)       ARCH_NAME="i686";     ARCH_DISPLAY="i686"    ;;
        *)                 die "Unsupported architecture: ${arch}" ;;
    esac
}

# ── Version Resolution ───────────────────────────────────────────────────────
resolve_version() {
    if [ -n "$VERSION" ]; then
        info "Requested version: ${VERSION}"
        return
    fi

    spinner_start "Fetching latest release..."
    local body
    body=$(curl_github_rest "${GITHUB_API}/releases/latest")
    if has jq; then
        VERSION=$(printf '%s' "$body" | jq -r '.tag_name // empty')
    else
        VERSION=$(printf '%s' "$body" \
            | grep '"tag_name"' \
            | head -1 \
            | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
    fi
    spinner_stop

    if [ -z "$VERSION" ] && printf '%s' "$body" | grep -qiE 'rate.?limit exceeded|secondary rate limit'; then
        warn "GitHub API rate-limit style response — retry later or export GITHUB_TOKEN / GITHUB_API_TOKEN for authenticated requests."
    fi

    if [ -n "$VERSION" ]; then
        ok "Latest release: ${BOLD}${VERSION}${RESET}"
        return
    fi

    spinner_start "No releases — checking latest commit..."
    local commits
    commits=$(curl_github_rest "${GITHUB_API}/commits/main")
    if has jq; then
        VERSION=$(printf '%s' "$commits" | jq -r '.sha // empty' | cut -c1-8)
    else
        VERSION=$(printf '%s' "$commits" \
            | grep '"sha"' \
            | head -1 \
            | sed -E 's/.*"sha": *"([^"]+)".*/\1/' \
            | cut -c1-8)
    fi
    spinner_stop

    [ -n "$VERSION" ] || die "Could not determine version"
    info "Latest commit: ${VERSION}"
}

# ── Checksum Verification ────────────────────────────────────────────────────
verify_checksum() {
    local archive="$1" checksum_file="$2"

    if [ "$NO_VERIFY" = "1" ]; then
        printf '\n'
        printf '  %b%b ⚠  WARNING %b  Checksum verification DISABLED (NO_VERIFY=1)\n' "$BG_RED" "$WHITE" "$RESET"
        printf '  %b  The binary has NOT been verified for integrity.%b\n' "$YELLOW" "$RESET"
        return
    fi

    if [[ "$VERSION" =~ ^[a-f0-9]{8}$ ]]; then
        warn "Skipping checksum for commit build"
        return
    fi

    spinner_start "Verifying checksum..."
    local result=0
    if command -v shasum > /dev/null 2>&1; then
        ( cd "$(dirname "$archive")" && shasum -a 256 -c "$checksum_file" > /dev/null 2>&1 ) || result=1
    elif command -v sha256sum > /dev/null 2>&1; then
        ( cd "$(dirname "$archive")" && sha256sum -c "$checksum_file" > /dev/null 2>&1 ) || result=1
    else
        spinner_stop
        die "No checksum tool found (need shasum or sha256sum). Refusing insecure install."
    fi
    spinner_stop

    [ "$result" = 0 ] || die "Checksum verification failed"
    ok "Checksum verified (SHA-256)"
}


# Create short-command symlink: rp -> lazyxrp (same INSTALL_DIR).
# Skip if a real `rp` binary is already present (cargo install / release archive).
link_rp_alias() {
    local dest="${INSTALL_DIR}/${BIN_NAME}"
    local alias_path="${INSTALL_DIR}/rp"
    [ -x "$dest" ] || die "Cannot link rp: missing ${dest}"
    if [ -e "$alias_path" ] && [ ! -L "$alias_path" ]; then
        ok "Short command already present: ${BOLD}${alias_path}${RESET}"
        return 0
    fi
    # Relative symlink so the install dir stays movable.
    ln -sfn "${BIN_NAME}" "$alias_path"
    ok "Short command linked: ${BOLD}${alias_path}${RESET} -> ${BIN_NAME}"
}

# ── Install Methods ──────────────────────────────────────────────────────────
install_via_cargo() {
    local repo_root="$1"
    local dest_bin cargo_root

    has cargo || die "cargo is required for source build (run installer again to install Rust)"

    if [ "$(basename "$INSTALL_DIR")" = bin ]; then
        cargo_root=$(dirname "$INSTALL_DIR")
        dest_bin="${INSTALL_DIR}/${BIN_NAME}"
    else
        warn "INSTALL_DIR is not a .../bin path — using ~/.cargo/bin"
        INSTALL_DIR="$HOME/.cargo/bin"
        dest_bin="${INSTALL_DIR}/${BIN_NAME}"
        cargo_root=""
    fi

    step "Building from source"
    printf '\n'

    local cargo_cmd="cargo install --locked --path ${repo_root}"
    [ -n "$cargo_root" ] && cargo_cmd="${cargo_cmd} --root ${cargo_root}"

    info "Running: ${DIM}${cargo_cmd}${RESET}"
    printf '\n'

    if [ -n "$cargo_root" ]; then
        cargo install --locked --path "$repo_root" --root "$cargo_root"
    else
        cargo install --locked --path "$repo_root"
    fi

    [ -x "$dest_bin" ] || die "Binary not found: ${dest_bin}"
    ok "Binary installed: ${BOLD}${dest_bin}${RESET}"
    link_rp_alias
}

install_via_binary() {
    if [[ "$VERSION" =~ ^[a-f0-9]{8}$ ]]; then
        die "No GitHub Releases yet — prebuilt archives unavailable.
Install from source instead:
  git clone https://github.com/${REPO}.git && cd lazyxrp && ./install.sh
  cargo install --git https://github.com/${REPO}.git"
    fi

    ASSET_BASE="${BIN_NAME}-${VERSION}-${OS_NAME}-${ARCH_NAME}"
    ARCHIVE="${ASSET_BASE}.tar.gz"
    CHECKSUM="${ASSET_BASE}.sha256"
    DOWNLOAD_URL="${GITHUB_RELEASES}/${VERSION}/${ARCHIVE}"
    CHECKSUM_URL="${GITHUB_RELEASES}/${VERSION}/${CHECKSUM}"

    step "Downloading"
    info "URL: ${DIM}${DOWNLOAD_URL}${RESET}"

    TMP_DIR=$(mktemp -d)
    chmod 700 "$TMP_DIR"
    TMP_DOWNLOAD_DIR="$TMP_DIR"

    if [ "$IS_TTY" = 1 ]; then
        printf '\n'
        progress_bar 0 4 "Preparing..."
        sleep_tick 0.2
        progress_bar 1 4 "Downloading archive..."
    fi
    fetch_file "$DOWNLOAD_URL" "${TMP_DIR}/${ARCHIVE}"
    [ "$IS_TTY" = 1 ] && progress_bar 2 4 "Downloading checksum..."
    fetch_file "$CHECKSUM_URL" "${TMP_DIR}/${CHECKSUM}"
    [ "$IS_TTY" = 1 ] && progress_bar 3 4 "Verifying..."
    printf '\n\n'

    ok "Download complete"

    step "Verifying"
    verify_checksum "${TMP_DIR}/${ARCHIVE}" "${TMP_DIR}/${CHECKSUM}"

    step "Installing"
    spinner_start "Verifying archive members..."
    while IFS= read -r member; do
        case "$member" in
            "" ) continue ;;
            */* | *..* | /* | ./*/* )
                spinner_stop
                die "Refusing unsafe tar member: ${member}"
                ;;
            "${BIN_NAME}" | "rp" | "./${BIN_NAME}" | "./rp" ) ;;
            * )
                spinner_stop
                die "Unexpected tar member: ${member} (expected ${BIN_NAME} and optional rp)"
                ;;
        esac
    done < <(tar -tzf "${TMP_DIR}/${ARCHIVE}")
    spinner_stop

    spinner_start "Extracting ${ARCHIVE}..."
    tar -xzf "${TMP_DIR}/${ARCHIVE}" -C "$TMP_DIR"
    spinner_stop

    EXTRACTED="${TMP_DIR}/${BIN_NAME}"
    [ -f "$EXTRACTED" ] || die "Binary not found after extraction"
    EXTRACTED_RP="${TMP_DIR}/rp"

    if [ ! -d "$INSTALL_DIR" ]; then
        info "Creating ${INSTALL_DIR}"
        mkdir -p "$INSTALL_DIR"
    fi

    DEST="${INSTALL_DIR}/${BIN_NAME}"

    PARTIAL_BIN_DEST="${DEST}.partial.$$"
    cp "$EXTRACTED" "$PARTIAL_BIN_DEST"
    chmod 755 "$PARTIAL_BIN_DEST"

    if [ -f "$DEST" ]; then
        OLD_VER=$("$DEST" --version 2>/dev/null | head -1 || echo "unknown")
        info "Replacing existing binary (${OLD_VER})"
        mv "$DEST" "${DEST}.bak"
    fi

    mv "$PARTIAL_BIN_DEST" "$DEST"
    PARTIAL_BIN_DEST=""

    if [ -f "$EXTRACTED_RP" ]; then
        RP_DEST="${INSTALL_DIR}/rp"
        PARTIAL_RP_DEST="${RP_DEST}.partial.$$"
        cp "$EXTRACTED_RP" "$PARTIAL_RP_DEST"
        chmod 755 "$PARTIAL_RP_DEST"
        mv "$PARTIAL_RP_DEST" "$RP_DEST"
        PARTIAL_RP_DEST=""
        ok "Short command installed: ${BOLD}${RP_DEST}${RESET}"
    else
        link_rp_alias
    fi

    rm -rf "$TMP_DIR"
    TMP_DOWNLOAD_DIR=""
    PARTIAL_BIN_DEST=""

    ok "Binary installed: ${BOLD}${DEST}${RESET}"
}

# ── System Info Card ─────────────────────────────────────────────────────────
print_system_info() {
    [ "$IS_TTY" = 0 ] && return

    printf '\n'
    printf '  %b┌─────────────────────────────────────────────┐%b\n' "$DIM" "$RESET"
    printf '  %b│%b  %b%-12s%b %s%-30s%b %b│%b\n' "$DIM" "$RESET" "$BOLD" "Platform" "$RESET" "" "${OS_DISPLAY} ${ARCH_DISPLAY}" "$RESET" "$DIM" "$RESET"

    local rust_ver="not installed"
    if command -v rustc > /dev/null 2>&1; then
        rust_ver=$(rustc --version 2>/dev/null | sed 's/rustc //')
    fi
    printf '  %b│%b  %b%-12s%b %s%-30s%b %b│%b\n' "$DIM" "$RESET" "$BOLD" "Rust" "$RESET" "" "$rust_ver" "$RESET" "$DIM" "$RESET"

    local cargo_status cargo_color
    if has cargo; then
        cargo_status="$(cargo --version 2>/dev/null | head -1)"
        cargo_color="$GREEN"
    else
        cargo_status="not installed"
        cargo_color="$YELLOW"
    fi
    printf '  %b│%b  %b%-12s%b %b%-30s%b %b│%b\n' "$DIM" "$RESET" "$BOLD" "Cargo" "$RESET" "$cargo_color" "$cargo_status" "$RESET" "$DIM" "$RESET"

    local mise_status mise_color
    if has mise; then
        mise_status="$(mise --version 2>/dev/null | head -1)"
        mise_color="$GREEN"
    else
        mise_status="not installed"
        mise_color="$YELLOW"
    fi
    printf '  %b│%b  %b%-12s%b %b%-30s%b %b│%b\n' "$DIM" "$RESET" "$BOLD" "mise" "$RESET" "$mise_color" "$mise_status" "$RESET" "$DIM" "$RESET"

    printf '  %b│%b  %b%-12s%b %s%-30s%b %b│%b\n' "$DIM" "$RESET" "$BOLD" "Install to" "$RESET" "" "$INSTALL_DIR" "$RESET" "$DIM" "$RESET"
    printf '  %b└─────────────────────────────────────────────┘%b\n' "$DIM" "$RESET"
}

# ── Success Summary ──────────────────────────────────────────────────────────
print_success() {
    local dest="$1"
    local ver

    ver=$("$dest" --version 2>/dev/null | head -1 || echo "unknown")

    if [ "$IS_TTY" = 1 ]; then
        printf '\n'
        printf '  %b%b ✔  SUCCESS %b\n' "$BG_GREEN" "$WHITE" "$RESET"
        printf '\n'
        printf '  %b┌─────────────────────────────────────────────┐%b\n' "$GREEN" "$RESET"
        printf '  %b│%b                                             %b│%b\n' "$GREEN" "$RESET" "$GREEN" "$RESET"
        printf '  %b│%b    %b%blazyxrp%b installed successfully!        %b│%b\n' "$GREEN" "$RESET" "$BOLD" "$WHITE" "$RESET" "$GREEN" "$RESET"
        printf '  %b│%b                                             %b│%b\n' "$GREEN" "$RESET" "$GREEN" "$RESET"
        printf '  %b│%b    Version : %b%-30s%b %b│%b\n' "$GREEN" "$RESET" "$CYAN" "$ver" "$RESET" "$GREEN" "$RESET"
        printf '  %b│%b    Path    : %b%-30s%b %b│%b\n' "$GREEN" "$RESET" "$CYAN" "$dest" "$RESET" "$GREEN" "$RESET"
        printf '  %b│%b                                             %b│%b\n' "$GREEN" "$RESET" "$GREEN" "$RESET"
        printf '  %b└─────────────────────────────────────────────┘%b\n' "$GREEN" "$RESET"
    else
        ok "Version: ${ver}"
        ok "Installed: ${dest}"
    fi

    hint_path_notice

    printf '\n'
    printf '  %bGet started:%b\n' "$BOLD" "$RESET"
    printf '    %b$ %s --help%b\n' "$CYAN" "$BIN_NAME" "$RESET"
    printf '    %b$ %s watch --account <r-address>%b\n' "$CYAN" "$BIN_NAME" "$RESET"
    printf '    %b$ rp -t <txid|r-address>%b\n' "$CYAN" "$RESET"
    printf '  %bTip:%b `rp` is a short lookup command (separate binary; older archives may use a symlink)\n' "$DIM" "$RESET"
    printf '\n'
}

install_dir_in_path() {
    case ":${PATH}:" in
        *:"${INSTALL_DIR}":*) return 0 ;;
        *) return 1 ;;
    esac
}

detect_shell_profile() {
    local shell_name
    shell_name=$(basename "${SHELL:-}")
    case "$shell_name" in
        bash)
            if [ -f "$HOME/.bash_profile" ]; then
                SHELL_PROFILE="$HOME/.bash_profile"
            elif [ -f "$HOME/.bashrc" ]; then
                SHELL_PROFILE="$HOME/.bashrc"
            else
                SHELL_PROFILE="$HOME/.bash_profile"
            fi
            SHELL_RELOAD_COMMAND=". \"$SHELL_PROFILE\""
            ;;
        zsh)
            SHELL_PROFILE="$HOME/.zshrc"
            SHELL_RELOAD_COMMAND=". \"$SHELL_PROFILE\""
            ;;
        fish)
            SHELL_PROFILE="$HOME/.config/fish/config.fish"
            SHELL_RELOAD_COMMAND="source \"$SHELL_PROFILE\""
            ;;
        *)
            SHELL_PROFILE="$HOME/.profile"
            SHELL_RELOAD_COMMAND=". \"$SHELL_PROFILE\""
            ;;
    esac
}

# Append INSTALL_DIR to the active shell profile when missing from PATH.
# Interactive: ask. Non-interactive/CI: auto-append (CodeRabbit-style).
setup_path() {
    local path_export fish_path_export
    PATH_UPDATE_STATUS="not_needed"
    install_dir_in_path && return 0

    detect_shell_profile
    path_export="export PATH=\"${INSTALL_DIR}:\$PATH\""

    if [ "$IS_TTY" = 1 ]; then
        if ! prompt_yn "Add ${BOLD}${INSTALL_DIR}${RESET} to PATH in ${SHELL_PROFILE}?" "y"; then
            PATH_UPDATE_STATUS="skipped"
            printf '\n'
            printf '  %b%b ⚠  NOTE %b  %s is not in your PATH.\n' "$BG_BLUE" "$WHITE" "$RESET" "$INSTALL_DIR"
            printf '  Add manually:\n'
            printf '    %b%s%b\n' "$CYAN" "$path_export" "$RESET"
            return 1
        fi
    fi

    case "$(basename "${SHELL:-}")" in
        fish)
            fish_path_export="set -gx PATH ${INSTALL_DIR} \$PATH"
            mkdir -p "$(dirname "$SHELL_PROFILE")" 2>/dev/null || true
            if [ ! -f "$SHELL_PROFILE" ]; then
                touch "$SHELL_PROFILE" 2>/dev/null || {
                    warn "Failed to update PATH in $SHELL_PROFILE"
                    PATH_UPDATE_STATUS="failed"
                    return 1
                }
            fi
            if ! grep -qF "$fish_path_export" "$SHELL_PROFILE" 2>/dev/null; then
                if ! printf '%s\n' "$fish_path_export" >> "$SHELL_PROFILE" 2>/dev/null; then
                    warn "Failed to update PATH in $SHELL_PROFILE"
                    PATH_UPDATE_STATUS="failed"
                    return 1
                fi
            fi
            ;;
        *)
            if [ ! -f "$SHELL_PROFILE" ]; then
                touch "$SHELL_PROFILE" 2>/dev/null || {
                    warn "Failed to update PATH in $SHELL_PROFILE"
                    PATH_UPDATE_STATUS="failed"
                    return 1
                }
            fi
            if ! grep -qF "$path_export" "$SHELL_PROFILE" 2>/dev/null; then
                if ! (
                    printf '\n' >> "$SHELL_PROFILE" &&
                    printf '%s\n' '# Added by lazyxrp installer' >> "$SHELL_PROFILE" &&
                    printf '%s\n' "$path_export" >> "$SHELL_PROFILE"
                ) 2>/dev/null; then
                    warn "Failed to update PATH in $SHELL_PROFILE"
                    PATH_UPDATE_STATUS="failed"
                    return 1
                fi
            fi
            ;;
    esac

    PATH_UPDATE_STATUS="updated"
    ok "Added ${INSTALL_DIR} to PATH in ${SHELL_PROFILE}"
    warn "Restart your shell or run: ${SHELL_RELOAD_COMMAND}"
    return 0
}

hint_path_notice() {
    setup_path || true
}

# ── Cleanup on exit ──────────────────────────────────────────────────────────
cleanup() {
    spinner_stop 2>/dev/null || true
    if [ -n "${PARTIAL_BIN_DEST:-}" ] && [ -f "$PARTIAL_BIN_DEST" ]; then
        rm -f "$PARTIAL_BIN_DEST"
    fi
    PARTIAL_BIN_DEST=""
    if [ -n "${PARTIAL_RP_DEST:-}" ] && [ -f "$PARTIAL_RP_DEST" ]; then
        rm -f "$PARTIAL_RP_DEST"
    fi
    PARTIAL_RP_DEST=""
    if [ -n "${TMP_DOWNLOAD_DIR:-}" ] && [ -d "$TMP_DOWNLOAD_DIR" ]; then
        rm -rf "$TMP_DOWNLOAD_DIR"
    fi
    TMP_DOWNLOAD_DIR=""
    # `-q` forces IS_TTY=0; do not end this function with a failing `test && cmd`
    # or the EXIT trap becomes the process exit status (mise: "task failed").
    if [ "$IS_TTY" = 1 ]; then
        printf '%b' "$SHOW_CURSOR"
    fi
}
trap cleanup EXIT INT TERM

# ── Main ─────────────────────────────────────────────────────────────────────
main() {
    # Animated banner
    print_animated_banner
    print_subtitle

    # Hard requirement: curl or wget
    ensure_downloader

    # Detect platform (needed for system info display)
    detect_platform

    if [ "$IS_TTY" = 1 ]; then
        print_system_info
    else
        printf 'lazyxrp installer\n'
        printf 'Platform   : %s %s\n' "$OS_DISPLAY" "$ARCH_DISPLAY"
        printf 'Repository : https://github.com/%s\n' "$REPO"
        printf 'Install to : %s\n' "$INSTALL_DIR"
        has cargo && printf 'Cargo      : %s\n' "$(cargo --version 2>/dev/null | head -1)" \
                  || printf 'Cargo      : not installed\n'
        has mise  && printf 'mise       : %s\n' "$(mise --version 2>/dev/null | head -1)" \
                  || printf 'mise       : not installed\n'
    fi

    local repo_root
    repo_root=$(script_dir)
    local has_local_tree=0
    [ -n "$repo_root" ] && [ -f "${repo_root}/Cargo.toml" ] && has_local_tree=1

    # ── Offer to install missing tools ──────────────────────────────────────
    [ "$IS_TTY" = 1 ] && step "Checking tools"
    if has cargo; then
        ok "cargo $(cargo --version 2>/dev/null | sed 's/cargo //')"
    else
        offer_install_rustup || true
    fi
    sleep_tick 0.1
    if has mise; then
        ok "mise $(mise --version 2>/dev/null | head -1)"
    else
        offer_install_mise || true
    fi

    # Re-check cargo availability after potential install
    local has_cargo=0
    has cargo && has_cargo=1

    # Determine install method
    local method=""

    if [ -n "$CLI_METHOD" ]; then
        case "$CLI_METHOD" in
            cargo|binary) method="$CLI_METHOD" ;;
            *) die "Invalid --method: use cargo or binary" ;;
        esac
    elif [ "$BINARY_INSTALL" = "1" ]; then
        method="binary"
    elif [ "$has_local_tree" = 1 ] && [ "$has_cargo" = 1 ]; then
        if [ "$IS_TTY" = 1 ]; then
            local choice
            choice=$(prompt_choice "How do you want to install?" \
                "Build from source (cargo install)" \
                "Download prebuilt binary (GitHub Releases)")
            case "$choice" in
                1) method="cargo" ;;
                2) method="binary" ;;
                *) method="cargo" ;;
            esac
        else
            method="cargo"
            info "Local source tree found — building from source"
        fi
    else
        method="binary"
        [ "$IS_TTY" = 0 ] && info "Using prebuilt binary download"
    fi

    if [ "$method" = "cargo" ]; then
        [ "$has_local_tree" = 1 ] || die \
            "Source build requires a local clone (Cargo.toml next to install.sh). Use --method binary or clone the repo."
        has cargo || die "cargo is required for source build (install Rust or drop --no-install-rust)"
    fi

    # Custom install dir prompt (interactive only)
    if [ "$IS_TTY" = 1 ]; then
        if ! prompt_yn "Install to ${BOLD}${INSTALL_DIR}${RESET}?" "y"; then
            printf '  %bPath:%b ' "$WHITE" "$RESET"
            read -r custom_dir </dev/tty || custom_dir=""
            if [ -n "$custom_dir" ]; then
                INSTALL_DIR="$custom_dir"
                info "Install directory: ${INSTALL_DIR}"
            fi
        fi
    fi

    # Pre-flight checks
    step "Pre-flight checks"
    if has curl; then
        ok "curl $(curl --version 2>/dev/null | head -1 | sed 's/curl //' | cut -d' ' -f1)"
    else
        ok "wget $(wget --version 2>/dev/null | head -1 | sed 's/^GNU Wget //' | cut -d' ' -f1)"
    fi
    sleep_tick 0.1

    if has tar; then
        ok "tar"
    else
        [ "$method" = "binary" ] && die "tar is required for binary install"
        warn "tar not found (binary fallback unavailable)"
    fi
    sleep_tick 0.1

    if [ "$method" = "cargo" ]; then
        ok "cargo $(cargo --version 2>/dev/null | sed 's/cargo //')"
    fi

    ok "Platform: ${BOLD}${OS_DISPLAY} ${ARCH_DISPLAY}${RESET}"

    # Resolve version (binary install needs it, cargo reads Cargo.toml)
    if [ "$method" = "binary" ]; then
        step "Resolving version"
        resolve_version
    fi

    # Confirm before install
    if [ "$IS_TTY" = 1 ]; then
        printf '\n'
        printf '  %b━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━%b\n' "$DIM" "$RESET"
        printf '  %bMethod:%b  %s\n' "$BOLD" "$RESET" \
            "$([ "$method" = "cargo" ] && echo "Build from source" || echo "Prebuilt binary (${VERSION})")"
        printf '  %bTarget:%b  %s\n' "$BOLD" "$RESET" "${INSTALL_DIR}/${BIN_NAME}"
        printf '  %b━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━%b\n' "$DIM" "$RESET"

        prompt_yn "Proceed with installation?" "y" || {
            printf '\n  %bInstallation cancelled.%b\n\n' "$DIM" "$RESET"
            exit 0
        }
    fi

    # Run installation
    case "$method" in
        cargo)
            install_via_cargo "$repo_root"
            ;;
        binary)
            install_via_binary
            ;;
    esac

    # Verify & print result
    local dest="${INSTALL_DIR}/${BIN_NAME}"
    [ -x "$dest" ] || die "Installation failed — binary not found at ${dest}"

    print_success "$dest"
}

main
