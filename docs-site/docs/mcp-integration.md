# MCP Integration

Candle includes a built-in Model Context Protocol (MCP) server, allowing AI agents like Claude Code to manage your development services.

## Starting the MCP Server

```bash
candle --mcp
```

This starts Candle in MCP server mode, communicating via stdin/stdout using the MCP protocol.

## Available Tools

When running as an MCP server, Candle exposes the following tools:

### ListServices

List services with structured output.

**Parameters:**
- `showAll` (boolean, optional) - Show all services globally, not just current directory

### GetLogs

Get recent logs for a specific service.

**Parameters:**
- `name` (string, required) - Name of the service
- `limit` (number, optional) - Maximum number of log lines to return
- `projectDir` (string, optional) - Project directory for cross-directory access

### StartService

Start a config-defined service.

**Parameters:**
- `name` (string, required) - Name of the service to start

### StartTransientService

Start a transient service with a custom shell command.

**Parameters:**
- `name` (string, required) - Name for the transient service
- `shell` (string, required) - Shell command to run
- `root` (string, optional) - Root directory for the service

### KillService

Kill a running service.

**Parameters:**
- `name` (string, required) - Name of the service to kill

### RestartService

Restart a running service.

**Parameters:**
- `name` (string, optional) - Name of the service to restart. If not provided, restarts all running services.

### AddServerConfig

Add a new server configuration to the config file.

**Parameters:**
- `name` (string, required) - Name of the service
- `shell` (string, required) - Shell command to run
- `root` (string, optional) - Root directory for the service

## Claude Code Integration

To use Candle with Claude Code, add it to your MCP configuration. Claude Code can then:

- Start and stop development servers
- View logs from running services
- Add new services to your configuration
- Restart services after making changes

### Initial Setup

Before using MCP tools, you'll typically want to configure your services. You can do this via:

- The CLI: `candle add-service api "npm run dev"`
- The MCP `AddServerConfig` tool (see above)

See [add-service](commands/add-service) for CLI usage details.

## Example MCP Workflow

1. Claude Code reads your project and identifies you need a dev server
2. Uses `StartService` to start your API server
3. Uses `GetLogs` to check if the server started successfully
4. After making code changes, uses `RestartService` to pick up the changes

## See Also

- [Getting Started](getting-started) - Basic Candle setup
- [Configuration](configuration) - Configuration file format
