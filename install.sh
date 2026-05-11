#!/bin/bash
# install.sh — lazyxrp installer
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/boborder/lazyxrp/main/install.sh | sh
#
# Environment overrides:
#   INSTALL_DIR   Installation directory  (default: ~/.local/bin)
#   VERSION       Specific version to install (default: latest)
#   NO_VERIFY     Set to 1 to skip checksum verification
#
# Example — install to a custom path:
#   curl -fsSL .../install.sh | INSTALL_DIR=/usr/local/bin sh

set -eu

# ── Configuration ────────────────────────────────────────────────────────────
REPO="boborder/lazyxrp"
BIN_NAME="lazyxrp"

VERSION="${VERSION:-}"          # empty → fetch latest release tag
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
NO_VERIFY="${NO_VERIFY:-0}"
GITHUB_API="https://api.github.com/repos/${REPO}"
GITHUB_RELEASES="https://github.com/${REPO}/releases/download"

# ── Colours (skip when not a tty) ────────────────────────────────────────────
if [ -t 1 ]; then
    BOLD="\033[1m"; GREEN="\033[32m"; YELLOW="\033[33m"
    RED="\033[31m"; CYAN="\033[36m"; RESET="\033[0m"
else
    BOLD=""; GREEN=""; YELLOW=""; RED=""; CYAN=""; RESET=""
fi

info()  { printf '%binfo%b  %s\n'       "$CYAN"   "$RESET" "$*"; }
ok()    { printf '%bok%b    %s\n'         "$GREEN"  "$RESET" "$*"; }
warn()  { printf '%bwarn%b  %s\n'        "$YELLOW" "$RESET" "$*" >&2; }
die()   { printf '%b%berror%b %s\n' "$RED" "$BOLD" "$RESET" "$*" >&2; exit 1; }
step()  { printf '\n%b==> %s%b\n'        "$BOLD"   "$*" "$RESET"; }

# ── Helpers ──────────────────────────────────────────────────────────────────
need() {
    command -v "$1" > /dev/null 2>&1 || die "Required command not found: $1"
}

fetch() {
    if command -v curl > /dev/null 2>&1; then
        curl -fsSL -H "User-Agent: lazyxrp-installer/1.0" "$1"
    elif command -v wget > /dev/null 2>&1; then
        wget -qO- "$1"
    else
        die "curl or wget is required to download files"
    fi
}

fetch_file() {
    local url="$1"
    local dest="$2"
    if command -v curl > /dev/null 2>&1; then
        curl -fsSL --progress-bar -o "$dest" "$url"
    elif command -v wget > /dev/null 2>&1; then
        wget -q --show-progress -O "$dest" "$url"
    else
        die "curl or wget is required to download files"
    fi
}

# ── Platform detection ───────────────────────────────────────────────────────
detect_platform() {
    local os
    local arch

    os=$(uname -s)
    arch=$(uname -m)

    case "$os" in
        Darwin) OS_NAME="macos" ;;
        Linux)  OS_NAME="linux" ;;
        *)      die "Unsupported operating system: ${os}. Windows is not supported by this script." ;;
    esac

    case "$arch" in
        x86_64 | amd64)    ARCH_NAME="x86_64" ;;
        aarch64 | arm64)   ARCH_NAME="arm64"  ;;
        i386 | i686)       ARCH_NAME="i686"   ;;
        *)                 die "Unsupported architecture: ${arch}" ;;
    esac
}

