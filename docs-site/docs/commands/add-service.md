# add-service

Add a new service to the configuration file.

## Syntax

```bash
candle add-service <name> <shell> [options]
```

## Description

The `add-service` command adds a new service definition to your `.candle.json` configuration file. If the file doesn't exist, it will be created.

## Arguments

- `name` - Name for the new service (required)
- `shell` - Shell command to run (required)

## Options

- `--root <directory>` - Working directory for the service (relative path)
- `--enable-stdin` - Enable stdin message polling

## Examples

### Add a basic service

```bash
candle add-service api "npm run dev"
```

This creates or updates `.candle.json`:

```json
{
  "services": [
    {
      "name": "api",
      "shell": "npm run dev"
    }
  ]
}
```

### Add a service with a root directory

```bash
candle add-service api "npm run dev" --root packages/api
```

### Add a service with stdin enabled

```bash
candle add-service worker "node worker.js" --enable-stdin
```

## Behavior

1. If `.candle.json` exists, the service is added to the existing configuration
2. If it doesn't exist, a new configuration file is created
3. Existing services are preserved

## Notes

- Service names must be unique within a configuration file
- The shell command is stored as-is and executed in a shell environment
- Root paths must be relative and cannot escape the project directory

## See Also

- [Configuration](../configuration) - Full configuration reference
- [start](start) - Start the newly added service
