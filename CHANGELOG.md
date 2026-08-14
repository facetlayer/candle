
# Unreleased
 - `candle list` (alias `ls`) is now a multiline detail view: one entry per service with a `name STATUS pid uptime` header line, followed by the service's full `command:` and `directory:`, neither of them truncated. An empty project still prints `No services configured.`
 - Add `candle ps` for the old table view, minus the COMMAND and DIRECTORY columns so it fits a narrow terminal: just `NAME STATUS PID UPTIME`. `status` is now an alias of `ps` instead of `list`.
 - `candle list` and `candle ps` both accept optional service names to filter the listing, and both still support `--json`. An unknown service name is an error naming that service, with a non-zero exit.
 - Fix the `command` field of a listing reporting the service's *name* instead of the shell command it runs. It's now the shell recorded on the running process, falling back to the configured service's `shell`. This corrects `--json` and the MCP listing as well as the printed output.
 - The launch banner printed by `candle start` / `candle run` is now two lines: `[Started process 'name'] $ <shell>` followed by `[With root directory: <dir>]`.
 - `install.sh --uninstall` now runs `candle kill-all` itself before removing the binary, instead of telling you to do it first. Services are launched detached, so they'd otherwise keep running with no CLI left to manage them. A missing binary or a failing `kill-all` doesn't stop the uninstall.
 - Agent-mode detection now recognizes Gemini CLI (`GEMINI_CLI`) and Cursor (`CURSOR_AGENT`) in addition to Claude Code (`CLAUDECODE`). Any one of them, set to a non-empty value, puts Candle in agent mode. Codex is not detected by an env var: its `CODEX_SANDBOX` signals an active sandbox rather than the agent, and is unset under `--sandbox danger-full-access` — Codex still gets non-interactive behavior from the stdout TTY check.
 - Candle now ships as a **single binary**. The separate `log-collector` sidecar is gone; its behavior moved into the main CLI as a mode, `candle --monitor`, which the CLI launches by re-invoking its own executable. Installation is one file, and the CLI and its monitors can no longer fall out of version sync. `install.sh` and `install-local.sh` remove a leftover `log-collector` from a previous install.
 - Remove the `logCollector` config setting. It chose between the old Node.js and Rust collector sidecars, neither of which exists now, and it had already been ignored at launch. `candle set-config logCollector ...` reports an unknown key; a leftover `"logCollector"` entry in an existing `.candle.json` is ignored and preserved as-is.
 - Reorganize the Rust source into one crate at `rust/` (was a workspace of `candle-core` + `candle-cli` + `log-collector`), now that there is only one binary to build.
 - `install.sh` now fails with an actionable message when the target directory isn't writable (e.g. `--bin-dir /usr/local/bin`), instead of a raw `mkdir: Permission denied`.
 - Expand the README installation section: verifying the install, the `PATH` note for `~/.local/bin`, upgrading, and uninstalling.

