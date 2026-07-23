#!/usr/bin/env bash
# install-local.sh — build the release binary from source and install it locally.
#
# For the maintainer's own machine: installs the just-built version immediately,
# without the GitHub release + download round-trip that install.sh / `update` use.
#
# Usage:
#   ./scripts/install-local.sh
#   INSTALL_DIR=/usr/local/bin ./scripts/install-local.sh

set -euo pipefail

INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
BIN_NAME="${BIN_NAME:-jj-prompt-launcher}"

err()  { printf 'error: %s\n' "$*" >&2; exit 1; }
info() { printf '%s\n' "$*"; }

command -v cargo >/dev/null 2>&1 || err "cargo is required"
case "$(uname -s)" in
  Darwin) ;;
  *) err "unsupported OS: $(uname -s) (only macOS is supported)" ;;
esac

# Resolve repo root (parent of this script's dir) so it runs from any cwd.
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

info "==> Building release (native arch)"
( cd "$repo_root" && cargo build --release )

src="${repo_root}/target/release/${BIN_NAME}"
[ -x "$src" ] || err "built binary not found: $src"

mkdir -p "$INSTALL_DIR"
dest="${INSTALL_DIR}/${BIN_NAME}"
# Atomic replace via a temp on the same filesystem (safe even if dest is running).
tmp="${INSTALL_DIR}/.${BIN_NAME}.install.$$"
cp -f "$src" "$tmp"
chmod +x "$tmp"
mv -f "$tmp" "$dest"

info "==> Installed: $dest"
"$dest" version || true

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    info ""
    info "warning: $INSTALL_DIR is not on your PATH."
    info "add to your shell rc:"
    info "    export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac
