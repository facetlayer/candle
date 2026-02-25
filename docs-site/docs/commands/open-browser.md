# open-browser

Open a browser window to a running service's port.

## Syntax

```bash
candle open-browser [name]
```

## Description

The `open-browser` command finds the open port for a running service and opens a browser window to `http://localhost:<port>`.

This command uses the same port detection as `list-ports` - it finds the actual listening port of the running process.

## Arguments

- `name` - Name of the service to open in browser (optional)

If no service name is provided, the command will auto-detect the target:
- If exactly one process is running in the project, that service is used
- If no processes are running, an error is thrown
- If multiple processes are running, an error is thrown (ambiguous)

## Examples

### Open browser to a running service

```bash
candle open-browser api
# Opens http://localhost:3000 (or whatever port the service is listening on)
```

### Auto-detect with a single running service

```bash
candle open-browser
# Automatically opens the browser to the only running service
```

### Start a service and open browser

```bash
candle start api && candle open-browser api
```

## Errors

The command will fail if:
- The service is not running
- The service has no open ports
- No service name was provided and no processes are running
- No service name was provided and multiple processes are running

## Platform Support

- **macOS**: Uses `open` command
- **Linux**: Uses `xdg-open` command
- **Windows**: Uses `start` command

## See Also

- [list-ports](list-ports) - List open ports for running services
- [start](start) - Start services
- [run](run) - Run services in foreground