# 0.14.0
 - Candle now detects whether it's running interactively (a human at a terminal) or non-interactively (a coding agent, script, or pipe). Detection: non-interactive when stdout is not a TTY or when `CLAUDECODE` is set.
 - `candle start` (and `candle run`) in interactive mode now stays attached after launching and watches the new process's logs — only logs from the fresh launch, no stale history. Ctrl+C detaches and leaves the process running. In non-interactive mode it exits as soon as the launch is confirmed and prints a hint pointing at `candle logs`.
 - Add `--watch` (force interactive/watch mode) and `--bg` (force non-interactive mode) flags to `candle start` and `candle restart`.
 - `candle restart` follows the same interactive/non-interactive behavior as `start`.
 - `candle watch` no longer launches processes — it only observes. With no names it always succeeds and watches every process in the project (including ones that haven't launched yet); with a name, the named process must be running or the command fails.
 - `candle check-start` always exits immediately (never watches), keeping it predictable for scripts.
 - Fix a "failed printing to stdout: Broken pipe" panic when piping watch output (e.g. `candle watch | head`); candle now exits quietly when the reader closes the pipe.
 - A process terminated by a signal (e.g. via `candle stop` or `restart`) now logs `Process was stopped` instead of `Process exited with code null`; a signal-killed process during the startup grace period logs `Process failed to start: stopped by a signal`.
 - Distribute prebuilt binaries. Pushing a `v*` tag now runs `.github/workflows/release.yml`, which builds `candle` + `log-collector` for macOS and Linux (x86_64 and arm64), publishes them as a GitHub Release with a `SHA256SUMS` file, and updates the Homebrew tap.
 - Add `install.sh`, a one-line installer (`curl -fsSL .../install.sh | sh`) that downloads the matching release for the host platform, verifies its checksum, and installs into `~/.local/bin`. Supports `--version`, `--bin-dir`, and `--uninstall`.
 - Add a LICENSE file (MIT, matching the license already declared in `rust/Cargo.toml`) and fill in package metadata (description, repository, authors, keywords) for all three crates.
 - Fix the documentation site still telling users to `npm install -g @facetlayer/candle`, which installs the retired Node implementation. Added a dedicated Installation page covering install, upgrade, and uninstall, and replaced the `your-org` / `your-domain.com` placeholders in the Docusaurus config.
 - Remove the legacy Node.js/TypeScript implementation now that the Rust port is complete. Candle is built and installed from source via `./install-local.sh`; the `@facetlayer/candle` npm package is retired. The Vitest acceptance suite is retained and runs against the Rust binary.
 - Print `candle <version>` as the first line of `candle --help`. The version is injected at build time from the workspace `version` in `rust/Cargo.toml` (Cargo's `CARGO_PKG_VERSION`), now the single source of truth for both `--help` and `--version`.

# 0.13.3
 - Fix `candle restart` ignoring edits to a config-defined service's `shell`/`root`. It now reloads the service definition from `.candle.json` on restart, so config edits take effect. Transient processes (started with `--shell`) still relaunch with their stored command.
 - Fix CI by authenticating to GitHub Packages so pnpm's supply-chain policy check can verify `@facetlayer/*` dependencies hosted there.

# 0.13.2
 - Fix `candle watch` printing no log output. The 10-second recency window compared second-resolution log timestamps against a millisecond cutoff, so every log line was filtered out.
 - Add unit tests for `LatestExecutionLogFilter` and integration tests for the `watch` command.

# 0.13.1
 - Fix `candle check-start` skipping a service when the DB has a stale `killed_at=null` row with a dead PID (post-reboot). It now verifies the PID is alive and clears the stale row before deciding.
 - Promote `filterAliveProcesses` to `process-alive.ts` and use it in both `handleList` and the `check-start` path.

# 0.13.0
 - Fix log-collector processes leaking as orphans after service exit (missing clearInterval and process.exit)
 - Add `candle remove-service` command to remove a service from .candle.json
 - `candle watch` now launches services that aren't running before watching them
 - `candle watch` trims initial output to a 10-second window, so long-running services don't spam history on attach
 - `candle run` is now an alias for `candle start`. Both launch services in the background and exit immediately. Use `candle watch` to watch logs.
 - Removed the agent-mode block on `candle run` (no longer needed, since it no longer enters watch mode)

# 0.12.0
 - Switch from better-sqlite3 to node:sqlite (Node.js built-in)
 - Change log eviction strategy to a per-service limit instead of a global limit.
 - Better support for stale process cleanup after a reboot.
 - Add `candle setup-project` and `candle check-start` commands.
 - Add `stop` as an alias for `kill`.
 - Upgrade @facetlayer/sqlite-wrapper to 1.2.2, remove @facetlayer/streams dependency
 - Block 'run' and 'watch' commands when running inside an AI agent (CLAUDECODE env var)
 - Remove port reservation system
 - Add experimental rust-based log collector

# 0.11.1
 - 'open-browser' can autodetect service name

# 0.11.0
 - Add port reservation system
 - Add 'open-browser' command

# 0.10.0
 - Add support for transient services
 - Rename config file to .candle.json
 - add-service: Autocreate the config file if missing
 - Better logs when a service fails to start
 - Add grace period, `start` waits 500ms to ensure the process doesn't fail on startup
 - Better support when commands are called with multiple service names.
 - Add list-ports command.

# 0.9.0
 - Show observed logs when wait-for-log fails.
 - 'restart' no longer enters log-watching mode.
 - Various bug fixes
 
# 0.8.0
  - Changed the storage directory to ~/.local/state/candle
  - Fixed issues with MCP output format
  - Add expect-mcp tests

# 0.7.1
  - When parsing the config file, allow 'services' to be an object instead of an array.

# 0.7.0

  - Code rewrite.
  - Added log events for initiated/started/exited.
  - Fix issues where `wait-for-log` could error if called too soon.
  - Remove code for port assignment.
  - Build tools: switch to ESbuild and PNPM.
  - Various fixes for more sensible default behavior.
  - Cleanup old logs on a regular interval.
 
# 0.6.1

  - Add `wait-for-log` command to help wait for services to start up.
  - Console output improvements.
  - Bug fixes and console print improvements.

# 0.6.0

 - Use a .candle-setup.json file instead of storing config settings in a database.

# 0.5.1

 - update 'sqlite-wrapper' and 'streams' dependencies.

# 0.5.0

 - Add commands: assign-port and clear-logs.
 - The GetLogs MCP tool now has a default limit of 200 log lines.
 - Fix bugs with `candle watch` displaying lines in the wrong order.
 - Add a max limit of 10000 log lines stored per process.

# 0.4.1

 - Fix an issue with NPM publish.

# 0.4.0

Initial public version.

Includes available commands: run, start, list, ls, list-all, stop, kill, kill-all, restart, logs, watch, config, set-command, delete-command, clear-database
