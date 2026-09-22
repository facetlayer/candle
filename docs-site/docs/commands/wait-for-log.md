# wait-for-log

Wait for a specific log message to appear.

This is a convenience command that is really helpful for CI based build scripts. You can launch a service in CI and then use `wait-for-log` to block the job until the service has fully started up.

The command is smart about checking for the most recent service launch, and not triggering from
a log that happened on a previous launch. If you use `candle wait-for-log` immediately after `candle start` then it will do the right thing.

## Syntax

```bash
candle wait-for-log [name] --message <message> [--timeout <seconds>]
```

## Arguments

- `name` - Name of the service to monitor. If omitted, the logs of every service in the project are searched.

## Options

- `--message <string>` - The exact log message substring to wait for (required)
- `--timeout <number>` - Timeout in seconds (default: 30)
- `--project-dir <dir>` - Act on the given project instead of the current directory. See [Targeting another project](../project-dir).

## Exit Status

Exits with status 0 once the message is found. Exits with status 1 if the timeout expires, if the process exits before the message appears, or if the service has no logs.

## Examples

### Wait for a server to be ready

```bash
candle start api
candle wait-for-log api --message "Server listening on port 3000"
npm test
```

### With custom timeout

```bash
candle wait-for-log api --message "Ready" --timeout 60
```

### In a CI pipeline

```bash
#!/bin/bash
candle start api
candle wait-for-log api --message "Server ready" --timeout 120
if [ $? -eq 0 ]; then
  npm run integration-tests
else
  echo "Server failed to start"
  exit 1
fi
```

## See Also

- [logs](logs) - View recent logs
- [start](start) - Start a service
- [watch](watch) - Watch live output
