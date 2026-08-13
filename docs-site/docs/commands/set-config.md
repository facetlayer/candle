# set-config

Set a configuration option in `.candle.json`.

## Syntax

```bash
candle set-config <key> <value>
```

## Description

The `set-config` command modifies a configuration option in your project's `.candle.json` file. This provides a convenient way to change settings without manually editing the JSON file.

## Arguments

- `key` - The configuration key to set (required)
- `value` - The value to set (required)

## Available Keys

| Key | Description | Valid Values |
|-----|-------------|--------------|
| `logEviction.maxLogsPerService` | Maximum number of log entries kept per service | Positive integer (default: 1000) |
| `logEviction.maxRetentionSeconds` | Maximum age of log entries in seconds | Positive integer (default: 86400) |

## Examples

### Increase log retention

```bash
candle set-config logEviction.maxLogsPerService 5000
candle set-config logEviction.maxRetentionSeconds 172800
```

## Behavior

1. Finds the nearest `.candle.json` in the current directory or parent directories
2. Validates the key and value
3. Updates the configuration file
4. Existing settings and services are preserved

## Notes

- The command validates values before writing to prevent invalid configuration
- `logCollector` is no longer a valid key. It chose between the old Node.js and Rust
  collector sidecars; Candle now ships one binary that supervises services itself.

## See Also

- [setup-project](setup-project) - Create a new configuration file
- [add-service](add-service) - Add a service to configuration
