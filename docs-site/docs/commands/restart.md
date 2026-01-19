# restart

Restart a running service.

## Syntax

```bash
candle restart [name]
```

## Description

The `restart` command stops a running service and starts it again.

## Arguments

- `name` - Name of the service to restart. If omitted, restarts all running services in the current project directory.

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
