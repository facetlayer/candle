# list / ls

Show details for the services in the current project directory.

## Syntax

```bash
candle list [names...] [--json]
candle ls [names...] [--json]
```

## Description

The `list` command displays the services for the current project directory.

This includes:
 - Services that are actively running.
 - And services that are configured in .candle.json that aren't running.

Output is a multiline detail view: one entry per service, with a header line
followed by indented `command:` and `directory:` lines. Those values are printed
in full and are never truncated, so you can see the exact shell command a service
runs and the exact directory it runs in.

For a compact one-line-per-service table, use [ps](ps) instead.

## Arguments

- `names` - Optional service name(s) to show. If omitted, every service in the
  project is listed. If a given name doesn't match any service, `list` prints an
  error naming the unknown service and exits with a non-zero status.

## Options

- `--json` - Print the listing as a JSON array instead of the detail view.

## Output

Each entry's header line is the service name, its status, and (when running) its
pid and uptime:

```
$ candle list
web  RUNNING  pid 12345  uptime 3m 5s
  command:   npm run dev
  directory: /Users/andy/proj/web

api  not running
  command:   npm run api
  directory: /Users/andy/proj
```

`pid` and `uptime` are omitted for services that aren't running.

If a running process was started from a service definition that has since been
edited in `.candle.json`, ` [config changed]` is appended to its status:

```
web  RUNNING [config changed]  pid 12345  uptime 3m 5s
  command:   npm run dev
  directory: /Users/andy/proj/web
```

When there are no services at all, `list` prints `No services configured.`

## Examples

### List every service in the current project

```bash
candle list
```

### Show just one service

```bash
candle list web
```

### Show a few services

```bash
candle ls web api
```

### Machine-readable output

```bash
candle list --json
```

The JSON is an array of objects with `serviceName`, `command` (the service's
shell command), `workingDir`, `uptime`, `pid`, `status`, and — for running
processes — `configChanged`. Passing service names filters the JSON the same way
it filters the detail view.

## See Also

- [ps](ps) - Compact status table for this project (aliases: `status`)
- [list-all](list-all) - List all services globally
