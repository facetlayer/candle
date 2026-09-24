---
name: mcp-usage
description: Run Candle as an MCP server for AI agents
---

# Using Candle as an MCP Server

Candle can run as a Model Context Protocol (MCP) server, so an AI agent can
start, stop and inspect services through tool calls instead of the shell.

Note: for coding agents that can run shell commands (such as Claude Code), the
plain `candle` CLI is the recommended integration. It's simpler, and the CLI
detects agents and never blocks. See `candle get-doc agents-intro`. MCP mode is
for clients that can't run shell commands, or where you want to restrict the
agent to a fixed set of operations.

## Launching

```bash
candle mcp
```

The server speaks MCP over stdin/stdout, so it is meant to be launched by an
MCP client, not run by hand. The project is the `.candle.json` found from the
server's working directory, so launch it from the project root.

Claude Code example:

```bash
claude mcp add candle -- candle mcp
```

Generic client configuration:

```json
{
  "mcpServers": {
    "candle": {
      "command": "candle",
      "args": ["mcp"]
    }
  }
}
```

## Tools

 - `ListServices` — services in the project with status, PID and uptime. `showAll: true` lists every project on the machine.
 - `StartService` — start a service defined in `.candle.json`. Does nothing if it's already running.
 - `StartTransientService` — start a one-off process from a `shell` command (and optional `root`). See `candle get-doc transient-processes`.
 - `RestartService` — restart one service (starting it if it's stopped), or every service in the project if no name is given.
 - `KillService` — stop a service.
 - `GetLogs` — recent output for a service (`limit`, default 200).
 - `ListPorts` — ports the running services are listening on.
 - `OpenBrowser` — open a browser to a service's detected port.
 - `AddServerConfig` — add a service to `.candle.json`.

`StartService` and `StartTransientService` are separate tools so a client can
allow launching configured services while denying arbitrary commands.
