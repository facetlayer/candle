I have enough to produce the spec.

# Technical Spec: `watch-wait` subsystem (candle)

This covers the `candle watch` and `candle wait-for-log` CLI commands and their supporting log-tailing infrastructure. Both poll a SQLite `process_output` table for new log rows; they differ in what they do when a new row appears.

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

**Critical:** `timestamp` is stored in **whole seconds** (`strftime('%s','now')`), but all wall-clock math in the code uses `Date.now()` (milliseconds). The one place this matters is the recency window — see §3.3.

### 1.2 `ProcessLog` row shape (`src/logs/processLogs.ts:13-20`)
`{ id: number, command_name: string, project_dir: string, content?: string, log_type: number, timestamp: number }`

### 1.3 `ProcessLogType` enum (`src/logs/ProcessLogType.ts`) — exact integer values
```
stdout                 = 1
stderr                 = 2
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
- If `afterLogId != null`: append ` and po.id > ?` (note: JS `!= null` excludes both `null` and `undefined`; `0` is a valid id and passes)
- Always append: ` order by po.timestamp desc, po.id desc`
- If `limit` set: ` limit ?`

So the DB returns **newest-first**. `getProcessLogs` (`processLogs.ts:74`) then does `logItems.reverse()` to hand back **chronological (oldest-first)** order. Every consumer assumes oldest-first.

`getProcessLogsWithEvictionInfo` also computes `logsWereEvicted` by re-running the query wrapped in `select count(*) as total from (<sql without limit>)` and comparing to the returned count — **not used** by watch/wait, can be skipped in a minimal port but `getProcessLogs` is the entry point both use.

Subtlety for Rust: ordering by `(timestamp desc, id desc)` then reversing is NOT the same as ordering by `id asc` when multiple rows share a timestamp (likely, since timestamps are 1-second granularity). Reproduce exactly: order desc by `(timestamp, id)`, then reverse the vector.

## 3. `LogIterator` (`src/logs/LogIterator.ts`)

Stateful cursor over `getProcessLogs`. Fields: `currentLogId: number | null` (starts `null`), `options: LogSearchOptions`.

- `peekNextLogs(partialOpts)`: merges `{...options, ...partial, afterLogId: this.currentLogId}` and calls `getProcessLogs`. When `currentLogId` is null, `afterLogId` is null → no `id >` filter → fetches the most recent `limit` rows.
- `getNextLogs(partialOpts)`: calls peek; if non-empty, sets `currentLogId = logs[last].id` (last = newest since chronological). Returns the batch.

Key behavior: because the cursor advances by **max id seen**, and the query filters `id > currentLogId`, each `getNextLogs` returns strictly new rows. The `limit` passed per-call caps batch size; `watchProcess` passes `limit: INITIAL_LOG_COUNT (100)` only on the first call and `{}` (no limit) afterward.

## 4. `LatestExecutionLogFilter` (`src/log-filters/LatestExecutionLogFilter.ts`)

Trims a log stream to "only the most recent launch, optionally within a recency window." Stateful across calls (the `recentCommandLaunch` map persists).

Options: `{ showPastLogsBehavior: 'show_logs_from_previous_launch' | 'only_show_after_recent_launch', recentWindowMs?: number }`.

### 4.1 `checkLatestLaunchStatus(logs)` (call once with the initial batch)
- Clears `recentCommandLaunch` map.
- If `recentWindowMs` set: `this.minTimestamp = (Date.now() - recentWindowMs) / 1000` — **converts ms→seconds to match DB timestamps** (`LatestExecutionLogFilter.ts:64`). Reproduce exactly in Rust: `(now_unix_millis - window_ms) as f64 / 1000.0`.
- Scans logs; for each `process_start_initiated` (type 3), records `recentCommandLaunch[command_name] = { startLogId: log.id }` (last one wins).

### 4.2 `filter(logs)` (call on every batch)
Per log, look up `status = recentCommandLaunch[command_name]`:
- If status exists: include iff `log.id >= status.startLogId && passesTimestampWindow(log)`.
- If no status:
  - If `log_type == process_start_initiated (3)`: record it as the launch (`startLogId = log.id`), include iff `passesTimestampWindow`.
  - Else if `showPastLogsBehavior == 'show_logs_from_previous_launch'`: include iff `passesTimestampWindow`.
  - Else (`only_show_after_recent_launch`): exclude.

`passesTimestampWindow(log)`: `true` if `minTimestamp === undefined`, else `log.timestamp >= this.minTimestamp` (`LatestExecutionLogFilter.ts:78-83`).

Subtlety: the map mutates inside `filter`, so a `process_start_initiated` in a later batch retroactively sets the launch point for that command for subsequent batches.

## 5. `ExecutionStatusTracker` (`src/log-filters/ExecutionStatusTracker.ts`)

Tracks per-command latest lifecycle event (only the 4 lifecycle types 3,4,5,6 update it). `apply(logs)` records `executionStatus[command_name] = { latestLifecycleEvent }` for the last lifecycle log seen. `countRunningProcesses()` counts distinct commands whose latest lifecycle event is `process_started (5)` OR `process_start_initiated (3)`. Used only by `watch` for the closing message.

## 6. Output formatting (`src/logs.ts`)

`consoleLogRow(row, { format, prefix })` (`logs.ts:63-85`) dispatches on `log_type`:
- `stdout (1)`: pretty → `console.log((prefix ?? '') + content)`; json → `console.log(JSON.stringify({ stdout: content }))`
- `stderr (2)`: pretty → `(prefix ?? '') + '[stderr] ' + content`; json → `{ stderr: content }`
- `process_exited (6)` / `process_start_failed (4)`: routed through `consoleLogSystemMessage` → pretty → `(prefix ?? '') + '[' + content + ']'`; json → `{ message: content }`
- `process_start_initiated (3)` / `process_started (5)`: **suppressed (printed nothing).**

`consoleLogSystemMessage(format, msg, prefix?)` (`logs.ts:58-61`): pretty → `(prefix ?? '') + '[' + msg + ']'`; json → `{ message: msg }`.

All output goes to **stdout** (`console.log`), even errors except where noted in §8. Each call emits one line via `console.log` (adds `\n`).

`enableAppNamePrefix` option exists but watch/wait don't use it (watch builds its own prefix).

## 7. `watch` command

### 7.1 CLI definition (`src/main-cli.ts:154-164`)
`watch [name...]` — positional `name` (variadic, strings). Hidden option `--exit-after-ms` (number, `hidden: true`). `.strictOptions()` (unknown flags rejected). Parsed as `argv['exit-after-ms']` (`main-cli.ts:283`).

### 7.2 Agent-mode disabling (`src/runContext.ts`, `main-cli.ts:434-441`)
`isRunByAgent = !!process.env.CLAUDECODE` (`runContext.ts:1`). In the `watch` case:
```js
if (isRunByAgent) {
  console.error("Error: 'watch' is not available in agent mode. Use 'candle logs' to view process output.");
  process.exit(1);
}
```
Exact stderr string and exit code 1. Also, `isRunByAgent` blanks the watch-related help lines (`main-cli.ts:42`) — cosmetic.

### 7.3 `handleWatch` (`src/watch-command.ts`)
1. `projectDir = findProjectDir()` (searches up for `.candle.json`; uses `process.cwd()`).
2. `commandNames = resolveCommandNamesOrAll(projectDir, options.commandNames)` — if none given, expands to all configured service names; throws `UsageError('No services configured in .candle.json')` if config empty (`configFile.ts:259`).
3. For each name: `startOneService({ projectDir, commandName: name, consoleOutputFormat: 'pretty', checkStart: true })` — `checkStart:true` is a no-op for already-running services. (Watch ensures services are up before tailing.)
4. Print header to stdout:
   - 1 name: `Watching process '<name>'`
   - N names: `Watching <N> processes:` then for each `  - '<name>'`
   - Then `Press Ctrl+C to stop watching.` and a blank line.
5. Call `watchProcess({ projectDir, commandNames, consoleOutputFormat: 'pretty', exitAfterMs })`.

### 7.4 `watchProcess` (`src/watchProcess.ts`) — the tail loop
Constants: `INITIAL_LOG_COUNT = 100`, `POLL_INTERVAL = 200` (ms), `RECENT_LOG_WINDOW_MS = 10_000`.

- `isBlendedMode = commandNames.length > 1`.
- `LogIterator({ projectDir, commandNames })`.
- `LogFilter = LatestExecutionLogFilter({ showPastLogsBehavior: 'show_logs_from_previous_launch' (default), recentWindowMs: 10_000 })`. (Watch passes no `showPastLogsBehavior`, so default applies; `run` uses `only_show_after_recent_launch`.)
- `initialLogs = logIterator.getNextLogs({ limit: 100 })`; then `logFilter.checkLatestLaunchStatus(initialLogs)`.

Subtle ordering bug-compatible behavior: `getNextLogs({limit:100})` is called **before** `checkLatestLaunchStatus`, and it already advances `currentLogId` to the newest of those 100. So the window cutoff is computed at that point.

- `--exit-after-ms`: if `exitAfterMs > 0`, a timer fires after that many ms, prints `consoleLogSystemMessage(format, 'Exiting watch mode after <exitAfterMs>ms timeout')` (pretty → `[Exiting watch mode after Nms timeout]`), and sets `watching = false`.
- Install `SIGINT`/`SIGTERM` handlers (`stopWatching`) that set `watching = false` and clear the timer.
- `printLogs(logs)`: `executionStatusTracker.apply(logs)`, then `logFilter.filter(logs)`, then per filtered log `consoleLogRow(log, { format, prefix })` where `prefix = isBlendedMode ? "[<command_name>] " : undefined`.
- Print initial logs once (`printLogs(initialLogs)`).
- Loop while `watching`: `printLogs(logIterator.getNextLogs({}))` (no limit), then `await sleep(200ms)`.
- After loop: clear timer; compute `runningProcesses = executionStatusTracker.countRunningProcesses()`:
  - `== 1`: `consoleLogSystemMessage(format, 'Stopped watching. Process is still running in the background.')`
  - `> 1`: `Stopped watching. <N> processes are still running in the background.`
  - `0`: nothing.
- Remove signal listeners.

Subtlety: the function returns normally (no `process.exit`); the process exits naturally. With `--exit-after-ms`, the only thing that ends the loop is `watching=false` on the next poll boundary, so actual stop latency is up to `POLL_INTERVAL` (200ms) after the timer fires.

## 8. `wait-for-log` command

### 8.1 CLI definition (`src/main-cli.ts:165-178`)
`wait-for-log [name]` — single positional `name`. Required option `--message <string>` (`demandOption: true`). Option `--timeout <number>` seconds, default `30`. `.strictOptions()`. **Not** disabled in agent mode.

Dispatch (`main-cli.ts:444-458`):
```js
const projectDir = findProjectDir();
const result = await handleWaitForLog({ projectDir, commandNames, message, timeoutMs: timeout * 1000 });
if (!result.success) process.exit(1);
```
So **exit code 0 on success, 1 on failure.** Timeout is converted seconds→ms here.

### 8.2 `handleWaitForLog` (`src/wait-for-log-command.ts`)
Constants: `POLL_INTERVAL = 200` (ms), `LOG_COUNT_SEARCH_LIMIT = 1000`. Default `timeoutMs = 30000` if unset.

Return shape: `{ success: boolean, message?: string }`. Caller only checks `success`.

Algorithm:
1. `LogIterator({ projectDir, commandNames, limit: 1000 })`. `allInitialLogs = logIterator.getNextLogs()` (uses limit 1000; advances cursor to newest).
2. `logFilter = LatestExecutionLogFilter({ showPastLogsBehavior: 'only_show_after_recent_launch' })` (**no recency window**). `logFilter.checkLatestLaunchStatus(allInitialLogs)`; `initialLogs = logFilter.filter(allInitialLogs)`.
3. If `initialLogs.length === 0`: return `{ success: false, message: 'Process has not started yet' }` (no console output; caller exits 1).
4. `hasProcessStarted = initialLogs.some(l => l.log_type === process_start_initiated (3))`. If false: `console.error('Process has not started yet')` (**stderr**) and return `{ success: false }`.
5. Scan `initialLogs`: if any `log.content?.includes(message)` (JS substring match; `content` may be undefined/null → skip): `console.log('Found message "<message>" in existing logs.')`, return `{ success: true }`.
6. Poll loop (`timeStarted = Date.now()`):
   - If `Date.now() - timeStarted > timeoutMs`: `console.log('wait-for-log failed: Timed out after <timeoutMs>ms and message "<message>" not found.')`, call `printRecentLogs(...)`, return `{ success: false }`.
   - `rawLogs = logIterator.getNextLogs()` (limit 1000); `logs = logFilter.filter(rawLogs)`.
   - For each log: if `content?.includes(message)` → `console.log('Found message "<message>" in logs.')`, return `{ success: true }`. Else if `log_type === process_exited (6)` → `console.log('wait-for-log failed: Process exited before finding message "<message>"')`, `printRecentLogs(...)`, return `{ success: false }`.
   - `await sleep(200ms)`.

Note the timeout is checked at the **top** of the loop before fetching; first check happens immediately (0 elapsed, won't trip). The exit-before-found check is per-log within a batch and is evaluated **after** the message check, so a batch where the matching line and the exit line both appear returns success if the match comes first in chronological order.

### 8.3 `printRecentLogs` (`wait-for-log-command.ts:17-29`)
Prints on failure paths:
- `console.log("Recent logs for '<commandNames.join(', ')>':")`
- New `LatestExecutionLogFilter({ showPastLogsBehavior: 'only_show_after_recent_launch' })` (fresh, no `checkLatestLaunchStatus` call — so `recentCommandLaunch` empty; filter discovers the launch inline).
- `getProcessLogs({ commandNames, limit: 100, projectDir })` → filter → `consoleLogRow(log, { format: 'pretty' })` each.

Subtlety: it calls `getProcessLogs` directly (not the iterator) and does not call `checkLatestLaunchStatus`, so its filtering relies entirely on `filter()` discovering the `process_start_initiated` row within the 100-row window.

### 8.4 Exact output strings (test-load-bearing)
- `Found message "<m>" in existing logs.` (stdout, success)
- `Found message "<m>" in logs.` (stdout, success)
- `wait-for-log failed: Timed out after <ms>ms and message "<m>" not found.` (stdout, failure)
- `wait-for-log failed: Process exited before finding message "<m>"` (stdout, failure)
- `Process has not started yet` (stderr, via `console.error`, when logs exist but none are starts)
- Note `<m>` is wrapped in literal double-quotes; `<ms>` is the raw timeout in **milliseconds**.

## 9. External npm dependencies → Rust crates

- **SQLite access** (`getDatabase()`, `db.list/get/run`) — TS uses `better-sqlite3` (synchronous). Rust: `rusqlite` (sync, matches the synchronous query model exactly) or `sqlx` (async). Prefer `rusqlite` to preserve the synchronous semantics.
- **yargs** CLI parsing (`main-cli.ts`) → `clap` (use a hidden arg for `--exit-after-ms`, e.g. `#[arg(hide = true)]`).
- **Timers/sleep** (`setTimeout`/`setInterval`) → `tokio::time::sleep` (async) or `std::thread::sleep` (sync loop). The 200ms poll and the exit-after-ms timer are the only timing primitives.
- **Signals** (`process.on('SIGINT'/'SIGTERM')`) → `tokio::signal` or `ctrlc` crate. Must flip a shared `watching` flag (e.g. `AtomicBool` / `tokio::select!`).
- No other third-party deps in this subsystem.

