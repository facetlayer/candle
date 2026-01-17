# Getting Started

This guide will help you install Candle and set up your first project.

## Installation

Install Candle globally using npm:

```bash
npm install -g candle
```

Or using pnpm:

```bash
pnpm add -g candle
```

## Creating a Configuration File

Create a `.candle.json` file in your project root:

```json
{
  "services": [
    {
      "name": "api",
      "shell": "npm run dev"
    },
    {
      "name": "web",
      "shell": "npm start",
      "root": "packages/web"
    }
  ]
}
```

Each service requires:
- `name` - A unique identifier for the service
- `shell` - The command to run

Optional fields:
- `root` - Subdirectory to run the command in (relative to config file location)
- `enableStdin` - Enable stdin message polling (advanced use case)

## Starting Services

Start a specific service:

```bash
candle start api
```

Start multiple services:

```bash
candle start api web
```

Start and watch output interactively:

```bash
candle run api
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

## Running Transient Commands

Run a command without adding it to your config:

```bash
candle run --shell "python -m http.server 8080"
```

Start a transient process in the background:

```bash
candle start --shell "node server.js" --root ./backend
```

## Adding Services via CLI

Add a service to your config file without editing it manually:

```bash
candle add-service myservice "npm run myservice"
```

## Next Steps

- [Configuration](configuration) - Learn about all configuration options
- [Commands](commands/run) - Explore all available commands
- [MCP Integration](mcp-integration) - Use Candle with AI agents
