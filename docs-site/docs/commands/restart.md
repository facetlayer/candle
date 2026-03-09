# restart

Restart running service(s).

## Syntax

```bash
candle restart [name...]
```

## Description

The `restart` command stops running service(s) and starts them again.

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
