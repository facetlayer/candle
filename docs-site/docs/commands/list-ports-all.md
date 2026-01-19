# list-ports-all

List open ports for all running services globally across the entire system.

## Syntax

```bash
candle list-ports-all
```

## Description

The `list-ports-all` command displays all open (listening) ports used by running services that were started by Candle, regardless of which project directory they were started from.

This is different from `candle list-ports`, which only shows ports for services in the current project.

## Output Format

The command outputs a table with the following columns:

| Column | Description |
|--------|-------------|
| SERVICE | Service name |
| PID | Process ID that has the port open |
| PORT | Port number |
| ADDRESS | Network address (e.g., 127.0.0.1, 0.0.0.0) |
| PROTOCOL | Network protocol (TCP/UDP), with "(child)" suffix for child processes |

## Example

```bash
candle list-ports-all
```

Example output:

```
SERVICE  PID    PORT  ADDRESS    PROTOCOL
api      12345  3000  127.0.0.1  TCP
api      12346  3001  127.0.0.1  TCP (child)
web      12400  8080  0.0.0.0    TCP
worker   12500  9000  127.0.0.1  TCP
```

## Comparison

| Command | Scope |
|---------|-------|
| `candle list-ports` | Current project only |
| `candle list-ports-all` | All projects globally |

## See Also

- [list-ports](list-ports) - List ports in current project
- [list-all](list-all) - List all services globally
- [list](list) - List services in current project
