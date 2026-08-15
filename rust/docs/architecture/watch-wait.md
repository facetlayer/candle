# Watch & wait-for-log

This covers the `candle watch` and `candle wait-for-log` CLI commands and their supporting log-tailing infrastructure. Both poll a SQLite `process_output` table for new log rows; they differ in what they do when a new row appears.

The Rust implementation lives in `rust/src/commands/{watch,wait_for_log}.rs`, with the shared tailing machinery in `rust/src/logs/log_iterator.rs`, `rust/src/logs/process_logs.rs`, `rust/src/logs/console_log.rs`, and the filters in `rust/src/log_filters/`. It mirrors the original Node implementation under `src/`, whose files are cross-referenced below as the historical source of truth.

## 1. Shared data model

### 1.1 `process_output` table (SQLite)
Defined in `src/database/database.ts:28-35`:

```sql
create table process_output(
    id integer primary key autoincrement,
    command_name text not null,
    project_dir text not null,
    content text,                                  -- nullable
    log_type integer not null,
    timestamp integer not null default (strftime('%s', 'now'))   -- UNIX SECONDS, not ms
)
```
Index used for tailing (`database.ts:49`):
`create index idx_process_output_lookup on process_output(project_dir, command_name, timestamp desc, id desc)`.

**Critical:** `timestamp` is stored in **whole seconds** (`strftime('%s','now')`), but all wall-clock math uses millisecond clocks. The one place this matters is the recency window — see §3.3.

### 1.2 `ProcessLog` row shape (`src/logs/processLogs.ts:13-20`)
`{ id: number, command_name: string, project_dir: string, content?: string, log_type: number, timestamp: number }`

### 1.3 `ProcessLogType` enum (`src/logs/ProcessLogType.ts`, Rust `logs/log_type.rs`) — exact integer values
```
stdout                  = 1
stderr                  = 2
process_start_initiated = 3   // written the moment a launch begins
process_start_failed    = 4
process_started         = 5   // subprocess successfully started
process_exited          = 6
```

## 2. The DB query: `buildLogSearchQuery` (`src/logs/buildLogSearchQuery.ts`)

`LogSearchOptions = { projectDir, commandNames?, limit?, sinceTimestamp?, afterLogId? }`.

Query construction (note the table alias `po`):
- 1 command name: `select po.* from process_output po where po.project_dir = ? and po.command_name = ?`
- N command names: `... and po.command_name in (?, ?, …)`
- No command names (defined projectDir): `... where po.project_dir = ?`
- If `sinceTimestamp` set: append ` and po.timestamp > ?`
- If `afterLogId != null`: append ` and po.id > ?` (the original JS `!= null` excludes both `null` and `undefined`; `0` is a valid id and passes — the Rust side models this as `Option<i64>` where `Some(0)` applies the filter and `None` does not)
- Always append: ` order by po.timestamp desc, po.id desc`
- If `limit` set: ` limit ?`

So the DB returns **newest-first**. `getProcessLogs` (`processLogs.ts:74`, Rust `logs/process_logs.rs`) then reverses the rows to hand back **chronological (oldest-first)** order. Every consumer assumes oldest-first.

`getProcessLogsWithEvictionInfo` also computes `logsWereEvicted` by re-running the query wrapped in `select count(*) as total from (<sql without limit>)` and comparing to the returned count. It is **not used** by watch/wait; `getProcessLogs` is the entry point both use.

Subtle ordering detail: ordering by `(timestamp desc, id desc)` then reversing is NOT the same as ordering by `id asc` when multiple rows share a timestamp (likely, since timestamps are 1-second granularity). The implementation orders descending by `(timestamp, id)`, then reverses the vector.

## 3. `LogIterator` (`src/logs/LogIterator.ts`, Rust `logs/log_iterator.rs`)

Stateful cursor over `getProcessLogs`. Fields: `currentLogId: Option<i64>` (starts `None`), plus the `LogSearchOptions`.

- `peekNextLogs(partialOpts)`: merges `{...options, ...partial, afterLogId: currentLogId}` and calls `getProcessLogs`. When `currentLogId` is `None`, `afterLogId` is null → no `id >` filter → fetches the most recent `limit` rows.
- `getNextLogs(partialOpts)`: calls peek; if non-empty, sets `currentLogId = logs[last].id` (last = newest since chronological). Returns the batch.

