# logs

Show recent logs for service(s).

## Syntax

```bash
candle logs [name...]
```

## Description

The `logs` command displays the most recent log output from one or more services. By default, it shows the last 100 lines and then exits (non-interactive).

This can include logs from previous launches, or logs for services that aren't running.

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

## See Also

- [watch](watch) - Watch live output (interactive)
- [run](run) - Start and watch a service
- [clear-logs](clear-logs) - Clear log history
