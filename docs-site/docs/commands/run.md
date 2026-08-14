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

Each launched service prints a two-line banner naming the shell command it ran
and the directory it ran in:

```
$ candle run api
[Started process 'api'] $ npm run api
[With root directory: /Users/andy/proj]
```

See the [start](start) command documentation for the full list of arguments, options, and examples.

## See Also

- [start](start) - Start services (the command this aliases)
- [watch](watch) - Watch live output from running services
- [logs](logs) - View recent logs from running services