Key behavior: because the cursor advances by **max id seen**, and the query filters `id > currentLogId`, each `getNextLogs` returns strictly new rows. The `limit` passed per-call caps batch size; `watchProcess` passes `limit: INITIAL_LOG_COUNT (100)` only on the first call and no limit afterward.

## 4. `LatestExecutionLogFilter` (`src/log-filters/LatestExecutionLogFilter.ts`, Rust `log_filters/latest_execution_log_filter.rs`)

Trims a log stream to "only the most recent launch, optionally within a recency window." Stateful across calls (the `recentCommandLaunch` map persists).

Options: `{ showPastLogsBehavior: 'show_logs_from_previous_launch' | 'only_show_after_recent_launch', recentWindowMs?: number }`.

### 4.1 `checkLatestLaunchStatus(logs)` (called once with the initial batch)
- Clears `recentCommandLaunch` map.
- If `recentWindowMs` set: `minTimestamp = (now_unix_millis - recentWindowMs) / 1000` — **converts ms→seconds to match DB timestamps** (`LatestExecutionLogFilter.ts:64`). The Rust code keeps the float division (`(now_unix_millis - window_ms) as f64 / 1000.0`) and does not round.
- Scans logs; for each `process_start_initiated` (type 3), records `recentCommandLaunch[command_name] = { startLogId: log.id, reportedStartResult: false }` (last one wins). A later `process_started (5)` / `process_start_failed (4)` for that command sets `reportedStartResult = true`.

### 4.2 `filter(logs)` (called on every batch)
Per log, first advance the launch boundary: a `process_start_initiated (3)` whose id is **greater** than the recorded `startLogId` (or with no record yet) becomes the new launch. The greater-than check matters because `filter` is normally handed the same batch `checkLatestLaunchStatus` just analyzed — without it, replaying an older launch event would move the boundary backwards and let that launch's stale output through. Then look up `status = recentCommandLaunch[command_name]`:
- If status exists:
  - If `log_type == process_exited (6)` and `!status.reportedStartResult`: exclude. A monitor always writes `process_started` before `process_exited`, so an exit arriving before this launch's own start result came from the instance that was just killed — its shutdown can outlive the new launch row.
  - Else include iff `log.id >= status.startLogId && passesTimestampWindow(log)`.
- If no status:
  - If `showPastLogsBehavior == 'show_logs_from_previous_launch'`: include iff `passesTimestampWindow`.
  - Else (`only_show_after_recent_launch`): exclude.

`passesTimestampWindow(log)`: `true` if `minTimestamp` is unset, else `log.timestamp >= minTimestamp` (`LatestExecutionLogFilter.ts:78-83`).

Subtlety: the map mutates inside `filter`, so a `process_start_initiated` in a later batch retroactively sets the launch point for that command for subsequent batches. The filter is a mutable struct reused across poll iterations, not a pure function.

## 5. `ExecutionStatusTracker` (`src/log-filters/ExecutionStatusTracker.ts`, Rust `log_filters/execution_status_tracker.rs`)

Tracks per-command latest lifecycle event (only the 4 lifecycle types 3,4,5,6 update it). `apply(logs)` records `executionStatus[command_name] = { latestLifecycleEvent }` for the last lifecycle log seen. `countRunningProcesses()` counts distinct commands whose latest lifecycle event is `process_started (5)` OR `process_start_initiated (3)`. Used only by `watch` for the closing message.

## 6. Output formatting (`src/logs.ts`, Rust `logs/console_log.rs`)

`consoleLogRow(row, { format, prefix })` (`logs.ts:63-85`) dispatches on `log_type`:
- `stdout (1)`: pretty → `(prefix ?? '') + content`; json → `{ stdout: content }`
- `stderr (2)`: pretty → `(prefix ?? '') + '[stderr] ' + content`; json → `{ stderr: content }`
- `process_exited (6)` / `process_start_failed (4)`: routed through `consoleLogSystemMessage` → pretty → `(prefix ?? '') + '[' + content + ']'`; json → `{ message: content }`
- `process_start_initiated (3)` / `process_started (5)`: **suppressed (nothing printed).**

