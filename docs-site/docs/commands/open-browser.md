# open-browser

Open a browser window to a running service's port.

## Syntax

```bash
candle open-browser <name>
```

## Description

The `open-browser` command finds the open port for a running service and opens a browser window to `http://localhost:<port>`.

This command uses the same port detection as `list-ports` - it finds the actual listening port of the running process, not a reserved port.

## Arguments

- `name` - Name of the service to open in browser (required)

## Examples

### Open browser to a running service

```bash
candle open-browser api
# Opens http://localhost:3000 (or whatever port the service is listening on)
```

### Start a service and open browser

```bash
candle start api && candle open-browser api
```

## Errors

The command will fail if:
- The service is not running
- The service has no open ports

## Platform Support

- **macOS**: Uses `open` command
- **Linux**: Uses `xdg-open` command
- **Windows**: Uses `start` command

## See Also

- [list-ports](list-ports) - List open ports for running services
- [start](start) - Start services
- [run](run) - Run services in foreground
