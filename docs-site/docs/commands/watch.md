# watch

Launch service(s) if needed and watch their live output.

## Syntax

```bash
candle watch [name...]
```

## Description

The `watch` command ensures the target services are running and then enters interactive mode to display real-time output from them. Press `Ctrl+C` to exit watch mode (services will keep running in the background).

- If called with no service names, `watch` behaves like [start](start): it launches all services configured in `.candle.json` and then watches them.
- If called with service names, `watch` ensures each named service is running before watching it.
- Services that are already running are left alone — they are not restarted.

## Arguments

- `name` - Name of the service(s) to watch. Can specify multiple services.

## Examples

### Launch and watch a single service

```bash
candle watch api
```

### Launch and watch multiple services

```bash
candle watch api web
```

### Launch and watch all services in this project

```bash
candle watch
```

## See Also

- [run](run) - Start and watch a service
- [logs](logs) - View recent logs (non-interactive)
- [start](start) - Start a service in the background
