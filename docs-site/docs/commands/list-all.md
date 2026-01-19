# list-all

List all active services across the entire system.

## Syntax

```bash
candle list-all
```

## Description

The `list-all` command displays all running services that were started by Candle, regardless of which project directory they were started from.

This is different from `candle list`, which only shows services in the current project directory.

## Example

```bash
candle list-all
```

## See Also

- [list](list) - List services in current project
- [kill-all](kill-all) - Kill all services globally
