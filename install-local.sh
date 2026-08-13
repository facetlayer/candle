#!/usr/bin/env bash
#
# install-local.sh — build and locally install the Rust-based Candle.
#
# This is the from-source path, for development and for platforms without a
# prebuilt release. End users normally install with ./install.sh, which
# downloads a prebuilt binary from GitHub Releases and needs no Rust toolchain.
#
# Candle is a single crate producing a single binary: `candle`. (Service
# supervision runs as `candle --monitor`, a mode of that same binary, so there is
# nothing else to install.)
#
# By default the binary is installed into ~/.cargo/bin (cargo's default). Set
# CARGO_INSTALL_ROOT to install elsewhere, e.g.:
#
#     CARGO_INSTALL_ROOT=/usr/local ./install-local.sh
#
# (the binary lands in $CARGO_INSTALL_ROOT/bin).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST="$SCRIPT_DIR/rust/Cargo.toml"

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found. Install Rust from https://rustup.rs/" >&2
  exit 1
fi

echo "==> Building Candle (Rust, release)…"
cargo build --release --manifest-path "$MANIFEST"

echo "==> Installing candle…"
cargo install --path "$SCRIPT_DIR/rust" --force

INSTALL_BIN="${CARGO_INSTALL_ROOT:-$HOME/.cargo}/bin"

# Older versions installed a separate `log-collector` sidecar here. It is no
# longer built or used; remove it so it doesn't linger on PATH.
if [ -f "$INSTALL_BIN/log-collector" ]; then
  rm -f "$INSTALL_BIN/log-collector"
  echo "==> Removed obsolete $INSTALL_BIN/log-collector"
fi

echo
echo "==> Done. Installed to $INSTALL_BIN"
echo "    candle -> $INSTALL_BIN/candle"

if ! command -v candle >/dev/null 2>&1; then
  echo
  echo "note: '$INSTALL_BIN' is not on your PATH. Add it with:" >&2
  echo "      export PATH=\"$INSTALL_BIN:\$PATH\"" >&2
fi
