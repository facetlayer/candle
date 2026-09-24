# list-all

List all active services across the entire system.

## Syntax

```bash
candle list-all [--json]
```

## Description

The `list-all` command displays all running services that were started by Candle, regardless of which project directory they were started from.

This is different from `candle list`, which only shows services in the current project directory.

Output is a table, in the same style as [ps](ps) but with the process's command and directory
included — those are the only way to tell processes from different projects apart. The DIRECTORY
column (and `workingDir` in JSON) is the directory the process runs in, the same one `candle list`
shows: the project directory, or the service's root inside it. In JSON, `projectDir` is the
project the service belongs to; pass it as `--project-dir` to act on that service from anywhere,
for example `candle kill api --project-dir <projectDir>`.

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
