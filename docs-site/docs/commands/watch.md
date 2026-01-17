# watch

Watch live output from running process(es).

## Syntax

```bash
candle watch [name...]
```

## Description

The `watch` command enters interactive mode to display real-time output from one or more running services. Press `Ctrl+C` to exit watch mode.

Unlike `logs`, this command requires the service to be currently running.

## Arguments

- `name` - Name of the service(s) to watch. Can specify multiple services.

## Examples

### Watch a single service

```bash
candle watch api
```

### Watch multiple services

```bash
candle watch api web
```

## Behavior

1. Connects to the running service(s)
2. Displays real-time output as it's generated
3. Press `Ctrl+C` to exit
4. The service continues running after exiting

## Error Cases

- Returns an error if the specified service is not currently running
- Use `candle start` or `candle run` to start the service first

## See Also

- [run](run) - Start and watch a service
- [logs](logs) - View recent logs (non-interactive)
- [start](start) - Start a service in the background
