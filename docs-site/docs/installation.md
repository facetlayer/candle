# Installation

Candle is a native binary. It supports macOS and Linux on both x86_64 and arm64.

Every install method places a **single** executable: `candle`. There is nothing else
to install — the per-service log monitor is a mode of that same binary
(`candle --monitor`), which Candle re-invokes for you.

## Recommended: one-line installer

```bash
curl -fsSL https://raw.githubusercontent.com/facetlayer/candle/main/install.sh | sh
```

This downloads the latest [GitHub Release](https://github.com/facetlayer/candle/releases)
for your platform, verifies its SHA-256 checksum, and installs into `~/.local/bin`.
No Rust toolchain required.

If `~/.local/bin` is not on your `PATH`, the installer tells you what to add to your
shell profile:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### Installer options

Pass options after `-s --` when piping to `sh`:

```bash
# Install a specific version
curl -fsSL .../install.sh | sh -s -- --version v0.13.3

# Install somewhere else. System directories are usually not writable by your
# user, so those need `sudo sh` rather than `sh`.
curl -fsSL .../install.sh | sudo sh -s -- --bin-dir /usr/local/bin
```

| Option | Description |
| ------ | ----------- |
| `--version <tag>` | Install a specific release tag. Default: latest. |
| `--bin-dir <dir>` | Install directory. Default: `~/.local/bin`. |
| `--uninstall` | Remove installed binaries. |

`CANDLE_VERSION` and `CANDLE_BIN_DIR` work as environment-variable equivalents.

## Homebrew

```bash
brew install facetlayer/tap/candle
```

## Build from source

Requires a [Rust toolchain](https://rustup.rs/) and a C compiler (Candle bundles SQLite,
which is compiled from source).

```bash
git clone https://github.com/facetlayer/candle.git
cd candle
./install-local.sh
```

This installs both binaries into `~/.cargo/bin`. Set `CARGO_INSTALL_ROOT` to install
elsewhere.

## Upgrading

| Installed with | Upgrade with |
| -------------- | ------------ |
| Installer script | Re-run the same `curl ... \| sh` command |
| Homebrew | `brew upgrade candle` |
| Source | `git pull && ./install-local.sh` |

Check the installed version with `candle --version`.

## Uninstalling

The installer script shuts down any running services for you, then removes the binary:

```bash
curl -fsSL https://raw.githubusercontent.com/facetlayer/candle/main/install.sh | sh -s -- --uninstall
```

The other install methods don't, so shut services down first — Candle launches them as
detached processes, and they keep running after the binary is removed:

```bash
candle kill-all

# Homebrew
brew uninstall candle

# Source install
rm ~/.cargo/bin/candle
```

Candle's only other footprint is its SQLite database, which is never removed
automatically. Delete it if you want a completely clean system:

```bash
rm -rf ~/.local/state/candle
```

(If you set `XDG_STATE_HOME` or `CANDLE_DATABASE_DIR`, the database lives under that
path instead. See [Database](database).)

Per-project `.candle.json` config files stay in your project directories; delete them
by hand if you no longer want them.
