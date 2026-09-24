# find-orphans

List running services whose project no longer accounts for them.

## Syntax

```bash
candle find-orphans [--json]
```

## Description

Candle records the project directory each service was started from, and `candle list` finds your services by looking for that directory from your current one. If the project changes underneath a running service, that link breaks — the process keeps running, but there is no longer a directory you can `cd` into to manage it.

`find-orphans` scans every service Candle is tracking across the whole system and reports the ones in that state. Like [kill-all](kill-all), it is a system-wide maintenance command rather than a project one.

A running service is orphaned when any of these is true:

- **The project directory no longer exists** — the folder was deleted or moved.
- **The config file is gone** — the directory is still there, but no longer contains a `.candle.json`.
- **The service is no longer in the config** — the config file is there, but the service was removed from it.

A [transient service](start#transient-services) (started with `--shell`) was never in the config, so only the first two apply to it.

Only live processes are reported. Entries for processes that have already exited are ordinary stale bookkeeping, which Candle clears on its own.

A config file that exists but cannot be parsed is not treated as orphaning, so a JSON typo never causes a healthy service to be reported.

## Options

- `--json` - Output as JSON.

## Cleaning up

Orphans are killed with [kill](kill) and its `--project-dir` option, which accepts a project directory that no longer exists:

```bash
candle kill --project-dir /path/to/old-project api
```

## Examples

### Find orphaned services

```bash
candle find-orphans
```

```
Found 1 orphaned process:

api (PID 40231)
  Project:  /home/me/old-project
  Orphaned: project directory no longer exists

Clean up with: candle kill --project-dir <project> <service>
```

### When nothing is orphaned

```bash
candle find-orphans
```

```
No orphaned processes found
```

### JSON output

```bash
candle find-orphans --json
```

```json
{
  "orphans": [
    {
      "serviceName": "api",
      "projectDir": "/home/me/old-project",
      "pid": 40231,
      "reason": "missingProjectDir"
    }
  ]
}
```

The `reason` field is one of `missingProjectDir`, `missingConfigFile`, or `serviceNotInConfig`.

## See Also

- [kill](kill) - Kill services, including in a project that is gone
- [kill-all](kill-all) - Kill all services globally
- [list-all](list-all) - List all services globally
