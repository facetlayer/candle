# mcp

Enter MCP (Model Context Protocol) server mode.

## Syntax

```bash
candle mcp
```

## Description

The `mcp` command starts Candle as an MCP server, allowing AI assistants and other MCP clients to manage services programmatically.

You can also use `--mcp` as an alternative flag.

## MCP Tools

When running as an MCP server, the following tools are available:

- `ListServices` - List services for a project
- `StartService` - Start a configured service
- `StartTransientService` - Start a transient process
- `KillService` - Stop a running service
- `RestartService` - Restart a service
- `GetLogs` - Get recent logs for a service
- `ListPorts` - List open ports for running services
- `ReservePort` - Reserve a port for a service
- `ReleasePorts` - Release reserved ports
- `ListReservedPorts` - List reserved ports
- `GetReservedPort` - Get the reserved port for a service
- `AddServerConfig` - Add a service to configuration

## Examples

### Start MCP server

```bash
candle mcp
```

## See Also

- [help](help) - Show help information