`consoleLogSystemMessage(format, msg, prefix?)` (`logs.ts:58-61`): pretty → `(prefix ?? '') + '[' + msg + ']'`; json → `{ message: msg }`.

All output goes to **stdout**, even errors except where noted in §8. Each call emits one line (adds `\n`).

`enableAppNamePrefix` option exists but watch/wait don't use it (watch builds its own prefix).

## 7. `watch` command (`rust/src/commands/watch.rs`)

### 7.1 CLI definition (`src/main-cli.ts:154-164`)
`watch [name...]` — positional `name` (variadic, strings). Hidden option `--exit-after-ms` (number, marked hidden — `#[arg(hide = true)]` in the clap definition). Unknown flags are rejected (strict options).

### 7.2 Agent-mode disabling (`src/runContext.ts`, `main-cli.ts:434-441`)
`is_run_by_agent` is derived from the coding-agent marker environment variables (see `run_context`). In the `watch` case, when run by an agent the command prints to **stderr** and exits with code **1**:
```
Error: 'watch' is not available in agent mode. Use 'candle logs' to view process output.
```
That exact stderr string and exit code 1 are preserved. Agent mode also blanks the watch-related help lines (`main-cli.ts:42`) — cosmetic.

`is_run_by_agent` is evaluated once: agent mode iff **any** of `CLAUDECODE` / `GEMINI_CLI` / `CURSOR_AGENT` is present and non-empty. The empty string means not-set; `"0"`/`"false"` are non-empty and therefore still count.

### 7.3 `handleWatch` (`src/watch-command.ts`)
1. `projectDir = findProjectDir()` (searches up for `.candle.json`; uses the current working directory).
2. `commandNames = resolveCommandNamesOrAll(projectDir, options.commandNames)` — if none given, expands to all configured service names; throws `UsageError('No services configured in .candle.json')` if config empty (`configFile.ts:259`).
3. For each name: `startOneService({ projectDir, commandName: name, consoleOutputFormat: 'pretty', checkStart: true })` — `checkStart:true` is a no-op for already-running services. (Watch ensures services are up before tailing.)
4. Print header to stdout:
   - 1 name: `Watching process '<name>'`
   - N names: `Watching <N> processes:` then for each `  - '<name>'`
   - Then `Press Ctrl+C to stop watching.` and a blank line.
5. Call `watchProcess({ projectDir, commandNames, consoleOutputFormat: 'pretty', exitAfterMs })`.

### 7.4 `watchProcess` — the tail loop
Constants: `INITIAL_LOG_COUNT = 100`, `POLL_INTERVAL = 200` (ms), `RECENT_LOG_WINDOW_MS = 10_000`.

- `isBlendedMode = commandNames.length > 1`.
- `LogIterator({ projectDir, commandNames })`.
- `LogFilter = LatestExecutionLogFilter({ showPastLogsBehavior: 'show_logs_from_previous_launch' (default), recentWindowMs: 10_000 })`. (Watch passes no `showPastLogsBehavior`, so the default applies; `run` uses `only_show_after_recent_launch`.)
- `initialLogs = logIterator.getNextLogs({ limit: 100 })`; then `logFilter.checkLatestLaunchStatus(initialLogs)`.

Ordering note: `getNextLogs({limit:100})` is called **before** `checkLatestLaunchStatus`, and it already advances `currentLogId` to the newest of those 100. So the window cutoff is computed at that point, and the initial 100 are both the status-seed and the first printed batch — there is no double-fetch.

- `--exit-after-ms`: if `exitAfterMs > 0`, a timer fires after that many ms, prints `consoleLogSystemMessage(format, 'Exiting watch mode after <exitAfterMs>ms timeout')` (pretty → `[Exiting watch mode after Nms timeout]`), and sets `watching = false`.
- Install `SIGINT`/`SIGTERM` handlers (`stopWatching`) that set `watching = false` and clear the timer.
- `printLogs(logs)`: `executionStatusTracker.apply(logs)`, then `logFilter.filter(logs)`, then per filtered log `consoleLogRow(log, { format, prefix })` where `prefix = isBlendedMode ? "[<command_name>] " : undefined`.
- Print initial logs once (`printLogs(initialLogs)`).
- Loop while `watching`: `printLogs(logIterator.getNextLogs({}))` (no limit), then sleep 200ms.
- After loop: clear timer; compute `runningProcesses = executionStatusTracker.countRunningProcesses()`:
  - `== 1`: `consoleLogSystemMessage(format, 'Stopped watching. Process is still running in the background.')`
  - `> 1`: `Stopped watching. <N> processes are still running in the background.`
  - `0`: nothing.
