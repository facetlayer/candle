# logs

Show recent logs for service(s).

## Syntax

```bash
candle logs [name...] [--count <number>] [--start-at <id>]
```

## Description

The `logs` command displays the most recent log output from one or more services. By default, it shows the last 100 lines and then exits (non-interactive).

It shows output from each service's most recent run. That works for services that aren't running anymore too, which is handy for seeing why one crashed. Output from earlier runs is left out.

`--count` counts the lines that are printed. When there are more lines from the latest run than `--count` allows, the output starts with a hint:

```
$ candle logs api --count 3
-- showing the last 3 lines; use --count to see more --
listening on port 3000
GET /health 200
GET /api/users 200
```

## Arguments

- `name` - Name of the service(s) to view logs for. Can specify multiple services.

## Options

- `--count <number>` - Number of log lines to show. Default: 100.
- `--start-at <id>` - Only show logs after this log ID. Useful for pagination.
- `--project-dir <dir>` - Act on the given project instead of the current directory. See [Targeting another project](../project-dir).

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
