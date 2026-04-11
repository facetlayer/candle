# run

Alias for [start](start). Launches service(s) in the background and exits.

## Syntax

```bash
candle run [name...] [options]
```

## Description

`candle run` is an alias for `candle start`. Both commands launch one or more services in the background and exit as soon as the services are running. Neither command enters watch mode.

To watch the output of services you've launched, use [watch](watch) or [logs](logs):

```bash
candle run api && candle watch api
```

See the [start](start) command documentation for the full list of arguments, options, and examples.

## See Also

- [start](start) - Start services in the background (the command this aliases)
- [watch](watch) - Launch (if needed) and watch output from services
- [logs](logs) - View recent logs from running services
