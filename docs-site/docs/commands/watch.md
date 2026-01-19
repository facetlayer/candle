# watch

Watch live output from running service(s).

## Syntax

```bash
candle watch [name...]
```

## Description

The `watch` command enters interactive mode to display real-time output from one or more running services. Press `Ctrl+C` to exit watch mode.

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

### Watch all services in this project

```bash
candle watch
```

## See Also

- [run](run) - Start and watch a service
- [logs](logs) - View recent logs (non-interactive)
- [start](start) - Start a service in the background
