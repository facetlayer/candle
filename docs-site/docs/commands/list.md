# list / ls

List active processes for the current project directory.

## Syntax

```bash
candle list
candle ls
```

## Description

The `list` command displays all running services that were started from the current project directory. The `ls` command is an alias with identical behavior.

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

## Examples

### List services in current project

```bash
candle list
```

Example output:

```
NAME    STATUS    PID     UPTIME    COMMAND         DIRECTORY
api     running   12345   2h 15m    npm run dev     /project/api
web     running   12346   2h 15m    npm start       /project/web
```

### Using the ls alias

```bash
candle ls
```

## Behavior

- Only shows services started from the current project directory
- Services started from other directories are not shown
- Use `candle list-all` to see all services globally

## See Also

- [list-all](list-all) - List all services globally
- [logs](logs) - View logs from services
- [watch](watch) - Watch live output
