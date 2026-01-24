# list-reserved-ports

List reserved ports for the current project.

## Syntax

```bash
candle list-reserved-ports
candle list-reserved-ports-all
```

## Description

The `list-reserved-ports` command displays all reserved ports for the current project directory.

Use `list-reserved-ports-all` to show reserved ports across all projects.

## Examples

### List reserved ports for current project

```bash
candle list-reserved-ports
```

Output:
```
Reserved ports:

  Port 4000 (service: api)
    Project: /path/to/project
    Reserved at: 1/23/2026, 10:30:00 AM

  Port 4001 (service: web)
    Project: /path/to/project
    Reserved at: 1/23/2026, 10:31:00 AM

  Port 4002 (project-level)
    Project: /path/to/project
    Reserved at: 1/23/2026, 10:32:00 AM
```

### List all reserved ports

```bash
candle list-reserved-ports-all
```

### When no ports are reserved

```bash
candle list-reserved-ports
```

Output:
```
No reserved ports found for current project
```

## Behavior

- Ports are displayed in reverse chronological order (most recently reserved first)
- Each entry shows the port number, associated service (if any), project directory, and reservation timestamp
- Project-level ports (not associated with a service) are labeled as "(project-level)"

## Exit Codes

- `0` - Always succeeds (even when no ports are reserved)

## MCP

This command is also available as the `ListReservedPorts` MCP tool with the optional `showAll` parameter.

## See Also

- [reserve-port](reserve-port) - Reserve an unused port
- [release-ports](release-ports) - Release reserved ports
- [get-reserved-port](get-reserved-port) - Get the reserved port for a service
