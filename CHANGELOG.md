
# Unreleased
 - Fatal errors now all print to stderr as a single `Error: <message>`. This replaces `Missing required argument: ...` (now `Error: --shell <command> is required` / `Error: --message <text> is required`), `Error adding service: ...`, `Error removing service: ...`, `candle: database error: ...` and unprefixed messages such as `No service '<name>' configured ...`, `Unknown argument: ...` and `No .candle.json ...`. `wait-for-log` failures drop their `wait-for-log failed:` prefix and move from stdout to stderr, e.g. `Error: Timed out after 30000ms and message "..." not found.` (the recent-log tail stays on stdout). A process that can't be killed is reported on stderr as `Error: Could not kill process ...`.
 - `clear-logs` and `erase-database` print plain status lines, with no check marks and no closing "...successfully!" line: `Cleared N log entries` / `No logs found to clear`, and `Removed database file` ... `Database erased. A new one will be created on next use.`
 - `candle logs --json` prints a JSON array of `{id, service, type, content, timestamp}` log rows (`type` is `stdout`, `stderr`, `exited` or `start_failed`). Use an `id` from it with `--start-at`. Unknown flags now read `Unknown argument: --flag`.
 - `candle logs <name>` with a name that isn't configured (and has no stored logs or process) now errors with `No service '<name>' configured for directory: ...`, exit 1, like `list` and `ps`.
 - `candle logs --count N` for several services now applies the limit to each service, and the truncation hint says which services had more lines. The MCP `GetLogs` hint now suggests passing a larger `limit`.
 - `candle wait-for-log` fails right away with `Service '<name>' is not running and message "..." was not found.` when the service isn't running, its latest run has exited, or its process disappears during the wait, instead of waiting for the full timeout. A finished run that did print the message still succeeds. On failure it shows the last 20 lines of the latest run. An unknown name is an error.
 - `candle help <command>` prints the same help as `candle <command> --help`.
 - `--enable-stdin` is no longer listed in the help or docs for `start`, `check-start` and `add-service`. It is still accepted.
 - `candle get-doc` now matches doc names exactly (ignoring case and an optional `.md`), instead of by prefix, so `get-doc start` no longer prints the getting-started doc. Frontmatter is stripped, and `list-docs` hints use the listed names.
 - `candle list-ports <name>` now shows only the named services; it used to ignore the names and list every port in the project. A name that isn't a configured (or transient) service in the project is an error, `No service '<name>' configured ...`, exit 1. `candle open-browser <name>` gives the same error for an unknown name, instead of "No open ports found".
 - `candle list-ports` and `candle list-ports-all` accept `--json`, printing `{ "ports": [...] }` (the MCP `ListPorts` shape).
 - Fix `candle list-ports-all` failing outside a project with "No .candle.json file found". It's system-wide and no longer looks for a config.
 - `candle ps` / `candle list` show `EXITED (<code>)` for a service whose latest run exited non-zero, instead of `not running`, so a crash is distinguishable from a service that was stopped or never started.
 - `list --json` / `ps --json` / `list-all --json` rows now always have the same keys: a stopped service has `"pid": null` (was `0`) and `"configChanged": false` (was omitted), and every row has a new `exitCode` (the non-zero exit code of the latest run, else `null`). The MCP `ListServices` output changes the same way. An unknown name for `list` / `ps` now reads `No service '<name>' configured`.
 - `candle list-all` now reports the directory a service runs in (its `root` resolved against the project), matching `candle list`; it used to show the bare project directory. `candle list` now shows a transient service started with `--root sub` in `<project>/sub`, as its start banner does.
 - `candle start <configured-service> --root <dir>` is now an error. `--root` only applies to transient services started with `--shell`; it used to be silently ignored.
 - A failed start now says what was missing. A `root` directory that doesn't exist fails up front with `root directory does not exist: <path>` (and no longer stops a running instance first), instead of a bare `No such file or directory (os error 2)` that looked the same as a missing executable. The `Recent logs:` label is followed by a newline instead of a trailing space and blank line.
 - `candle restart` now exits 1 when starting a service again fails. It printed `Failed to restart: ...` but exited 0, so scripts and CI couldn't tell.
 - `candle restart` no longer prints `[Cleaning up stale process entry ...]` between its kill and start lines. The message is now reserved for a row that claimed to be running but whose process was gone; sweeping a row that was already marked killed is silent.
 - `candle add-service` now rejects a `--root` directory that doesn't exist, naming the resolved path, and rejects service names that use anything other than letters, digits, `-`, `_` and `.`. Nothing is written in either case.
 - Candle now warns on stderr about unknown keys in `.candle.json`, at the top level or in a service, and suggests the likely key (for example `"cwd"` → `"root"`). The command still runs.
 - Fix `add-service`, `remove-service` and `set-config` deleting per-service keys Candle doesn't recognize. They are now kept, in their original position. Files these commands write, and `setup-project`, now end with a newline.
 - Fix `candle clear-logs` with no names clearing nothing. It now clears logs for every service in the project, as documented.
 - `--project-dir` pointing at a directory with no config now says `No .candle.json in <dir> (--project-dir doesn't search parent directories)`. Without `--project-dir`, the "No .candle.json file found" error now suggests `candle add-service <name> --shell <cmd>` or `candle setup-project`.
 - Opening an old `candle.db` that lacks newer columns now upgrades it in place, keeping its rows. Previously the missing columns were never added.
 - Fix a race between `candle erase-database` and a concurrent `start`: erase now holds a lock that every start takes, so no service can launch between its running-process check and the deletion.
 - Fix the monitor sometimes dropping a process's last output lines, such as a fast failure's error message. After the process exits, the monitor keeps collecting output until both pipes close, for up to 2s, since a background child can hold them open indefinitely.
 - Fix `candle kill` leaving a service running when it traps or ignores `SIGTERM`, while reporting it killed. Kill still sends `SIGTERM` to the process tree first, but now waits up to 5s for every process in the tree to exit and then sends `SIGKILL` to whatever is left, including children that were reparented to init when the root shell exited. The escalation is reported on stderr. `start` and `restart` share this path when they replace a running instance.
 - Fix concurrent `candle start` of the same service launching duplicate instances. Five parallel starts left five running copies, only one of which `ps` showed. Starts of a service are now serialized with an advisory lock under `<state dir>/locks/`, held from the check-start test through the start/fail decision, so racing starts end with one instance and racing `check-start` calls launch once.
 - `candle erase-database` now refuses while Candle-managed processes are still running, listing them and exiting 1. Erasing would leave them running with nothing in Candle able to stop them. Pass `--force` to erase anyway. If the database can't be read, which is a main reason to erase it, the check is skipped with a warning.
 - Fix `candle logs --count N` printing fewer than N lines. The limit counted rows that never print: the monitor's `process_started` marker, written after the first output lines, and rows from a previous run that the display filter then dropped. The limit now applies only to printable rows from each service's latest run.
 - The `logs` truncation hint now reads `-- showing the last N lines; use --count to see more --` instead of `-- older logs have been removed --`, and appears only when `--count` actually cut off lines from the latest run.
 - Add a `--project-dir <dir>` option to the commands that act on a single project (`start`, `run`, `check-start`, `restart`, `kill`, `list`, `ps`, `logs`, `watch`, `wait-for-log`, `clear-logs`, `list-ports`, `open-browser`). The named directory *is* the project, overriding the usual search up from the current directory; the path may be relative. Unlike that search it never falls back to a parent, so naming a subdirectory of a project is an error rather than a silent match on the parent. The system-wide commands (`list-all`, `list-ports-all`, `kill-all`, `find-orphans`) don't take it.
 - `kill --project-dir` (along with `logs`, `clear-logs`, and `wait-for-log`) accepts a project directory that no longer exists, or that no longer has a config file — these work purely from Candle's own records, so services left behind by a deleted project can still be cleaned up. Because there is no config to check against, an unrecognized service name there reports that nothing is running instead of failing.
 - Add `candle find-orphans`, a system-wide maintenance command in the same family as `kill-all`. It lists running services whose project no longer accounts for them: the project directory was deleted, its config file was removed, or the service was dropped from the config. Supports `--json`. A config file that exists but won't parse is not treated as orphaning, so a JSON typo never flags a healthy service.
 - Fix `candle kill <name>` printing nothing at all when the named service wasn't running. Kill queries every matching row including already-killed ones, and counted each row it swept as a kill — so whether you got the "No running processes found" message depended on whether the reaper had cleared the previous kill's row yet. Only rows with a live process to signal count now.
 - Fix `candle start` reporting success for a service that had already failed. The monitor's 500ms startup grace period abandoned its event queue the moment the deadline passed; a process that dies instantly still writes its error output first, and on a loaded machine logging those lines could outlast the window, leaving the exit event unread. The queue is now drained before the start/fail decision.
 - Fix `candle erase-database` failing with "No such file or directory" when a database file vanished between the existence check and the removal — SQLite checkpointing away a WAL file as a monitor shut down was enough to turn a successful erase into a non-zero exit.
 - Fix watch mode showing output from the instance it just replaced. A restart only signals the old process and returns immediately, so a service that shuts down slowly wrote its last lines — including `[Process was stopped]` — after the new launch had already been recorded, and `candle start` / `run` / `restart` replayed them as if they belonged to the new instance. Starting a service now waits (up to 2s) for the previous instance and its monitor to actually exit before recording the new launch, and the log filter no longer attributes an exit event to a launch that hasn't reported starting yet.
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
