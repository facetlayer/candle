# logs

Show recent logs for service(s).

## Syntax

```bash
candle logs [name...] [--count <number>] [--start-at <id>]
```

## Description

The `logs` command displays the most recent log output from one or more services. By default, it shows the last 100 lines and then exits (non-interactive).

This can include logs from previous launches, or logs for services that aren't running.

## Arguments

- `name` - Name of the service(s) to view logs for. Can specify multiple services.

## Options

- `--count <number>` - Number of log lines to show. Default: 100.
- `--start-at <id>` - Only show logs after this log ID. Useful for pagination.

## Examples

### View logs for a specific service

```bash
candle logs api
```

### View logs for multiple services

```bash
candle logs api web
```

### Show only the last 10 log lines

```bash
candle logs api --count 10
```

### Show logs after a specific log ID

```bash
candle logs api --start-at 500
```

## See Also

- [watch](watch) - Watch live output (interactive)
- [run](run) - Start and watch a service
- [clear-logs](clear-logs) - Clear log history
