# logs

Show recent logs for service(s).

## Syntax

```bash
candle logs [name...] [--count <number>] [--previous | --all-runs] [--start-at <id>] [--json]
```

## Description

The `logs` command displays the most recent log output from one or more services. By default, it shows the last 100 lines and then exits (non-interactive).

It shows output from each service's most recent run. That works for services that aren't running anymore too, which is handy for seeing why one crashed. Output from earlier runs is left out.

If a crashed service has already been started again, its crash output belongs to the previous run. Use `--previous` to see it. [start](start) and [restart](restart) point at it when they relaunch a service whose last run exited with an error:

```
$ candle start api
[Started process 'api'] $ npm run dev
[With root directory: /home/me/app]
[The previous run exited with code 1; see 'candle logs api --previous']
```

`--all-runs` shows every stored run of each service, oldest first, with a `-- new run --` line where each run begins:

```
$ candle logs api --all-runs
listening on port 3000
[stderr] Error: connection refused
[Process exited with code 1]
-- new run --
listening on port 3000
```

How many earlier runs are available depends on log retention; see [Configuration](../configuration).

`--count` counts the lines that are printed. When there are more lines from the latest run than `--count` allows, the output starts with a hint:

```
$ candle logs api --count 3
-- showing the last 3 lines; use --count to see more --
listening on port 3000
GET /health 200
GET /api/users 200
```

When more than one service is shown, `--count` applies to each service separately, so one chatty service can't push the others out of the output. The hint names the services that had more lines:

```
$ candle logs --count 3
-- showing the last 3 lines per service (api had more); use --count to see more --
[web] compiled successfully
[api] listening on port 3000
[api] GET /health 200
[api] GET /api/users 200
```

Naming a service that isn't configured, and that has no stored logs, is an error: `logs` prints `No service '<name>' configured for directory: <dir>` and exits with status 1.

## Arguments

- `name` - Name of the service(s) to view logs for. Can specify multiple services. If omitted, shows logs for every service in the project. When more than one service is shown, each line is prefixed with `[service-name]`.

## Options

- `--count <number>` - Number of log lines to show, per service. Default: 100.
- `--previous` - Show the run before the latest one instead of the latest.
- `--all-runs` - Show every stored run, oldest first. Can't be combined with `--previous`.
- `--start-at <id>` - Only show logs with an ID greater than `<id>`. Log IDs appear in the `--json` output, so pass the `id` of the last entry you've seen to fetch only newer lines.
- `--json` - Print the logs as a JSON array. Each entry has `id`, `service`, `type` (`stdout`, `stderr`, `exited` or `start_failed`), `content`, `timestamp` (Unix seconds) and `run` (an ID shared by every line of one run of the service). No truncation hint is printed; an empty result is `[]`.
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

### See why a service crashed, after starting it again

```bash
candle logs api --previous
```

### Show only the last 10 log lines

```bash
candle logs api --count 10
```

### Get logs as JSON

```bash
$ candle logs api --count 2 --json
[
  {
    "id": 511,
    "service": "api",
    "type": "stdout",
    "content": "GET /health 200",
    "timestamp": 1790000000,
    "run": 498
  },
  {
    "id": 512,
    "service": "api",
    "type": "stdout",
    "content": "GET /api/users 200",
    "timestamp": 1790000001,
    "run": 498
  }
]
```

### Show only lines newer than ones you've already seen

Take the `id` of the last entry from `--json` output and pass it to `--start-at`:

```bash
candle logs api --json --start-at 512
```

## See Also

- [watch](watch) - Watch live output (interactive)
- [run](run) - Start and watch a service
- [clear-logs](clear-logs) - Clear log history