## 10. Platform / correctness gotchas for Rust

1. **Timestamp unit mismatch:** DB `timestamp` is UNIX **seconds**; `Date.now()` is **ms**. The recency window divides ms by 1000 (`/1000`, keeps fractional) and compares against integer seconds. Replicate the float division; do not round.
2. **Order-then-reverse:** must reproduce `ORDER BY timestamp DESC, id DESC` then reverse, not a plain `id ASC`, because second-granularity timestamps tie frequently.
3. **`afterLogId != null`** semantics: `null`/`undefined` → no filter; `0` is valid and applies `id > 0`. Use `Option<i64>` with `Some(0)` distinct from `None`.
4. **`content` is nullable**; `content?.includes(message)` skips null. In Rust, `Option<String>` → match before substring search. Substring match is plain `str::contains` (case-sensitive, no regex).
5. **Stateful filter mutation:** `LatestExecutionLogFilter.filter` mutates `recentCommandLaunch`; the same instance is reused across poll iterations in both commands. Keep it as a mutable struct, not a pure function.
6. **Exit codes:** `wait-for-log` → `exit(1)` on `!success`, `0` otherwise. `watch` in agent mode → `exit(1)` with the exact stderr message; otherwise exits 0 naturally.
7. **stdout vs stderr:** nearly everything is stdout (`console.log`). Exceptions: the agent-mode watch error and the "Process has not started yet" (step 4) use `console.error` → **stderr**. Note step 3's "Process has not started yet" is only a return-value `message` field, never printed.
8. **Cursor pre-advance in watch:** `getNextLogs({limit:100})` advances the cursor before `checkLatestLaunchStatus`; the initial 100 are both the status-seed and the first printed batch (`printLogs(initialLogs)`), then the loop continues from the new cursor. Don't double-fetch.
9. **`isRunByAgent` is evaluated once** from `process.env.CLAUDECODE` (truthy = any non-empty value). In Rust: `std::env::var("CLAUDECODE").map(|v| !v.is_empty()).unwrap_or(false)` — but note JS `!!` treats the empty string as false and any non-empty string as true; `"0"`/`"false"` are still truthy. Match that: presence of a non-empty value = agent mode.

