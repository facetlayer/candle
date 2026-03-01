# check-start

Start service(s) only if not already running.

## Syntax

```bash
candle check-start [name...] [options]
```

## Description

The `check-start` command starts one or more services only if they are not already running. If a service is already running, it is left alone — no restart occurs. This makes it safe to use in scripts and CI pipelines where you want to ensure a service is running without disrupting it.

Compared to `start`, which always kills and restarts a running service, `check-start` is a no-op for services that are already up.

## Arguments

- `name` - Name of the service(s) to start. If omitted, checks and starts all services defined in the configuration file.

## Options

- `--shell <command>` - Start a transient service with the specified shell command
- `--root <directory>` - Set the working directory for a transient service

## Examples

### Ensure a service is running

```bash
candle check-start api
```

### Ensure multiple services are running

```bash
candle check-start api web worker
```

### Use in a script

```bash
# Start dependencies without restarting them if they're already up
candle check-start database
candle check-start api
candle start my-dev-server
```

## See Also

- [start](start) - Start services (always restarts if already running)
- [restart](restart) - Restart running services
- [list](list) - List services and their status
