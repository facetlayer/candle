# erase-database

Completely erase the Candle database.

## Syntax

```bash
candle erase-database
```

## Description

The `erase-database` command deletes the SQLite database and all associated files. A new database will be created automatically on the next Candle command.

## Database Location

The database is stored at:

```
~/.candle/candle.db
```

Associated files that are also deleted:
- `~/.candle/candle.db-wal` (Write-Ahead Log)
- `~/.candle/candle.db-shm` (Shared Memory)

## Example

```bash
candle erase-database
```

## Warning

This command will:
- Delete all log history
- Remove all process tracking data
- Orphan any currently running processes (they will continue running but Candle won't be able to manage them)

Make sure to stop all services before erasing the database:

```bash
candle kill-all
candle erase-database
```

## Use Cases

- Recovering from database corruption
- Starting fresh with a clean slate
- Troubleshooting issues

## See Also

- [clear-logs](clear-logs) - Clear logs without erasing the database
- [kill-all](kill-all) - Stop all running processes
