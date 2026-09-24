# Unreleased
 - **Breaking:** `candle start` no longer restarts a service that is already running; it leaves it alone and says so. Use `candle restart` to kill and relaunch.
 - **Breaking:** removed `candle check-start` (`candle start` now behaves the way it did).
 - `candle restart` with no names restarts every service in the project, starting any that are stopped.
 - `candle restart <name> --shell <cmd>` replaces the command of a transient process. `candle start` with a different `--shell` for a running service is now an error.
 - Starting or restarting several services no longer stops at the first failure; the rest are still started, then the failures are listed.
 - Installer: fail when `SHA256SUMS` has no (or more than one) entry for the archive, instead of printing "Checksum verified" without checking. Warn visibly when the checksum can't be verified at all.
 - Switches like `--json`, `--bg` and `--force` now reject an inline value (`--force=false` used to enable force).
 - `--version` / `--help` given as an option value (e.g. `wait-for-log api --message --version`) is treated as the value, not handled as a flag.
 - `--` ends option parsing; everything after it is positional.
 - `list --json`, `ps --json` and `list-all --json` rows include `projectDir`.
 - Docs: `project-setup` explains terminal vs. background `start`, start vs. restart, how services run (shell, environment, crashes, ports), soft log retention, and what to check before committing `.candle.json`.
 - `candle get-doc` no longer prints a `(File source: ...)` line after each doc.
 - Fix: `find-orphans` no longer reports running transient processes (started with `--shell`) as orphaned because they aren't in `.candle.json`.


# 0.15.0 (2026-09-23)
 - Lots of bug fixes and improvements.
 - Add `--json` option to many commands.
 - Add `--project-dir` option to many commands.
 - Add command `candle find-orphans`
 - Split up commands `candle ps` (table view) and `candle list` (multi-line listing)
 - New `candle list` format: `[name]` header with indented `status:`, `command:` and `directory:` lines
 - Rewrite the docs seen in `candle list-docs`
 - Bug fixes to not show log messages from old/stale instances
 - Remove the `--mcp` option (use the `candle mcp` command instead)

# 0.14.0 (2026-08-12)
 - Added coding agent detection, some commands have different behavior when called by an agent.
 - `candle watch` returns an error when called by an agent.
 - Commands like `candle start` are non-interactive when called by an agent. (can be overridden with `--watch`)

# 0.13.3 (2026-06-10)
 - `candle restart` now uses the latest .candle.json setting when launching the new instance.

# 0.13.2 (2026-05-28)
 - Fix bug where `candle watch` would prin no log output.

# 0.13.1 (2026-04-26)
 - Fix bug where `candle check-start` could skip a dead service.

# 0.13.0 (2026-04-21)
 - Fix bug where log-collector could leak processes.
 - Add `candle remove-service` command.
 - Bug fixes on `candle watch`

# 0.12.0 (2026-04-18)
 - Change log eviction strategy to a per-service limit instead of a global limit.
 - Better support for stale process cleanup after a reboot.
 - Add `candle setup-project` and `candle check-start` commands.
 - Add `stop` as an alias for `kill`.
 - Remove port reservation system

# 0.11.1 (2026-02-13)
 - 'open-browser' can autodetect service name

# 0.11.0 (2026-01-27)
 - Add port reservation system
 - Add 'open-browser' command

# 0.10.0 (2026-01-23)
 - Add support for transient services
 - Rename config file to .candle.json
 - add-service: Autocreate the config file if missing
 - Better logs when a service fails to start
 - Add grace period, `start` waits 500ms to ensure the process doesn't fail on startup
 - Better support when commands are called with multiple service names.
 - Add `list-ports` command.

# 0.9.0 (2025-12-06)
 - Show observed logs when wait-for-log fails.
 - 'restart' no longer enters log-watching mode.
 - Various bug fixes
 
# 0.8.0 (2025-10-17)
  - Changed the storage directory to ~/.local/state/candle
  - Fixed issues with MCP output format
  - Add expect-mcp tests

# 0.7.1 (2025-09-26)
  - When parsing the config file, allow 'services' to be an object instead of an array.

# 0.7.0 (2025-08-24)

  - Code rewrite.
  - Added log events for initiated/started/exited.
  - Fix issues where `wait-for-log` could error if called too soon.
  - Remove code for port assignment.
  - Build tools: switch to ESbuild and PNPM.
  - Various fixes for more sensible default behavior.
  - Cleanup old logs on a regular interval.
 
# 0.6.1 (2025-07-30)
  - Add `wait-for-log` command to help wait for services to start up.
  - Console output improvements.
  - Bug fixes and console print improvements.

# 0.6.0 (2025-07-22)
 - Use a .candle-setup.json file instead of storing config settings in a database.

# 0.5.1 (2025-07-15)
 - Update 'sqlite-wrapper' and 'streams' dependencies.

# 0.5.0 (2025-07-14)

 - Add commands: assign-port and clear-logs.
 - The GetLogs MCP tool now has a default limit of 200 log lines.
 - Fix bugs with `candle watch` displaying lines in the wrong order.
 - Add a max limit of 10000 log lines stored per process.

# 0.4.0 (2025-07-07)

Initial published version.