# ── Version resolution ───────────────────────────────────────────────────────
resolve_version() {
    if [ -n "$VERSION" ]; then
        info "Using requested version: ${VERSION}"
        return
    fi
    info "Fetching latest release..."
    VERSION=$(fetch "${GITHUB_API}/releases/latest" \
        | grep '"tag_name"' \
        | head -1 \
        | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
    if [ -n "$VERSION" ]; then
        info "Latest release: ${VERSION}"
        return
    fi
    # Fallback: fetch latest commit SHA if no releases exist
    info "No releases found — falling back to latest commit SHA..."
    VERSION=$(fetch "${GITHUB_API}/commits/main" \
        | grep '"sha"' \
        | head -1 \
        | sed -E 's/.*"sha": *"([^"]+)".*/\1/' \
        | cut -c1-8)
    [ -n "$VERSION" ] || die "Could not determine version from GitHub API"
    info "Latest commit: ${VERSION}"
}

# ── Checksum verification ────────────────────────────────────────────────────
verify_checksum() {
    local archive="$1"
    local checksum_file="$2"

    if [ "$NO_VERIFY" = "1" ]; then
        # Security: skipping checksum verification means the binary is not verified
        # against the official release. Only use in trusted, air-gapped environments.
        printf '%b%bWARNING%b Checksum verification DISABLED (NO_VERIFY=1).\n' "$RED" "$BOLD" "$RESET" >&2
        printf '%b         The downloaded binary has NOT been verified for integrity.%b\n' "$YELLOW" "$RESET" >&2
        return
    fi

    # For commit builds (fallback when no releases exist), skip checksum verification
    if [[ "$VERSION" =~ ^[a-f0-9]{8}$ ]]; then
        warn "Skipping checksum verification for commit build"
        return
    fi

    info "Verifying checksum..."
    if command -v shasum > /dev/null 2>&1; then
        # macOS / BSD
        # .sha256 format: "<hash>  <filename>" — strip directory prefix for -c
        ( cd "$(dirname "$archive")" && shasum -a 256 -c "$checksum_file" ) \
            || die "Checksum verification failed"
    elif command -v sha256sum > /dev/null 2>&1; then
        # Linux
        ( cd "$(dirname "$archive")" && sha256sum -c "$checksum_file" ) \
            || die "Checksum verification failed"
    else
        warn "Neither shasum nor sha256sum found — skipping checksum verification"
    fi
    ok "Checksum verified"
}

# ── Main ─────────────────────────────────────────────────────────────────────
main() {
    printf '%blazyxrp installer%b\n' "$BOLD" "$RESET"
    printf 'Repository : https://github.com/%s\n' "$REPO"
    printf 'Install to : %s\n' "$INSTALL_DIR"

    step "Checking requirements"
    need uname
    need tar

    step "Detecting platform"
    detect_platform
    ok "Platform: ${OS_NAME}-${ARCH_NAME}"

    step "Resolving version"
    resolve_version
    ok "Version: ${VERSION}"

    # Build asset names
    ASSET_BASE="${BIN_NAME}-${VERSION}-${OS_NAME}-${ARCH_NAME}"
    ARCHIVE="${ASSET_BASE}.tar.gz"
    CHECKSUM="${ASSET_BASE}.sha256"
    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE}"
    CHECKSUM_URL="https://github.com/${REPO}/releases/download/${VERSION}/${CHECKSUM}"

    step "Downloading"
    info "URL: ${DOWNLOAD_URL}"

    TMP_DIR=$(mktemp -d)
    trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

    fetch_file "$DOWNLOAD_URL"  "${TMP_DIR}/${ARCHIVE}"
    fetch_file "$CHECKSUM_URL"  "${TMP_DIR}/${CHECKSUM}"

    ok "Download complete"

    step "Verifying"
    verify_checksum "${TMP_DIR}/${ARCHIVE}" "${TMP_DIR}/${CHECKSUM}"

    step "Installing"
    info "Extracting ${ARCHIVE}..."
    tar -xzf "${TMP_DIR}/${ARCHIVE}" -C "$TMP_DIR"

    EXTRACTED="${TMP_DIR}/${BIN_NAME}"
    [ -f "$EXTRACTED" ] || die "Binary not found after extraction: ${EXTRACTED}"

    # Create install directory if needed
    if [ ! -d "$INSTALL_DIR" ]; then
        info "Creating ${INSTALL_DIR}"
        mkdir -p "$INSTALL_DIR"
    fi

    DEST="${INSTALL_DIR}/${BIN_NAME}"

    # Back up existing binary if present
    if [ -f "$DEST" ]; then
        OLD_VER=$("$DEST" --version 2>/dev/null | head -1 || echo "unknown")
        info "Replacing existing binary (${OLD_VER})"
        mv "$DEST" "${DEST}.bak"
    fi

    cp "$EXTRACTED" "$DEST"
    chmod 755 "$DEST"

    ok "Installed: ${DEST}"

    step "Verifying installation"
    INSTALLED_VER=$("$DEST" --version 2>/dev/null | head -1 || echo "unknown")
    ok "Version: ${INSTALLED_VER}"

    # ── PATH hint ────────────────────────────────────────────────────────────
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*)
            ;;
        *)
            printf '\n%b%bNote:%b %s is not in your PATH.\n' "$YELLOW" "$BOLD" "$RESET" "$INSTALL_DIR"
            printf 'Add the following to your shell profile:\n\n'
            printf '  %bexport PATH="$PATH:%s"%b\n\n' "$CYAN" "$INSTALL_DIR" "$RESET"
            ;;
    esac

    printf '\n%b%bDone!%b Run %b%s --help%b to get started.\n' "$GREEN" "$BOLD" "$RESET" "$CYAN" "$BIN_NAME" "$RESET"
    printf 'Quick start: %b%s watch --account <r-address>%b\n\n' "$CYAN" "$BIN_NAME" "$RESET"
}

main "$@"
