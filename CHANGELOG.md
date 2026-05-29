
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
