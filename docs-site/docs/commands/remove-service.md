# remove-service

Remove a service from the configuration file.

## Syntax

```bash
candle remove-service <name>
```

## Description

The `remove-service` command removes a service definition from your `.candle.json` configuration file. The service must exist in the configuration.

## Arguments

- `name` - Name of the service to remove (required)

## Examples

### Remove a service

```bash
candle remove-service api
```

This removes the "api" service entry from `.candle.json`. Other services and configuration settings are preserved.

## Behavior

1. Finds the nearest `.candle.json` (or `.candle-setup.json`) configuration file
2. Removes the service with the matching name
3. Writes the updated configuration back to the file
4. Errors if the service name is not found

## Notes

- This only removes the service from the configuration file — it does not kill a running process. Use [kill](kill) first if the service is running.
- Other configuration fields (such as `logEviction`) are preserved.

## See Also

- [add-service](add-service) - Add a new service
- [kill](kill) - Kill a running service
- [Configuration](../configuration) - Full configuration reference
