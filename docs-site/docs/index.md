# Candle

Candle is a lightweight process manager designed for local development. It allows you to start, stop, and manage multiple services from a single CLI, with built-in log aggregation and watching capabilities.

## Features

- **Simple Configuration** - Define services in a `.candle.json` file with just a name and shell command
- **Project-Scoped** - Commands are scoped to your current project directory by default
- **Log Aggregation** - All service output is stored in a SQLite database for easy retrieval
- **Watch Mode** - Monitor live output from running services
- **Transient Services** - Run one-off commands without configuration
- **MCP Integration** - Built-in Model Context Protocol server for AI agent integration

## Quick Start

```bash
# Install globally
npm install -g candle

# Create a config file
echo '{"services": [{"name": "api", "shell": "npm run dev"}]}' > .candle.json

# Start your service
candle start api

# View logs
candle logs api

# Watch live output
candle watch api

# Stop the service
candle kill api
```

## Core Concepts

### Services
A service is a named process defined in your `.candle.json` configuration file. Services can be started, stopped, restarted, and monitored using Candle commands.

### Project Scope
Candle tracks services by project directory. When you run commands like `list` or `kill` without arguments, they only affect services started from the current project directory.

### Transient Services
You can also run services without defining them in a config file using the `--shell` flag:

```bash
candle run server --shell "python -m http.server 8080"
```

## Commands Overview

| Command | Description |
|---------|-------------|
| [`run`](commands/run) | Start service(s) and watch their output |
| [`start`](commands/start) | Start service(s) in the background |
| [`restart`](commands/restart) | Restart running service(s) |
| [`kill`](commands/kill) | Stop running service(s) |
| [`list`](commands/list) | List active services |
| [`logs`](commands/logs) | View recent logs |
| [`watch`](commands/watch) | Watch live service output |
| [`wait-for-log`](commands/wait-for-log) | Wait for a specific log message |

## See Also

- [Getting Started](getting-started) - Installation and setup guide
- [Configuration](configuration) - Configuration file reference
- [MCP Integration](mcp-integration) - Using Candle with AI agents
