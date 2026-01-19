
# Database

The way Candle works is: Every time a service is launched, it's launched by a wrapper process called the 'log watcher'. The log watcher process captures the subprocess's stdout, stderr, and exit code, and saves all events to an SQLite database. Then, other Candle commands (such as `candle logs`, `candle ls`, `candle watch`) work by reading events from the same database.

### Location

By default the database is stored at `~/.local/state/candle/candle.db`. This location can change based on your `XDG_STATE_HOME` environment variable.


