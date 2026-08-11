# Getting Started

This guide will help you install Candle and set up your first project.

## Installation

Install the latest release with the one-line installer:

```bash
curl -fsSL https://raw.githubusercontent.com/facetlayer/candle/main/install.sh | sh
```

See [Installation](installation) for Homebrew, building from source, upgrading, and
uninstalling.

:::note
Candle was previously distributed as the `@facetlayer/candle` npm package. That package
is retired — Candle is now a native binary and is no longer installed through npm.
:::

## Configure a service

Use the `add-service` command to add a new configured service:

```bash
candle add-service api --shell "npm run dev"
```

This will create a `.candle.json` file in the current directory.

## Project Organization

Candle services are organized by project directory. The project directory is the nearest directory that has a `.candle.json` configuration file.

This means you can use Candle in different projects, and the services will be separate.

```
cd ~/projects/my-project-1
candle start api          # Starts the service named "api" for "my-project-1"
cd ~/projects/my-project-2
candle start api          # Starts a different service named "api" for "my-project-2"
```

See [Project Organization](project-organization) for more details.


## Starting Services

Start all configured services:

```bash
candle start
```

Start a specific service:

```bash
candle start api
```

Start multiple services:

```bash
candle start api web
```

## Viewing Logs

View recent logs from a service:

```bash
candle logs api
```

Watch live output:

```bash
candle watch api
```

## Stopping Services

Stop a specific service:

```bash
candle kill api
```

Stop all services in the current project:

```bash
candle kill
```

## Listing Services

List running services in current project:

```bash
candle list
```

List all running services globally:

```bash
candle list-all
```

## Next Steps

- [Configuration](configuration) - Learn about all configuration options
- [Commands](commands/run) - Explore all available commands
- [MCP Integration](mcp-integration) - Use Candle with AI agents
