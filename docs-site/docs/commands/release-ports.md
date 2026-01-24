# release-ports

Release reserved port(s) for a service or project.

## Syntax

```bash
candle release-ports [name]
```

## Description

The `release-ports` command releases previously reserved ports from the Candle database, making them available for other services or projects.

## Arguments

- `name` - Name of the service to release the port for (optional). If omitted, releases all reserved ports for the current project.

## Examples

### Release port for a specific service

```bash
candle release-ports api
```

Output:
```
Released port 4000 for service 'api'
```

### Release all ports for current project

```bash
candle release-ports
```

Output:
```
Released 3 reserved ports
```

### When no ports are reserved

```bash
candle release-ports
```

Output:
```
No reserved ports to release
```

## Behavior

- When a service name is provided, only the port for that service is released
- When no service name is provided, all reserved ports for the current project are released
- If no ports exist to release (when called without a service name), the command succeeds with a message
- Releasing a port does not affect any currently running services using that port

## Exit Codes

- `0` - Ports released successfully (or no ports to release when called without a service name)
- `1` - Error (specified service has no reserved port)

## Errors

If no reserved port exists for the specified service:
```
Error: No reserved port found for service 'api'
```

## MCP

This command is also available as the `ReleasePorts` MCP tool with the optional `serviceName` parameter.

## See Also

- [reserve-port](reserve-port) - Reserve an unused port
- [list-reserved-ports](list-reserved-ports) - List reserved ports
- [get-reserved-port](get-reserved-port) - Get the reserved port for a service
