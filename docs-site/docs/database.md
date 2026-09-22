
# Database

This section talks about the Candle database, which helps understand how the tool works.

Whenever candle launches a service, there is a wrapper process called the monitor (the `candle` binary re-invoking itself as `candle --monitor`) which launches that service as a subprocess. The monitor captures stdout, stderr, and exit code, and saves all events to an SQLite database. Then, other Candle commands (such as `candle logs`, `candle ls`, `candle watch`) work by reading events from the same database.

Usually you don't need to interact with this database directly, but it's there for advanced usage.

## Location

By default the database is stored at `~/.local/state/candle/candle.db`. The directory is chosen in this order:

1. `$CANDLE_DATABASE_DIR`, if set (used as-is)
2. `$XDG_STATE_HOME/candle`, if `XDG_STATE_HOME` is set
3. `~/.local/state/candle`

## Commands

The `candle erase-database` command is available if you want to erase your local database and start fresh. This is not commonly needed. It refuses to run while Candle-managed processes are still running, since erasing would leave them running untracked. Run `candle kill-all` first, or pass `--force` to erase anyway.
