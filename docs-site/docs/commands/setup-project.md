# setup-project

Create a new `.candle.json` configuration file in the current directory.

## Syntax

```bash
candle setup-project
```

## Description

The `setup-project` command initializes a new Candle project by creating a `.candle.json` configuration file in the current directory. The file is created with an empty services array, ready for you to add services.

If a `.candle.json` already exists in the current directory or any parent directory, the command will report its location without making any changes.

## Examples

### Initialize a new project

```bash
cd my-project
candle setup-project
```

This creates `.candle.json`:

```json
{
  "services": []
}
```

### Then add services

```bash
candle setup-project
candle add-service api --shell "npm run dev"
candle add-service web --shell "npm run start"
```

## Behavior

1. Searches the current directory and parent directories for an existing config file
2. If a config file is found, prints its location and exits without changes
3. If no config file exists, creates a new `.candle.json` with an empty services array

## See Also

- [add-service](add-service) - Add a service to the configuration file
- [Configuration](../configuration) - Full configuration reference
