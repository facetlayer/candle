#!/usr/bin/env bash
#
# install-local.sh — build and locally install the Rust-based Candle.
#
# This is the from-source path, for development and for platforms without a
# prebuilt release. End users normally install with ./install.sh, which
# downloads prebuilt binaries from GitHub Releases and needs no Rust toolchain.
#
# Builds the Rust workspace in release mode and installs both the `candle` CLI
# and its `log-collector` sidecar. The two binaries MUST live in the same
# directory: at runtime `candle` resolves the log-collector as a sibling of its
# own executable (see rust/candle-core/src/start/launch.rs). `cargo install`
# places both into the same bin directory, which satisfies that requirement.
#
# By default binaries are installed into ~/.cargo/bin (cargo's default). Set
# CARGO_INSTALL_ROOT to install elsewhere, e.g.:
#
#     CARGO_INSTALL_ROOT=/usr/local ./install-local.sh
#
# (binaries land in $CARGO_INSTALL_ROOT/bin).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST="$SCRIPT_DIR/rust/Cargo.toml"

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found. Install Rust from https://rustup.rs/" >&2
  exit 1
fi

echo "==> Building Candle (Rust, release)…"
cargo build --release --manifest-path "$MANIFEST"

echo "==> Installing candle + log-collector…"
cargo install --path "$SCRIPT_DIR/rust/candle-cli" --force
cargo install --path "$SCRIPT_DIR/rust/log-collector" --force

INSTALL_BIN="${CARGO_INSTALL_ROOT:-$HOME/.cargo}/bin"
echo
echo "==> Done. Installed to $INSTALL_BIN"
echo "    candle        -> $INSTALL_BIN/candle"
echo "    log-collector -> $INSTALL_BIN/log-collector"

if ! command -v candle >/dev/null 2>&1; then
  echo
  echo "note: '$INSTALL_BIN' is not on your PATH. Add it with:" >&2
  echo "      export PATH=\"$INSTALL_BIN:\$PATH\"" >&2
fi
