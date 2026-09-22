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
      "root": "string (optional)"
    }
  ],
  "logEviction": {
    "maxLogsPerService": "number (optional, default: 1000)",
    "maxRetentionSeconds": "number (optional, default: 86400)"
  }
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

The directory where the command will run. A relative path is resolved against the config file location. An absolute path is used as-is.

**Constraints:**
- A relative path cannot use `..` to escape the project directory

```json
{
  "root": "packages/api"
}
```

## Unknown keys

Candle warns on stderr about any key it doesn't recognize, at the top level or inside a service, each time it loads the config. The command still runs. When the key looks like a known one, the warning suggests it:

```
Warning: unknown key "cwd" in service "api" in .candle.json (did you mean "root"?)
```

Unknown keys are kept as-is when `add-service`, `remove-service` or `set-config` rewrites the file.

## Log Eviction

The `logEviction` field controls how Candle manages stored log data. By default, Candle keeps up to 1000 log entries per service and deletes logs older than 24 hours.

### maxLogsPerService (optional)

Maximum number of log entries to keep per service. When a service exceeds this limit, the oldest logs are removed during cleanup. Default: `1000`.

### maxRetentionSeconds (optional)

Maximum age of log entries in seconds. Logs older than this are deleted during cleanup. Default: `86400` (24 hours).

```json
{
  "logEviction": {
    "maxLogsPerService": 5000,
    "maxRetentionSeconds": 172800
  }
}
```

Cleanup runs at most once every 10 minutes, as part of a normal Candle command. Evicted logs are gone for good; `candle logs` doesn't flag them. It prints `-- showing the last N lines; use --count to see more --` only when `--count` cut off lines from the latest run.

## Log monitoring

Each service Candle starts is supervised by a monitor process that captures its
stdout/stderr and writes the output to the database. The monitor is the `candle`
binary re-invoking itself (`candle --monitor`); there is no separate collector
binary and nothing to configure.

:::note Removed setting
Older versions had a `logCollector` field for choosing between a Node.js and a Rust
collector sidecar. Neither exists anymore, so the field is gone: `candle set-config
logCollector ...` now reports an unknown key. A leftover `"logCollector"` entry in an
existing `.candle.json` is harmless: Candle ignores it, preserves it as-is, and prints an
unknown-key warning. Delete the entry to silence the warning.
:::

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
      "shell": "node worker.js"
    },
    {
      "name": "database",
      "shell": "docker-compose up postgres"
    }
  ],
  "logEviction": {
    "maxLogsPerService": 5000,
    "maxRetentionSeconds": 172800
  }
}
```

## Adding Services via CLI

You can add services without manually editing the config file:

```bash
candle add-service api --shell "npm run dev" --root packages/api
```

This will create or update `.candle.json` with the new service.

## Database Location

See [Database](database) for details on where Candle stores logs and service state.

## See Also

- [Getting Started](getting-started) - Quick setup guide
- [add-service](commands/add-service) - Add services via CLI