## 11. Rust reimplementation notes (modules / functions, dependency order)

Build bottom-up:

1. `process_log_type` — const i64s (1–6). No deps.
2. `db` — `rusqlite` connection accessor; `process_output` schema + indexes; `get_process_logs(opts) -> Vec<ProcessLog>` (and optional `..._with_eviction_info`).
3. `build_log_search_query(opts) -> (String, Vec<Value>)` — exact SQL from §2. Depends on (1).
4. `ProcessLog` struct + `LogSearchOptions` struct. Depends on (1).
5. `log_iterator::LogIterator` — `current_log_id: Option<i64>`, `peek_next_logs`, `get_next_logs`. Depends on (2,3,4).
6. `latest_execution_log_filter::LatestExecutionLogFilter` — map + `min_timestamp: Option<f64>`; `check_latest_launch_status`, `filter`, `passes_timestamp_window`. Depends on (1,4).
7. `execution_status_tracker::ExecutionStatusTracker` — `apply`, `count_running_processes`. Depends on (1,4). (watch only)
8. `console_log` — `console_log_row`, `console_log_system_message`, stdout/stderr + pretty/json formatting. Depends on (1,4).
9. `watch_process(opts)` — the tail loop, signals, exit-after-ms timer, closing message. Depends on (5,6,7,8).
10. `handle_watch(opts)` — resolve names, ensure-started (calls into `start_one_service`, out of scope here), header print, calls (9). Depends on config/start modules + (9).
11. `handle_wait_for_log(opts) -> WaitResult{ success, message }` + `print_recent_logs`. Depends on (2,5,6,8). Returns result; CLI layer maps to exit code.
12. CLI wiring (`clap`): `watch [name...] --exit-after-ms(hidden)`, `wait-for-log [name] --message(required) --timeout(=30s)`; agent-mode guard for watch; `timeout*1000`; `exit(1)` on wait failure. Depends on (10,11) + `run_context::is_run_by_agent`.

