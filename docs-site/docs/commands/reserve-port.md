# reserve-port

Reserve an unused port for a service or project.

## Syntax

```bash
candle reserve-port [name]
```

## Description

The `reserve-port` command reserves an unused port number and stores it in the Candle database. This port can be associated with a specific service or reserved at the project level.

When a service with a reserved port is started, Candle automatically sets the `PORT` environment variable to the reserved port number.

## Arguments

- `name` - Name of the service to reserve the port for (optional). If omitted, reserves a project-level port.

## Port Assignment

Ports are assigned starting from 4000 and incrementing up to 65535 (with wraparound). Before assigning a port, Candle:

1. Checks the database to ensure the port is not already reserved
2. Tests if the port is actually available on the system by attempting to bind to it
3. Handles race conditions if multiple processes try to reserve ports simultaneously

If all ports in the range (4000-65535) are exhausted, the command will fail with an error.

## Examples

### Reserve a port for a service

```bash
candle reserve-port api
```

Output:
```
Reserved port 4000 for service 'api'
```

### Reserve a project-level port

```bash
candle reserve-port
```

Output:
```
Reserved port 4001 for project
```

## Behavior

- Each service can only have one reserved port
- Attempting to reserve a port for a service that already has one will result in an error
- Project-level ports (without a service name) can be reserved multiple times
- Reserved ports persist across restarts until explicitly released

## Exit Codes

- `0` - Port reserved successfully
- `1` - Error (service already has a port, no available ports, etc.)

## Errors

If the service already has a reserved port:
```
Error: Service 'api' already has a reserved port: 4000
```

## MCP

This command is also available as the `ReservePort` MCP tool with the optional `serviceName` parameter.

## See Also

- [release-ports](release-ports) - Release reserved ports
- [list-reserved-ports](list-reserved-ports) - List reserved ports
- [get-reserved-port](get-reserved-port) - Get the reserved port for a service
- [start](start) - Start services (uses PORT env var if reserved)
