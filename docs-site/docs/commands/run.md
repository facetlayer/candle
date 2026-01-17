# run

Launch process(es) and watch their output in interactive mode.

## Syntax

```bash
candle run [name...] [options]
```

## Description

The `run` command starts one or more services and enters watch mode, displaying their output in real-time. Press `Ctrl+C` to exit watch mode - the process will continue running in the background.

If the service is already running, it will be restarted first.

## Arguments

- `name` - Name of the service(s) to run. If omitted, runs all services defined in the configuration file.

## Options

- `--shell <command>` - Run a transient process with the specified shell command (no config required)
- `--root <directory>` - Set the working directory for a transient process
- `--enable-stdin` - Enable stdin message polling from the database

## Examples

### Run a configured service

```bash
candle run api
```

### Run multiple services

```bash
candle run api web worker
```

### Run all configured services

```bash
candle run
```

### Run a transient process

```bash
candle run --shell "python -m http.server 8080"
```

### Run a transient process in a subdirectory

```bash
candle run --shell "npm run dev" --root ./packages/api
```

## Behavior

1. If the service is already running, Candle will restart it
2. The service starts and output is displayed in real-time
3. Press `Ctrl+C` to exit watch mode
4. The process continues running in the background after exiting

## Exit Codes

- `0` - Command executed successfully
- `1` - Service not found or error occurred

## See Also

- [start](start) - Start services without watching
- [watch](watch) - Watch output of already running services
- [logs](logs) - View recent logs without watching
