# restart

Restart running service(s).

## Syntax

```bash
candle restart [name...]
```

## Description

The `restart` command stops running service(s) and starts them again.

For services defined in `.candle.json`, `restart` reloads the service
definition from the config file, so edits to a service's `shell` or `root`
take effect on the next restart. Transient processes (started with `--shell`
and not present in the config) are relaunched with the same command they were
originally started with.

## Arguments

- `name` - Name of the service(s) to restart. If omitted, restarts all running services in the current project directory.

## Examples

### Restart a specific service

```bash
candle restart api
```

### Restart all running services

```bash
candle restart
```

## See Also

- [start](start) - Start a service
- [kill](kill) - Stop a service
- [run](run) - Start and watch a service
