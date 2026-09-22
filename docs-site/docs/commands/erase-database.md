# erase-database

Completely erase the Candle database.

## Syntax

```bash
candle erase-database [--force]
```

## Description

The `erase-database` command deletes the SQLite database and all associated files. A new database will be created automatically on the next Candle command.

If any Candle-managed services are still running, the command refuses and lists them, then exits with status 1 without deleting anything. Erasing would leave those processes running with nothing in Candle able to see or stop them. Stop them first with `candle kill-all`.

```
$ candle erase-database
Refusing to erase the database: 1 Candle-managed process is still running.
Erasing now would leave them running with no way for Candle to stop them.
  api (pid 12345) in /Users/you/projects/my-app
Run 'candle kill-all' first, or pass --force to erase anyway.
```

If the database is corrupted and can't be read, the running-services check is skipped with a warning and the erase goes ahead.

A `start` or `restart` that is already under way finishes before the erase checks for running services, and a new one waits until the erase is done. So a service can't be launched between the check and the deletion.

## Options

- `--force` - Erase even while services are running. They keep running, untracked.

## Database Location

See [Database](../database) for details on the database location.

Associated files that are also deleted:
- `candle.db-wal` (Write-Ahead Log)
- `candle.db-shm` (Shared Memory)

## Example

```bash
candle erase-database
```

## Warning

This command will:
- Delete all log history
- Remove all service tracking data
- With `--force`, orphan any currently running services (they will continue running but Candle won't be able to manage them)

Stop all services before erasing the database:

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
- [kill-all](kill-all) - Stop all running services
