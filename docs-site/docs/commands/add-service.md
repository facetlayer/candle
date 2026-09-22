# add-service

Add a new service to the configuration file.

## Syntax

```bash
candle add-service <name> --shell <command> [options]
```

## Description

The `add-service` command adds a new service definition to your `.candle.json` configuration file. Candle uses the nearest `.candle.json` in the current directory or a parent directory. If there isn't one, it creates `.candle.json` in the current directory.

## Arguments

- `name` - Name for the new service (required). Use only letters, digits, `-`, `_` and `.`, so the name never needs quoting in later commands. Other characters, such as spaces or shell characters, are rejected.

## Options

- `--shell <command>` - Shell command to run the service (required)
- `--root <directory>` - Working directory for the service, relative to the project directory. The directory must already exist.

## Examples

### Add a basic service

```bash
candle add-service api --shell "npm run dev"
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
candle add-service api --shell "npm run dev" --root packages/api
```

## Behavior

1. If `.candle.json` exists, the service is added to the existing configuration
2. If it doesn't exist, a new configuration file is created
3. Existing services are preserved, including any keys Candle doesn't recognize
4. The file is written with 2-space indentation and a trailing newline

## Notes

- Service names must be unique within a configuration file
- The shell command is stored as-is and executed in a shell environment
- A relative root path can't use `..` to escape the project directory

## See Also

- [Configuration](../configuration) - Full configuration reference
- [start](start) - Start the newly added service
