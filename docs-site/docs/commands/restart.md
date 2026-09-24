# restart

Restart service(s).

## Syntax

```bash
candle restart [name...] [options]
```

## Description

The `restart` command stops service(s) and starts them again. A service that
isn't running is simply started, so `restart` always leaves you with a fresh
instance.

For services defined in `.candle.json`, `restart` reloads the service
definition from the config file, so edits to a service's `shell` or `root`
take effect on the next restart. Transient processes (started with `--shell`
and not present in the config) are relaunched with the same command they were
originally started with, unless you pass a new one with `--shell`.

If a service fails to start again, `restart` prints `Failed to restart: <reason>`
and exits with code 1. When restarting several services, the others are still
restarted.

`restart` follows the same interactive behavior as [start](start): when run
interactively it watches the restarted process's logs until `Ctrl+C` (the
process keeps running); when run non-interactively (agents, scripts, pipes) it
exits as soon as the restart is confirmed.

## Arguments

- `name` - Name of the service(s) to restart. If omitted, restarts every service in the project: all services in `.candle.json`, plus any running transient processes. A service that isn't running is started.

## Options

- `--watch` - Force interactive mode: watch logs after restarting
- `--bg` - Force non-interactive mode: exit as soon as the restart is confirmed
- `--shell <command>` - Relaunch a transient service with this command instead of the one it was started with. Requires exactly one service name.
- `--root <directory>` - Working directory for the new command. Only valid with `--shell`.
- `--project-dir <dir>` - Act on the given project instead of the current directory. See [Targeting another project](../project-dir).

## Examples

### Restart a specific service

```bash
candle restart api
```

### Restart every service in the project

```bash
candle restart
```

### Change a transient service's command

```bash
candle restart server --shell "python -m http.server 9090"
```

## See Also

- [start](start) - Start a service
- [kill](kill) - Stop a service
- [run](run) - Start and watch a service
