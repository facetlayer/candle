# get-or-reserve-port

Get an existing reserved port or reserve a new one for a service.

## Syntax

```bash
candle get-or-reserve-port <name>
```

## Description

The `get-or-reserve-port` command returns the reserved port for a service. If no port has been reserved yet, it automatically reserves one.

This is useful in scripts where you want to ensure a port is available without checking first whether one already exists.

The command outputs just the port number, making it easy to capture in scripts.

## Arguments

- `name` - Name of the service to get or reserve a port for (required)

## Output

Outputs the port number to stdout.

## Examples

### Get or reserve a port for a service

```bash
candle get-or-reserve-port api
# Output: 54321
```

### Use in a script

```bash
PORT=$(candle get-or-reserve-port api)
echo "API will run on port $PORT"
```

### Start a service with its reserved port

```bash
PORT=$(candle get-or-reserve-port api) npm start
```

## See Also

- [reserve-port](reserve-port) - Reserve a new port
- [get-reserved-port](get-reserved-port) - Get an existing reserved port (errors if none exists)
- [release-ports](release-ports) - Release reserved ports
- [list-reserved-ports](list-reserved-ports) - List all reserved ports
