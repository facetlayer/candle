# list-all

List all active services across the entire system.

## Syntax

```bash
candle list-all
```

## Description

The `list-all` command displays all running services that were started by Candle, regardless of which project directory they were started from.

This is different from `candle list`, which only shows services in the current project directory.

Output is a table, in the same style as [ps](ps) but with the process's command and directory
included — those are the only way to tell processes from different projects apart.

## Example

```bash
candle list-all
```

## See Also

- [list](list) - List services in current project
- [ps](ps) - Compact status table for the current project
- [kill-all](kill-all) - Kill all services globally
