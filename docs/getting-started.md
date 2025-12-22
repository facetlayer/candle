---
name: getting-started
description: Quick start guide for using Candle
---

# Getting Started with Candle

Candle is a process manager for local development. It helps you run, monitor, and manage background services.

## Basic Usage

### Running a Service

```bash
# Run a service in the foreground (with live logs)
candle run my-service

# Start services in the background
candle start my-service
candle start service1 service2
```

### Managing Running Services

```bash
# List running services
candle list

# View logs for a service
candle logs my-service

# Watch live output
candle watch my-service

# Stop a service
candle stop my-service
candle kill my-service
```

## Configuration

Services are defined in a `.candle.json` file in your project directory:

```json
{
  "services": {
    "api": {
      "shell": "npm run dev"
    },
    "worker": {
      "shell": "npm run worker",
      "root": "./packages/worker"
    }
  }
}
```

## MCP Integration

Candle can be used as an MCP server for AI assistants:

```bash
candle --mcp
```

This exposes tools for listing, starting, stopping, and viewing logs of services.
