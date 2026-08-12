# run

Alias for [start](start). Launches service(s).

## Syntax

```bash
candle run [name...] [options]
```

## Description

`candle run` is an alias for `candle start`. Both commands launch one or more
services in the background. When run interactively, they then watch the new
process's logs until `Ctrl+C`; when run non-interactively (agents, scripts,
pipes), they exit as soon as the services are running.

See the [start](start) command documentation for the full list of arguments, options, and examples.

## See Also

- [start](start) - Start services (the command this aliases)
- [watch](watch) - Watch live output from running services
- [logs](logs) - View recent logs from running services
