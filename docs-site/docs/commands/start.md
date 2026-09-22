# start

Start service(s).

`candle run` is an alias for this command — both do exactly the same thing.

## Syntax

```bash
candle start [name...] [options]
candle run [name...] [options]
```

## Description

The `start` command launches one or more services in the background.

The flow of running `start`:

1. If the service is already running, restart it.
2. Launch and wait for the service to successfully start.
3. Wait for a 'grace period' (500ms) to make sure the service stays running.

Starts of the same service are serialized. If two `start` commands for one service run at
the same time, for example from parallel agents or scripts, the second waits for the first
to finish and then restarts it, so you always end up with one instance. Two racing
`check-start` commands launch the service once.

What happens next depends on how `start` was invoked:

- **Interactive mode** (a human at a terminal): `start` stays attached and streams
  the new process's logs, starting from the fresh launch (no stale logs).
  Press `Ctrl+C` to stop watching — the process keeps running in the background
  until you stop it with a command like `candle stop`.
- **Non-interactive mode** (AI agents, scripts, pipes, CI): `start` exits as soon
  as the launch is confirmed and prints a hint pointing at `candle logs`.

Candle picks the mode automatically: it uses non-interactive mode when the output
is not a terminal or when run by a coding agent (such as Claude Code). Use
`--watch` or `--bg` to force a mode explicitly.

## Arguments

- `name` - Name of the service(s) to start. If omitted, starts all services defined in the configuration file.

## Options

- `--watch` - Force interactive mode: watch logs after starting
- `--bg` - Force non-interactive mode: exit as soon as the launch is confirmed
- `--shell <command>` - Start a transient service with the specified shell command
- `--root <directory>` - Set the working directory for a transient service. Only valid with `--shell`; for a configured service, set `root` in `.candle.json` instead (passing `--root` is an error)
- `--project-dir <dir>` - Act on the given project instead of the current directory. See [Targeting another project](../project-dir).

## Examples

### Start all configured services

```bash
candle start
```

### Start a configured service

```bash
candle start api
```

Each launched service prints a two-line banner naming the shell command it ran
and the directory it ran in:

```
$ candle start api
[Started process 'api'] $ npm run api
[With root directory: /Users/andy/proj]
```

### Start multiple services

```bash
candle start api web worker
```

### Start a service in the background without watching logs

```bash
candle start api --bg
```

## Behavior

1. The service is started in the background
2. Output is logged to the database (viewable with `candle logs`)
3. In interactive mode, `start` watches the new logs until `Ctrl+C`; in
   non-interactive mode it exits immediately
4. Use `candle watch` or `candle logs` at any time to view output

## Transient Services

A "transient" service is when you launch a service without defining it in the `.candle.json` config file.

This can be done with the `--shell` option (and optionally `--root` to change the directory). `--shell` requires exactly one service name.

### Start a transient service

```bash
candle start server --shell "python -m http.server 8080"
```

### Start a transient service in a subdirectory

```bash
candle start server --shell "npm run dev" --root ./packages/api
```

## When a start fails

If the service can't be launched, `start` exits with code 1 and says why. A
`root` directory that doesn't exist is reported by path, before anything is
launched (and before any running instance is stopped):

```
Process 'api' failed to start: root directory does not exist: /Users/andy/proj/packages/api
```

If the process starts but exits during startup, `start` prints the logs it
captured, such as the shell's `command not found` for a missing executable:

```
Process 'api' failed to start. Recent logs:
sh: nosuch-binary: command not found
Process failed to start: exited with code 127
```

## See Also

- [run](run) - Alias for `start`
- [logs](logs) - View logs from started services
- [watch](watch) - Watch live output from running services
