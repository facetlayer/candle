# kill-all

Kill all services globally across the entire system.

## Syntax

```bash
candle kill-all
```

## Description

The `kill-all` command terminates all services that were started by Candle, regardless of which project directory they were started from.

This is different from `candle kill` (without arguments), which only kills services in the current project.

## Example

```bash
candle kill-all
```

## See Also

- [kill](kill) - Kill services in current project
- [list-all](list-all) - List all services globally
- [erase-database](erase-database) - Reset the database
