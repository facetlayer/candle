# run

Launch service(s) and watch their output in interactive mode.

## Syntax

```bash
candle run [name...] [options]
```

## Description

The `run` command starts one or more services and then enters watch mode, displaying their output in real-time. Press `Ctrl+C` to exit watch mode - the service will continue running in the background.

If a service is already running, it will be restarted first.

## Difference in run vs start

The `run` and `start` commands are similar, the main differences are:
 - `run` - Watches the logs after launch.
 - `start` - Exits as soon as the service is running, does not watch logs.

## Arguments

- `name` - Name of the service(s) to run (required). You can specify multiple services.

## Options

- `--shell <command>` - Override the shell command. Can be used for transient services.
- `--root <directory>` - Override the working directory. Can be used for transient services.

## Examples

### Run a configured service

```bash
candle run api
```

### Run multiple services

```bash
candle run api web worker
```

## Exit Codes

- `0` - Command executed successfully
- `1` - Service not found or error occurred


## Transient Services

A "transient" service is when you launch a service without defining it in the `.candle.json` config file.

This can be done with the `--shell` option (and optionally `--root` to change the directory). You'll still need to provide a unique name for the service.

### Run a transient service

This example starts a Python server with a name of `server`:

```bash
candle run server --shell "python -m http.server 8080"
candle kill server
```

### Run a transient service in a subdirectory

```bash
candle run server --shell "npm run dev" --root ./packages/api
```

## See Also

- [start](start) - Start services without watching
- [watch](watch) - Watch output of already running services
- [logs](logs) - View recent logs without watching
