#!/usr/bin/env bash
# install.sh — download the latest jj-agentic-launcher binary from GitHub Releases.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/<owner>/<repo>/main/install.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/<owner>/<repo>/main/install.sh | VERSION=v0.1.0 bash
#   INSTALL_DIR=/usr/local/bin ./install.sh

set -euo pipefail

REPO="${REPO:-yigegongjiang/jj-agentic-launcher}"
VERSION="${VERSION:-latest}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
# Cargo.toml package name == repo name == binary name == "jj-agentic-launcher".
BIN_NAME="${BIN_NAME:-jj-agentic-launcher}"

err()  { printf 'error: %s\n' "$*" >&2; exit 1; }
info() { printf '%s\n' "$*"; }

command -v curl >/dev/null 2>&1 || err "curl is required"

case "$(uname -s)" in
  Darwin) ;;
  *) err "unsupported OS: $(uname -s) (only macOS is supported)" ;;
esac
case "$(uname -m)" in
  x86_64|amd64)  host_arch="x64" ;;
  aarch64|arm64) host_arch="arm64" ;;
  *) err "unsupported macOS architecture: $(uname -m)" ;;
esac

asset="${BIN_NAME}-darwin-${host_arch}"

if [ "$VERSION" = "latest" ]; then
  base="https://github.com/${REPO}/releases/latest/download"
else
  base="https://github.com/${REPO}/releases/download/${VERSION}"
fi
asset_url="${base}/${asset}"
checksums_url="${base}/checksums.txt"

info "==> Installing ${BIN_NAME} ${VERSION}"
info "    repo:   ${REPO}"
info "    target: ${INSTALL_DIR}/${BIN_NAME}"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

tmp_asset="${tmpdir}/${asset}"
info "==> Downloading"
# `--progress-bar` shows a curl-native bar to stderr; mirrors `update` subcommand's progress UX.
curl -fL --progress-bar --retry 3 -o "$tmp_asset" "$asset_url" || err "download failed: $asset_url"

# Verify SHA256 if checksums.txt is published with the release.
if hash_line="$(curl -fsSL --retry 3 "$checksums_url" 2>/dev/null | grep " ${asset}$" || true)"; then
  if [ -n "$hash_line" ]; then
    expected="${hash_line%% *}"
    actual="$(shasum -a 256 "$tmp_asset" | awk '{print $1}')"
    [ "$expected" = "$actual" ] || err "checksum mismatch for ${asset} (expected ${expected}, got ${actual})"
    info "==> Checksum OK"
  fi
fi

mkdir -p "$INSTALL_DIR"
dest="${INSTALL_DIR}/${BIN_NAME}"
chmod +x "$tmp_asset"
mv -f "$tmp_asset" "$dest"

info "==> Installed: $dest"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    info ""
    info "warning: $INSTALL_DIR is not on your PATH."
    info "add to your shell rc:"
    info "    export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac
