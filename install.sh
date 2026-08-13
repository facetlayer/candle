#!/bin/sh
#
# install.sh — install Candle from prebuilt GitHub Release binaries.
#
# Usage:
#     curl -fsSL https://raw.githubusercontent.com/facetlayer/candle/main/install.sh | sh
#
# Options (pass after `| sh -s --` when piping):
#     --version <tag>    Install a specific release (e.g. v0.13.3). Default: latest.
#     --bin-dir <dir>    Where to install. Default: $HOME/.local/bin.
#     --uninstall        Remove the installed binary (and optionally the database).
#     --help             Show this message.
#
# Environment overrides: CANDLE_VERSION, CANDLE_BIN_DIR.
#
# Candle ships as a single binary: `candle`.

set -eu

REPO="facetlayer/candle"
BIN_DIR="${CANDLE_BIN_DIR:-$HOME/.local/bin}"
VERSION="${CANDLE_VERSION:-latest}"
ACTION="install"
BINARY="candle"

say() { printf '%s\n' "$*"; }
err() { printf 'error: %s\n' "$*" >&2; exit 1; }

usage() {
  # Print the leading comment block (everything after the shebang, up to the
  # first line that isn't a comment), with the '#' markers stripped.
  awk 'NR==1 {next} /^#/ {sub(/^# ?/, ""); print; next} {exit}' "$0"
  exit 0
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="${2:-}"; [ -n "$VERSION" ] || err "--version needs a value"; shift 2 ;;
    --bin-dir) BIN_DIR="${2:-}"; [ -n "$BIN_DIR" ] || err "--bin-dir needs a value"; shift 2 ;;
    --uninstall) ACTION="uninstall"; shift ;;
    --help|-h) usage ;;
    *) err "unknown option: $1 (try --help)" ;;
  esac
done

need() { command -v "$1" >/dev/null 2>&1 || err "'$1' is required but was not found"; }

detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Darwin) os_part="apple-darwin" ;;
    Linux)  os_part="unknown-linux-gnu" ;;
    *) err "unsupported operating system: $os. Candle supports macOS and Linux; build from source with ./install-local.sh" ;;
  esac
  case "$arch" in
    x86_64|amd64) arch_part="x86_64" ;;
    arm64|aarch64) arch_part="aarch64" ;;
    *) err "unsupported architecture: $arch" ;;
  esac
  printf '%s-%s' "$arch_part" "$os_part"
}

