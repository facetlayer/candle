# list-ports

List open ports for running services in the current project directory.

## Syntax

```bash
candle list-ports
```

## Description

The `list-ports` command displays all open (listening) ports used by running services that were started from the current project directory. It also shows ports opened by child processes spawned by those services.

## Output Format

The command outputs a table with the following columns:

| Column | Description |
|--------|-------------|
| SERVICE | Service name |
| PID | Process ID that has the port open |
| PORT | Port number |
| ADDRESS | Network address (e.g., 127.0.0.1, 0.0.0.0) |
| PROTOCOL | Network protocol (TCP/UDP), with "(child)" suffix for child processes |

## Examples

### List ports in current project

```bash
candle list-ports
```

Example output:

```
SERVICE  PID    PORT  ADDRESS    PROTOCOL
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
