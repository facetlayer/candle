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

## Options

- `--project-dir <dir>` - Act on the given project instead of the current directory. See [Targeting another project](../project-dir).

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

1. Deletes log entries for the named services, or for every service in the project when no names are given
2. Removes orphaned log entries (logs for any service, in any project, that Candle no longer tracks)
3. Optimizes the database to reclaim space

## Notes

- This only clears logs, not running services
- The action cannot be undone
- Use `candle erase-database` to completely reset all data

## See Also

- [logs](logs) - View logs
- [erase-database](erase-database) - Completely reset database
