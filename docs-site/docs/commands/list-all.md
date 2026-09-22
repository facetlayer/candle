# list-all

List all active services across the entire system.

## Syntax

```bash
candle list-all [--json]
```

## Description

The `list-all` command displays all running services that were started by Candle, regardless of which project directory they were started from.

This is different from `candle list`, which only shows services in the current project directory.

Output is a table, in the same style as [ps](ps) but with the process's command and project directory
included — those are the only way to tell processes from different projects apart.

## Options

- `--json` - Print the listing as a JSON array instead of the table, in the same shape as `candle list --json`.

## Example

```bash
candle list-all
```

## See Also

- [list](list) - List services in current project
- [ps](ps) - Compact status table for the current project
- [kill-all](kill-all) - Kill all services globally
