# ps / status

Compact status table for the services in the current project directory.

## Syntax

```bash
candle ps [names...] [--json]
candle status [names...] [--json]
```

## Description

The `ps` command shows the same set of services as [list](list) — running
services plus services configured in `.candle.json` that aren't running — but as
a one-line-per-service table.

The table has four columns: `NAME`, `STATUS`, `PID`, and `UPTIME`. It
deliberately omits the service's command and directory, which are the two widest
values, so the table stays narrow enough to read in a small terminal. Use
[list](list) when you want to see those values in full.

## Arguments

- `names` - Optional service name(s) to show. If omitted, every service in the
  project is listed. If a given name doesn't match any service, `ps` prints an
  error naming the unknown service and exits with a non-zero status.

## Options

- `--json` - Print the listing as a JSON array instead of the table. This is the
  same JSON that `candle list --json` emits, including the command and directory
  fields that the table leaves out.

## Output

```
$ candle ps
NAME  STATUS       PID    UPTIME
----  -----------  -----  ------
web   RUNNING      12345  3m 5s
api   not running  -      -
```

A service that isn't running shows `-` for its pid and uptime. If a running
process drifted from its current `.candle.json` definition, its status reads
`RUNNING [config changed]`.

When there are no services at all, `ps` prints `No services configured.`

## Examples

### Status of every service in the project

```bash
candle ps
```

### Status of one service

```bash
candle status web
```

### Machine-readable output

```bash
candle ps --json
```

## See Also

- [list](list) - Full detail view, with each service's command and directory
- [list-all](list-all) - List all services globally
