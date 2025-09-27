
# 0.8.0
  - Changed the storage directory to ~/.local/state/candle

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