- Remove signal listeners.

Subtlety: the function returns normally (no forced exit); the process exits naturally. With `--exit-after-ms`, the only thing that ends the loop is `watching=false` on the next poll boundary, so actual stop latency is up to `POLL_INTERVAL` (200ms) after the timer fires.

## 8. `wait-for-log` command (`rust/src/commands/wait_for_log.rs`)

### 8.1 CLI definition (`src/main-cli.ts:165-178`)
`wait-for-log [name]` — single positional `name`. Required option `--message <string>`. Option `--timeout <number>` in seconds, default `30`. Strict options. **Not** disabled in agent mode.

Dispatch (`main-cli.ts:444-458`): resolve `projectDir`, call `handleWaitForLog({ projectDir, commandNames, message, timeoutMs: timeout * 1000 })`, and exit `1` if `!result.success`. So **exit code 0 on success, 1 on failure.** Timeout is converted seconds→ms here.

### 8.2 `handleWaitForLog` (`src/wait-for-log-command.ts`)
Constants: `POLL_INTERVAL = 200` (ms), `LOG_COUNT_SEARCH_LIMIT = 1000`. Default `timeoutMs = 30000` if unset.

Return shape: `{ success: boolean, message?: string }`. Caller only checks `success`.

Algorithm:
1. `LogIterator({ projectDir, commandNames, limit: 1000 })`. `allInitialLogs = logIterator.getNextLogs()` (uses limit 1000; advances cursor to newest).
2. `logFilter = LatestExecutionLogFilter({ showPastLogsBehavior: 'only_show_after_recent_launch' })` (**no recency window**). `logFilter.checkLatestLaunchStatus(allInitialLogs)`; `initialLogs = logFilter.filter(allInitialLogs)`.
3. If `initialLogs.length === 0`: return `{ success: false, message: 'Process has not started yet' }` (no console output; caller exits 1).
4. `hasProcessStarted = initialLogs.some(l => l.log_type === process_start_initiated (3))`. If false: print `Process has not started yet` to **stderr** and return `{ success: false }`.
5. Scan `initialLogs`: if any `log.content?.includes(message)` (substring match; `content` may be null → skipped): print `Found message "<message>" in existing logs.` and return `{ success: true }`.
6. Poll loop (`timeStarted = now`):
   - If `now - timeStarted > timeoutMs`: print `wait-for-log failed: Timed out after <timeoutMs>ms and message "<message>" not found.`, call `printRecentLogs(...)`, return `{ success: false }`.
   - `rawLogs = logIterator.getNextLogs()` (limit 1000); `logs = logFilter.filter(rawLogs)`.
   - For each log: if `content?.includes(message)` → print `Found message "<message>" in logs.` and return `{ success: true }`. Else if `log_type === process_exited (6)` → print `wait-for-log failed: Process exited before finding message "<message>"`, call `printRecentLogs(...)`, return `{ success: false }`.
   - Sleep 200ms.

