# list-all

List all active processes globally across the entire system.

## Syntax

```bash
candle list-all
```

## Description

The `list-all` command displays all running services that were started by Candle, regardless of which project directory they were started from.

This is different from `candle list`, which only shows services in the current project.

## Output Format

The command outputs a table with the following columns:

| Column | Description |
|--------|-------------|
| NAME | Service name |
| STATUS | Running status |
| PID | Process ID |
| UPTIME | How long the process has been running |
| COMMAND | The shell command being executed |
| DIRECTORY | Working directory of the process |

## Example

```bash
candle list-all
```

Example output:

```
NAME    STATUS    PID     UPTIME    COMMAND         DIRECTORY
api     running   12345   2h 15m    npm run dev     /project-a/api
web     running   12346   1h 30m    npm start       /project-b/web
worker  running   12347   45m       node worker.js  /project-c
```

## Comparison

| Command | Scope |
|---------|-------|
| `candle list` | Current project only |
| `candle list-all` | All projects globally |

## See Also

- [list](list) - List services in current project
- [kill-all](kill-all) - Kill all services globally
