# kill / stop

Kill (stop) running processes.

## Syntax

```bash
candle kill [name...]
candle stop [name...]
```

## Description

The `kill` command stops running services. The `stop` command is an alias with identical behavior.

When called without arguments, it kills all services in the current project directory.

## Arguments

- `name` - Name of the service(s) to kill. If omitted, kills all services in the current project.

## Examples

### Kill a specific service

```bash
candle kill api
```

### Kill multiple services

```bash
candle kill api web worker
```

### Kill all services in current project

```bash
candle kill
```

### Using the stop alias

```bash
candle stop api
```

## Behavior

1. The specified service(s) receive a termination signal
2. The process is removed from the running services list
3. Logs are preserved and can still be viewed with `candle logs`

## Exit Codes

- `0` - Service(s) killed successfully
- `1` - Service not found or error occurred

## See Also

- [kill-all](kill-all) - Kill all services globally
- [start](start) - Start services
- [restart](restart) - Restart services
