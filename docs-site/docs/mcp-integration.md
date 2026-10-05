# MCP Integration

Candle includes a built-in Model Context Protocol (MCP) server, allowing AI agents like Claude Code to manage your development services.

Note: The recommended way to use `candle` with your agent is as a command-line app instead of an MCP integration: have the agent run `candle --help`. See [Interactive and Agent Mode](agent-mode#using-candle-from-a-coding-agent). MCP support is maintained in case you need it, for example for a client that can't run shell commands.

## Starting the MCP Server

```bash
candle mcp
```

This starts Candle in MCP server mode, communicating via stdin/stdout using the MCP protocol. See [mcp](commands/mcp) for client configuration examples.

## Available Tools

When running as an MCP server, Candle exposes the following tools:

### ListServices

List services with structured output.

**Parameters:**
- `showAll` (boolean, optional) - Show all services globally, not just current directory

### ListPorts

List open ports for running services.

**Parameters:**
- `showAll` (boolean, optional) - Show ports for all services globally, not just the current project
- `serviceName` (string, optional) - Filter to a specific service

### GetLogs

Get recent logs for a specific service.

**Parameters:**
- `name` (string, required) - Name of the service
- `limit` (number, optional) - Maximum number of log lines to return (default: 200)
- `previous` (boolean, optional) - Return the run before the latest one, like `candle logs --previous`
- `allRuns` (boolean, optional) - Return every stored run, oldest first, like `candle logs --all-runs`. Can't be combined with `previous`
- `projectDir` (string, optional) - Project directory for cross-directory access

A name that isn't configured in the project, and has no stored logs or process, is an error: `No service '<name>' configured for directory: <dir>`, the same as `candle logs`. A finished transient service still has its logs, so its name keeps working.

When earlier lines from the latest run were left out, the output starts with a hint to pass a larger `limit`.

### StartService

Start a config-defined service. Does nothing if it's already running (use `RestartService` to restart it).

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

An unknown name is an error, `No service '<name>' configured for directory: <dir>`, like `candle kill`.

### RestartService

Restart a service, starting it if it's stopped.

**Parameters:**
- `name` (string, optional) - Name of the service to restart. If not provided, restarts every service in the project.

### AddServerConfig

Add a new server configuration to the config file.

**Parameters:**
- `name` (string, required) - Name of the service
- `shell` (string, required) - Shell command to run
- `root` (string, optional) - Root directory for the service

### OpenBrowser

Open a browser window to a running service's lowest listening port.

**Parameters:**
- `serviceName` (string, required) - Name of the service to open

## Claude Code Integration

Claude Code can run shell commands, so it doesn't need the MCP server: let it use the `candle` CLI and have it run `candle --help`.

If you want the MCP server anyway, for example to limit the agent to a fixed set of operations, register it with:

```bash
claude mcp add candle -- candle mcp
```

Configure your services first, either with the CLI (`candle add-service api --shell "npm run dev"`, see [add-service](commands/add-service)) or with the `AddServerConfig` tool.

## Example MCP Workflow

1. Claude Code reads your project and identifies you need a dev server
2. Uses `StartService` to start your API server
3. Uses `GetLogs` to check if the server started successfully
4. After making code changes, uses `RestartService` to pick up the changes

## See Also

- [Interactive and Agent Mode](agent-mode) - Using the CLI from a coding agent
- [mcp](commands/mcp) - The `mcp` command
- [Getting Started](getting-started) - Basic Candle setup
- [Configuration](configuration) - Configuration file format