resolve_version() {
  if [ "$VERSION" != "latest" ]; then
    printf '%s' "$VERSION"
    return
  fi
  # Follow the /releases/latest redirect and read the tag off the final URL.
  # With no published releases GitHub redirects to /releases instead of
  # /releases/tag/<tag>, so require the /tag/ segment before trusting the result.
  url="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" 2>/dev/null || true)"
  case "$url" in
    */tag/*) tag="${url##*/tag/}" ;;
    *) tag="" ;;
  esac
  case "$tag" in
    ''|*/*) err "could not determine the latest release of $REPO. Check https://github.com/$REPO/releases, or pass --version <tag>." ;;
  esac
  printf '%s' "$tag"
}

do_uninstall() {
  removed=0
  for dir in "$BIN_DIR" "$HOME/.cargo/bin" "/usr/local/bin"; do
    if [ -f "$dir/$BINARY" ]; then
      rm -f "$dir/$BINARY"
      say "removed $dir/$BINARY"
      removed=1
    fi
  done
  # Older installs also placed a `log-collector` sidecar next to `candle`; it is
  # no longer shipped (that mode now lives inside `candle` itself), so clean it up.
  for dir in "$BIN_DIR" "$HOME/.cargo/bin" "/usr/local/bin"; do
    if [ -f "$dir/log-collector" ]; then
      rm -f "$dir/log-collector"
      say "removed $dir/log-collector (obsolete sidecar)"
      removed=1
    fi
  done
  [ "$removed" -eq 1 ] || say "no Candle binary found in $BIN_DIR, ~/.cargo/bin, or /usr/local/bin"

  state_dir="${CANDLE_DATABASE_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/candle}"
  say ""
  say "Candle's database was not removed. To delete it:"
  say "    rm -rf \"$state_dir\""
  say ""
  say "Note: any services still running were launched as detached processes and"
  say "keep running after uninstall. Run 'candle kill-all' before uninstalling to"
  say "shut them down."
  exit 0
}

[ "$ACTION" = "uninstall" ] && do_uninstall

need curl
need tar
need uname

TARGET="$(detect_target)"
TAG="$(resolve_version)"
ARCHIVE="candle-$TAG-$TARGET.tar.gz"
BASE_URL="https://github.com/$REPO/releases/download/$TAG"

say "==> Installing Candle $TAG ($TARGET)"

TMP_DIR="$(mktemp -d)"
cleanup() { rm -rf "$TMP_DIR"; }
trap cleanup EXIT INT TERM

say "==> Downloading $ARCHIVE"
curl -fsSL "$BASE_URL/$ARCHIVE" -o "$TMP_DIR/$ARCHIVE" \
  || err "download failed. Is there a release named '$TAG' with an asset for $TARGET?"

# Verify the checksum when a SHA256SUMS asset and a local sha256 tool are both available.
if curl -fsSL "$BASE_URL/SHA256SUMS" -o "$TMP_DIR/SHA256SUMS" 2>/dev/null; then
  if command -v shasum >/dev/null 2>&1; then
    sha_cmd="shasum -a 256"
  elif command -v sha256sum >/dev/null 2>&1; then
    sha_cmd="sha256sum"
  else
    sha_cmd=""
  fi
  if [ -n "$sha_cmd" ]; then
    expected="$(grep " $ARCHIVE\$" "$TMP_DIR/SHA256SUMS" | awk '{print $1}')"
    actual="$($sha_cmd "$TMP_DIR/$ARCHIVE" | awk '{print $1}')"
    if [ -n "$expected" ] && [ "$expected" != "$actual" ]; then
      err "checksum mismatch for $ARCHIVE (expected $expected, got $actual)"
    fi
    say "==> Checksum verified"
  fi
fi

tar -xzf "$TMP_DIR/$ARCHIVE" -C "$TMP_DIR"

if ! mkdir -p "$BIN_DIR" 2>/dev/null; then
  err "cannot create '$BIN_DIR' (permission denied). Re-run with sudo, or choose a
       writable directory with --bin-dir (the default, ~/.local/bin, needs no sudo)."
fi
if [ ! -w "$BIN_DIR" ]; then
  err "'$BIN_DIR' is not writable. Re-run with sudo, or choose a writable directory
       with --bin-dir (the default, ~/.local/bin, needs no sudo)."
fi

src="$(find "$TMP_DIR" -type f -name "$BINARY" | head -n 1)"
[ -n "$src" ] || err "'$BINARY' was not found inside $ARCHIVE"
install -m 755 "$src" "$BIN_DIR/$BINARY" 2>/dev/null || {
  cp "$src" "$BIN_DIR/$BINARY" && chmod 755 "$BIN_DIR/$BINARY"
}
say "    $BINARY -> $BIN_DIR/$BINARY"

# Candle used to ship a `log-collector` sidecar alongside `candle`. That mode now
# lives inside `candle` itself (`candle --monitor`), so remove any leftover copy
# from an earlier install rather than leaving a dead binary on the user's PATH.
if [ -f "$BIN_DIR/log-collector" ]; then
  rm -f "$BIN_DIR/log-collector"
  say "    removed obsolete $BIN_DIR/log-collector"
fi

say ""
say "==> Done. Installed Candle $TAG to $BIN_DIR"

case ":$PATH:" in
  *":$BIN_DIR:"*) say "    Run 'candle --help' to get started." ;;
  *)
    say ""
    say "note: '$BIN_DIR' is not on your PATH. Add it to your shell profile:"
    say "      export PATH=\"$BIN_DIR:\$PATH\""
    ;;
esac
