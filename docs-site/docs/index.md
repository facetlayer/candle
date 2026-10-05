# Candle

Candle is a lightweight process manager designed for local development. It allows you to start, stop, and manage multiple services from a single CLI, with built-in log aggregation and watching capabilities.

## Features

- **Simple Configuration** - Define services in a `.candle.json` file with just a name and shell command
- **Project-Scoped** - Commands are scoped to your current project directory by default
- **Log Aggregation** - All service output is stored in a SQLite database for easy retrieval
- **Watch Mode** - Monitor live output from running services
- **Transient Services** - Run one-off commands without adding them to your config
- **Agent-Friendly** - Detects coding agents and never blocks them, so an agent can use the same CLI you do

## Quick Start

```bash
# Install
curl -fsSL https://raw.githubusercontent.com/facetlayer/candle/main/install.sh | sh

# Add a service (this creates .candle.json if there isn't one)
candle add-service api --shell "npm run dev"

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
You can also run services without defining them in the config file using the `--shell` flag (a `.candle.json` must still exist in the project):

```bash
candle start server --shell "python -m http.server 8080"
```

## Commands

Every command has its own page, listed in the sidebar by the same categories as `candle --help`.

## See Also

- [Installation](installation) - Install, upgrade and uninstall
- [Getting Started](getting-started) - Set up your first project
- [Configuration](configuration) - Configuration file reference
- [Interactive and Agent Mode](agent-mode) - Using Candle with coding agents
