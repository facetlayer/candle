# logs

Show recent logs for process(es).

## Syntax

```bash
candle logs [name...]
```

## Description

The `logs` command displays the most recent log output from one or more services. By default, it shows the last 100 lines and then exits (non-interactive).

Logs are persisted in the database, so you can view them even after a service has stopped.

## Arguments

- `name` - Name of the service(s) to view logs for. Can specify multiple services.

## Examples

### View logs for a specific service

```bash
candle logs api
```

### View logs for multiple services

```bash
candle logs api web
```

## Behavior

1. Retrieves the most recent 100 log lines from the database
2. Displays them and exits immediately
3. Works even if the service is not currently running
4. Shows logs from the most recent process run

## Notes

- Logs are stored in the SQLite database at `~/.candle/candle.db`
- Use `candle clear-logs` to clear log history
- For live output, use `candle watch` instead

## See Also

- [watch](watch) - Watch live output (interactive)
- [run](run) - Start and watch a service
- [clear-logs](clear-logs) - Clear log history