Constants to centralize: watch `INITIAL_LOG_COUNT=100`, `POLL_INTERVAL=200ms`, `RECENT_LOG_WINDOW_MS=10_000`; wait `POLL_INTERVAL=200ms`, `LOG_COUNT_SEARCH_LIMIT=1000`, default `timeout=30s`.

Relevant source files: `/Users/andy/candle/src/watch-command.ts`, `/Users/andy/candle/src/watchProcess.ts`, `/Users/andy/candle/src/wait-for-log-command.ts`, `/Users/andy/candle/src/logs/LogIterator.ts`, `/Users/andy/candle/src/log-filters/LatestExecutionLogFilter.ts`, `/Users/andy/candle/src/log-filters/ExecutionStatusTracker.ts`, `/Users/andy/candle/src/logs/processLogs.ts`, `/Users/andy/candle/src/logs/buildLogSearchQuery.ts`, `/Users/andy/candle/src/logs/SqlBuilder.ts`, `/Users/andy/candle/src/logs/ProcessLogType.ts`, `/Users/andy/candle/src/logs.ts`, `/Users/andy/candle/src/runContext.ts`, `/Users/andy/candle/src/main-cli.ts`, `/Users/andy/candle/src/database/database.ts`, `/Users/andy/candle/src/configFile.ts`.