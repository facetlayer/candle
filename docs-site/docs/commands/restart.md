# restart

Restart a running process service.

## Syntax

```bash
candle restart [name]
```

## Description

The `restart` command stops a running service and starts it again. This is useful when you've made configuration changes or want to reset the service state.

## Arguments

- `name` - Name of the service to restart. If omitted, restarts all running services in the current project.

## Examples

### Restart a specific service

```bash
candle restart api
```

### Restart all running services

```bash
candle restart
```

## Behavior

1. The specified service is killed (if running)
2. The service is started again with the same configuration
3. Logs from the new process are available immediately

## Exit Codes

- `0` - Service restarted successfully
- `1` - Service not found or error occurred

## See Also

- [start](start) - Start a service
- [kill](kill) - Stop a service
- [run](run) - Start and watch a service
