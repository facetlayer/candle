# list-ports

List open ports for running services in the current project directory.

## Syntax

```bash
candle list-ports [names...] [--json]
```

## Description

The `list-ports` command displays all open (listening) ports used by running services that were started from the current project directory. It also shows ports opened by child processes spawned by those services.

If `[names]` are provided, only show ports for those services. Otherwise, show ports for all running services in the current project. A name that is neither a configured service nor a transient service in this project is an error (`No service '<name>' configured ...`, exit code 1).

## Output Format

The command outputs a table with the following columns:

| Column | Description |
|--------|-------------|
| SERVICE | Service name |
| PID | Process ID that has the port open |
| PORT | Port number |
| ADDRESS | Network address (e.g., 127.0.0.1, 0.0.0.0) |
| PROTOCOL | Network protocol (always TCP, since only listening TCP sockets are detected), with "(child)" suffix for child processes |

### JSON output

With `--json`, the command prints an object with a `ports` array instead of the table:

```json
{
  "ports": [
    {
      "serviceName": "api",
      "pid": 12345,
      "port": 3000,
      "address": "127.0.0.1",
      "protocol": "TCP",
      "isChildProcess": false
    }
  ]
}
```

`isChildProcess` is `true` when the port is held by a child of the service's process rather than the process itself.

## Options

- `--json` - Output as JSON (see above).
- `--project-dir <dir>` - Act on the given project instead of the current directory. See [Targeting another project](../project-dir).

## Examples

### List ports in current project

```bash
candle list-ports
```

### List ports for specific services

```bash
candle list-ports api
candle list-ports api web
```

Example output:

```
SERVICE  PID    PORT  ADDRESS    PROTOCOL
-------  -----  ----  ---------  -----------
api      12345  3000  127.0.0.1  TCP
api      12346  3001  127.0.0.1  TCP (child)
web      12400  8080  0.0.0.0    TCP
```

## Behavior

- Only shows ports for services started from the current project directory
- Recursively discovers ports opened by child processes
- Uses `lsof` to detect listening ports
- Services started from other directories are not shown
- Use `candle list-ports-all` to see ports for all services globally

## See Also

- [list-ports-all](list-ports-all) - List ports for all services globally
- [list](list) - List running services
- [logs](logs) - View logs from services