The timeout is checked at the **top** of the loop before fetching; the first check happens immediately (0 elapsed, won't trip). The exit-before-found check is per-log within a batch and is evaluated **after** the message check, so a batch where the matching line and the exit line both appear returns success if the match comes first in chronological order.

### 8.3 `printRecentLogs` (`wait-for-log-command.ts:17-29`)
Prints on failure paths:
- `Recent logs for '<commandNames.join(', ')>':`
- New `LatestExecutionLogFilter({ showPastLogsBehavior: 'only_show_after_recent_launch' })` (fresh, no `checkLatestLaunchStatus` call — so `recentCommandLaunch` is empty; `filter()` discovers the launch inline).
- `getProcessLogs({ commandNames, limit: 100, projectDir })` → filter → `consoleLogRow(log, { format: 'pretty' })` each.

Subtlety: it calls `getProcessLogs` directly (not the iterator) and does not call `checkLatestLaunchStatus`, so its filtering relies entirely on `filter()` discovering the `process_start_initiated` row within the 100-row window.

### 8.4 Exact output strings (test-load-bearing)
- `Found message "<m>" in existing logs.` (stdout, success)
- `Found message "<m>" in logs.` (stdout, success)
- `wait-for-log failed: Timed out after <ms>ms and message "<m>" not found.` (stdout, failure)
- `wait-for-log failed: Process exited before finding message "<m>"` (stdout, failure)
- `Process has not started yet` (stderr, when logs exist but none are starts)
- `<m>` is wrapped in literal double-quotes; `<ms>` is the raw timeout in **milliseconds**.

## 9. Implementation dependencies

- **SQLite access** (`getDatabase()`, list/get/run): the Rust implementation uses `rusqlite` (synchronous), which preserves the synchronous query semantics of the original `better-sqlite3` code exactly.
- **CLI parsing**: `clap`, with `--exit-after-ms` declared as a hidden arg (`#[arg(hide = true)]`).
- **Timers/sleep**: the 200ms poll and the exit-after-ms timer are the only timing primitives.
- **Signals** (`SIGINT`/`SIGTERM`): flip a shared `watching` flag.
- No other third-party dependencies in this subsystem.

## 10. Platform / correctness gotchas

1. **Timestamp unit mismatch:** DB `timestamp` is UNIX **seconds**; wall-clock math is **ms**. The recency window divides ms by 1000 (keeping the fraction) and compares against integer seconds. The float division is reproduced without rounding.
2. **Order-then-reverse:** `ORDER BY timestamp DESC, id DESC` then reverse, not a plain `id ASC`, because second-granularity timestamps tie frequently.
3. **`afterLogId` semantics:** absent → no filter; `0` is valid and applies `id > 0`. Modeled as `Option<i64>` with `Some(0)` distinct from `None`.
4. **`content` is nullable;** a null `content` skips the substring search. Substring match is plain `str::contains` (case-sensitive, no regex).
5. **Stateful filter mutation:** `LatestExecutionLogFilter.filter` mutates `recentCommandLaunch`; the same instance is reused across poll iterations in both commands. It is a mutable struct, not a pure function.
6. **Exit codes:** `wait-for-log` → exit `1` on `!success`, `0` otherwise. `watch` in agent mode → exit `1` with the exact stderr message; otherwise exits 0 naturally.
7. **stdout vs stderr:** nearly everything is stdout. Exceptions: the agent-mode watch error and the "Process has not started yet" (step 4) go to **stderr**. Step 3's "Process has not started yet" is only a return-value `message` field, never printed.
8. **Cursor pre-advance in watch:** `getNextLogs({limit:100})` advances the cursor before `checkLatestLaunchStatus`; the initial 100 are both the status-seed and the first printed batch (`printLogs(initialLogs)`), then the loop continues from the new cursor — no double-fetch.
9. **`is_run_by_agent` is evaluated once** from the agent marker vars (`CLAUDECODE` / `GEMINI_CLI` / `CURSOR_AGENT`): any one present and non-empty = agent mode. `"0"`/`"false"` are non-empty and therefore still count; only unset or empty is non-agent.

## 11. Source files

Rust modules: `rust/src/commands/watch.rs`, `rust/src/commands/wait_for_log.rs`, `rust/src/logs/log_iterator.rs`, `rust/src/logs/process_logs.rs`, `rust/src/logs/console_log.rs`, `rust/src/logs/log_type.rs`, `rust/src/log_filters/latest_execution_log_filter.rs`, `rust/src/log_filters/execution_status_tracker.rs`.

Historical Node source of truth: `src/watch-command.ts`, `src/watchProcess.ts`, `src/wait-for-log-command.ts`, `src/logs/LogIterator.ts`, `src/log-filters/LatestExecutionLogFilter.ts`, `src/log-filters/ExecutionStatusTracker.ts`, `src/logs/processLogs.ts`, `src/logs/buildLogSearchQuery.ts`, `src/logs/SqlBuilder.ts`, `src/logs/ProcessLogType.ts`, `src/logs.ts`, `src/runContext.ts`, `src/main-cli.ts`, `src/database/database.ts`, `src/configFile.ts`.
