# clear-logs

Clear logs for services in the current project directory.

## Syntax

```bash
candle clear-logs [name...]
```

## Description

The `clear-logs` command deletes log entries from the database. It also optimizes the database and removes any orphaned log entries.

## Arguments

- `name` - Name of the service(s) to clear logs for. If omitted, clears logs for all services in the current project.

## Examples

### Clear logs for a specific service

```bash
candle clear-logs api
```

### Clear logs for multiple services

```bash
candle clear-logs api web
```

### Clear all logs in current project

```bash
candle clear-logs
```

## Behavior

1. Deletes log entries from the SQLite database
2. Optimizes the database to reclaim space
3. Removes orphaned log entries

## Notes

- This only clears logs, not running services
- The action cannot be undone
- Use `candle erase-database` to completely reset all data

## See Also

- [logs](logs) - View logs
- [erase-database](erase-database) - Completely reset database
