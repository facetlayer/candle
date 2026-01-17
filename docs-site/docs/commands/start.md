# start

Start process(es) in the background and exit immediately.

## Syntax

```bash
candle start [name...] [options]
```

## Description

The `start` command launches one or more services in the background without displaying their output. The command exits immediately after starting the services.

Use this command when you want to start services without monitoring their output.

## Arguments

- `name` - Name of the service(s) to start. If omitted, starts all services defined in the configuration file.

## Options

- `--shell <command>` - Start a transient process with the specified shell command
- `--root <directory>` - Set the working directory for a transient process
- `--enable-stdin` - Enable stdin message polling from the database

## Examples

### Start a configured service

```bash
candle start api
```

### Start multiple services

```bash
candle start api web worker
```

### Start all configured services

```bash
candle start
```

### Start a transient process

```bash
candle start --shell "node server.js"
```

### Start with a custom working directory

```bash
candle start --shell "npm run dev" --root ./backend
```

## Behavior

1. The service is started in the background
2. Output is logged to the database (viewable with `candle logs`)
3. The command exits immediately
4. Use `candle watch` or `candle logs` to view output

## Exit Codes

- `0` - Service started successfully
- `1` - Service not found or error occurred

## See Also

- [run](run) - Start and watch services interactively
- [logs](logs) - View logs from started services
- [watch](watch) - Watch live output from running services
