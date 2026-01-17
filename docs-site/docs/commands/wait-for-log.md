# wait-for-log

Wait for a specific log message to appear.

## Syntax

```bash
candle wait-for-log <name> --message <message> [--timeout <seconds>]
```

## Description

The `wait-for-log` command polls the service logs until a specific message appears. This is particularly useful in CI/CD pipelines to wait for a server to be ready before running tests.

## Arguments

- `name` - Name of the service to monitor (required)

## Options

- `--message <string>` - The exact log message substring to wait for (required)
- `--timeout <number>` - Timeout in seconds (default: 30)

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

## Behavior

1. Checks existing logs for the message immediately
2. If not found, polls for new log entries
3. Returns success (exit code 0) when the message appears
4. Returns failure (exit code 1) if timeout is reached

The command handles race conditions intelligently - it checks for the message in existing logs first, so it works even if the message appeared before the command started.

## Exit Codes

- `0` - Message found in logs
- `1` - Timeout reached or service not found

## See Also

- [logs](logs) - View recent logs
- [start](start) - Start a service
- [watch](watch) - Watch live output
