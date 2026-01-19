
# Database

This section talks about the Candle database, which helps understand how the tool works.

Whenever candle launches a service, there is a wrapper process called the 'log watcher' which launches that service as a subprocess. The log watcher process captures stdout, stderr, and exit code, and saves all events to an SQLite database. Then, other Candle commands (such as `candle logs`, `candle ls`, `candle watch`) work by reading events from the same database.

Usually you don't need to interact with this database directly, but it's there for advanced usage.

## Location

By default the database is stored at `~/.local/state/candle/candle.db`. This location can change based on your `XDG_STATE_HOME` environment variable.

## Commands

The `candle erase-database` command is available if you want to erase your local database and start fresh. This is not commonly needed. If you use this, make sure to run `candle kill-all` first, to avoid leaving orphaned processes behind.
