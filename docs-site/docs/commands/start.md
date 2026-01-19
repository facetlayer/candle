# start

Start service(s) in the background.

## Syntax

```bash
candle start [name...] [options]
```

## Description

The `start` command launches one or more services in the background without displaying their output.

The flow of running `start`:

1. If the service is already running, restart it.
2. Launch and wait for the service to successfully start.
3. Wait for a 'grace period' (default of 500ms) to make sure the service stays running.
4. Then exit.

After running `start` you can check on the service using the `watch` or `logs` commands.

## Arguments

- `name` - Name of the service(s) to start (required). You can specify multiple services.

## Options

- `--shell <command>` - Start a transient service with the specified shell command
- `--root <directory>` - Set the working directory for a transient service

## Examples

### Start a configured service

```bash
candle start api
```

### Start multiple services

```bash
candle start api web worker
```

## Behavior

1. The service is started in the background
2. Output is logged to the database (viewable with `candle logs`)
3. The command exits immediately
4. Use `candle watch` or `candle logs` to view output

## Transient Services

A "transient" service is when you launch a service without defining it in the `.candle.json` config file.

This can be done with the `--shell` option (and optionally `--root` to change the directory).

### Start a transient service

```bash
candle start server --shell "python -m http.server 8080"
```

### Start a transient service in a subdirectory

```bash
candle start server --shell "npm run dev" --root ./packages/api
```

## See Also

- [run](run) - Start and watch services interactively
- [logs](logs) - View logs from started services
- [watch](watch) - Watch live output from running services
