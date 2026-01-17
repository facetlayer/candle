# Configuration

Candle uses a JSON configuration file to define services for your project.

## Configuration File

Candle looks for configuration files in this order:

1. `.candle.json` (recommended)
2. `.candle-setup.json` (deprecated, still supported)

The configuration file should be placed in your project root. Candle will search upward from the current directory to find it.

## Schema

```json
{
  "services": [
    {
      "name": "string",
      "shell": "string",
      "root": "string (optional)",
      "enableStdin": "boolean (optional)"
    }
  ]
}
```

## Service Fields

### name (required)

A unique identifier for the service. Used to reference the service in all commands.

```json
{
  "name": "api"
}
```

### shell (required)

The shell command to execute when starting the service.

```json
{
  "shell": "npm run dev"
}
```

### root (optional)

A relative directory path where the command will run. Must be relative to the config file location.

**Constraints:**
- Cannot be an absolute path
- Cannot use `..` to escape the project directory

```json
{
  "root": "packages/api"
}
```

### enableStdin (optional)

Enable stdin message polling from the database. This is an advanced feature for sending input to running processes.

```json
{
  "enableStdin": true
}
```

## Complete Example

```json
{
  "services": [
    {
      "name": "api",
      "shell": "npm run dev",
      "root": "packages/api"
    },
    {
      "name": "web",
      "shell": "npm start",
      "root": "packages/web"
    },
    {
      "name": "worker",
      "shell": "node worker.js",
      "enableStdin": true
    },
    {
      "name": "database",
      "shell": "docker-compose up postgres"
    }
  ]
}
```

## Adding Services via CLI

You can add services without manually editing the config file:

```bash
candle add-service api "npm run dev" --root packages/api
```

This will create or update `.candle.json` with the new service.

## Database Location

Candle stores process state and logs in a SQLite database at:

```
~/.candle/candle.db
```

This can be overridden with the `LOCAL_SERVER_STATE_DIR` environment variable.

## See Also

- [Getting Started](getting-started) - Quick setup guide
- [add-service](commands/add-service) - Add services via CLI
