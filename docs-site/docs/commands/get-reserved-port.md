# get-reserved-port

Get the reserved port for a specific service.

## Syntax

```bash
candle get-reserved-port <name>
```

## Description

The `get-reserved-port` command retrieves and prints the reserved port number for a specific service. This is useful in scripts or when you need to know which port a service will use.

This command only works for service-level port reservations. To view project-level ports, use `list-reserved-ports`.

## Arguments

- `name` - Name of the service to get the reserved port for (required)

## Examples

### Get the reserved port for a service

```bash
candle get-reserved-port api
```

Output:
```
4000
```

### Use in a script

```bash
PORT=$(candle get-reserved-port api)
curl http://localhost:$PORT/health
```

## Behavior

- Only prints the port number, making it easy to use in scripts
- Only retrieves ports reserved for a specific service (not project-level ports)
- If no port is reserved for the service, exits with an error

## Exit Codes

- `0` - Port retrieved successfully
- `1` - Error (no reserved port for the specified service)

## Errors

If no reserved port exists for the specified service:
```
Error: No reserved port found for service 'api'
```

## MCP

This command is also available as the `GetReservedPort` MCP tool with the required `serviceName` parameter.

## See Also

- [reserve-port](reserve-port) - Reserve an unused port
- [release-ports](release-ports) - Release reserved ports
- [list-reserved-ports](list-reserved-ports) - List reserved ports
