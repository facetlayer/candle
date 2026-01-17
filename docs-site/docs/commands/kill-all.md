# kill-all

Kill all processes globally across the entire system.

## Syntax

```bash
candle kill-all
```

## Description

The `kill-all` command terminates all processes that were started by Candle, regardless of which project directory they were started from.

This is different from `candle kill` (without arguments), which only kills services in the current project.

## Example

```bash
candle kill-all
```

## Behavior

1. Finds all processes tracked by Candle across all projects
2. Sends termination signals to each process
3. Removes them from the running services list

## Use Cases

- Cleaning up before erasing the database
- Stopping all development services at once
- Recovering from a situation where you've lost track of running services

## Comparison

| Command | Scope |
|---------|-------|
| `candle kill` | Current project only |
| `candle kill-all` | All projects globally |

## See Also

- [kill](kill) - Kill services in current project
- [list-all](list-all) - List all services globally
- [erase-database](erase-database) - Reset the database
