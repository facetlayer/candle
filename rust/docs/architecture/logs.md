# Logs subsystem

The Rust logs subsystem lives under `rust/src/logs/` (`log_type.rs`, `process_logs.rs`, `log_iterator.rs`, `console_log.rs`), `rust/src/log_filters/` (`latest_execution_log_filter.rs`, `execution_status_tracker.rs`), and the CLI handlers `rust/src/commands/logs.rs` and `rust/src/commands/clear_logs.rs`. It mirrors the original Node implementation under `src/`; those `src/...` references below are kept as source-of-truth links.

## 1. Storage model

All logs live in a single SQLite table `process_output`. Schema (mirrors `src/database/database.ts:28-35`):

```sql
create table process_output(
    id integer primary key autoincrement,
    command_name text not null,
    project_dir text not null,
    content text,                 -- nullable
    log_type integer not null,
    timestamp integer not null default (strftime('%s', 'now'))
)
```

Indexes (`database.ts:47-49`):
```sql
create index idx_process_output_command_name on process_output(command_name);
create index idx_process_output_project_dir on process_output(project_dir);
create index idx_process_output_lookup on process_output(project_dir, command_name, timestamp desc, id desc);
```

Critical points:
- `timestamp` is **whole Unix seconds** (`strftime('%s','now')`), NOT milliseconds. Mixing with `Date.now()` (ms) is a recurring foot-gun — see the `/1000` conversion in the filter (§7).
- `content` is nullable. Lifecycle events (types 3 and 5) typically have no content; stdout/stderr always have content. `console_log_row` passes `row.content` even for `process_exited`/`process_start_failed`, so content for those carries a human message.
- `id` is a monotonically increasing autoincrement integer and is the canonical ordering tiebreaker / cursor.

## 2. ProcessLogType enum (exact integers)

`rust/src/logs/log_type.rs` (mirrors `src/logs/ProcessLogType.ts`):

| Name | Value |
|------|-------|
| `stdout` | `1` |
| `stderr` | `2` |
| `process_start_initiated` | `3` (saved immediately when launching begins) |
| `process_start_failed` | `4` |
| `process_started` | `5` (subprocess successfully started) |
| `process_exited` | `6` |

These integers are persisted in the DB and MUST be kept stable. The Rust enum is `#[repr(i64)]` with explicit discriminants, plus a `TryFrom<i64>`.

The four "lifecycle" event types are `{3,4,5,6}` (mirrors `ExecutionStatusTracker.ts:12-17`). Note: `process_start_initiated` (3) is the one used as the "launch boundary" by `LatestExecutionLogFilter`, NOT `process_started` (5).

## 3. Data types

In `rust/src/logs/process_logs.rs`:

`NewProcessLog` (insert): `command_name: string`, `project_dir: string`, `content?: string`, `log_type: number`.
`ProcessLog` (row): `id`, `command_name`, `project_dir`, `content?`, `log_type`, `timestamp` (all as above).

`LogSearchOptions` (mirrors `processLogs.ts:22-31`):
- `projectDir: string` (primary)
- `commandNames?: string[]` — empty/undefined ⇒ all commands in the project
- `limit?: number`
- `sinceTimestamp?: number` (seconds)
- `afterLogId?: number`

Insert (mirrors `processLogs.ts:33-39`):
```sql
insert into process_output(command_name, project_dir, content, log_type) values(?, ?, ?, ?)
```
(`timestamp` and `id` use DB defaults.)

## 4. Query accumulator

The original `src/logs/SqlBuilder.ts` is a trivial accumulator: `add(sqlFragment, params[])` appends a string to `sql` and pushes params; `getSql()`/`getParams()` return them. No spacing/escaping logic. Fragments are concatenated **verbatim**, so leading spaces in fragments matter (e.g. `' and po.timestamp > ?'`). The Rust implementation uses the equivalent `String` + `Vec<rusqlite::types::Value>` pair, built directly inside `build_log_search_query` in `rust/src/logs/process_logs.rs`.

## 5. build_log_search_query — exact SQL

`build_log_search_query` in `rust/src/logs/process_logs.rs` (mirrors `src/logs/buildLogSearchQuery.ts`). Base SELECT is always `select po.* from process_output po`. Branch logic (`hasCommandNames = commandNames !== undefined && commandNames.length > 0`):

- `projectDir` set **and** has names:
  - 1 name: `... where po.project_dir = ? and po.command_name = ?` params `[projectDir, name]`
  - N names: `... where po.project_dir = ? and po.command_name in (?, ?, ...)` params `[projectDir, ...names]`
- `projectDir` set, no names: `... where po.project_dir = ?` params `[projectDir]`
- No `projectDir`, has names:
  - 1 name: `... where po.command_name = ?`
  - N names: `... where po.command_name in (?, ...)`
- Neither ⇒ throw `Error('Must provide projectDir or commandNames')`.

Then appended in this fixed order:
- if `sinceTimestamp !== undefined`: `' and po.timestamp > ?'` (strictly greater)
- if `afterLogId != null`: `' and po.id > ?'` (strictly greater; note `!= null` so both `null` and `undefined` are skipped — `LogIterator` relies on `currentLogId: number | null`)
- always: `' order by po.timestamp desc, po.id desc'` (**most recent first**)
- if `limit !== undefined`: `' limit ?'`

The IN-clause placeholder string is `'?, ?, ?'` (comma-space). The original `__tests__/buildLogSearchQuery.test.ts` asserts these exact strings; the spacing is replicated exactly. Examples:
- Single name + limit: `select po.* from process_output po where po.project_dir = ? and po.command_name = ? order by po.timestamp desc, po.id desc limit ?`
- All filters, 2 names: `... where po.project_dir = ? and po.command_name in (?, ?) and po.timestamp > ? and po.id > ? order by po.timestamp desc, po.id desc limit ?`

## 6. getProcessLogs / eviction info

`rust/src/logs/process_logs.rs` (mirrors `src/logs/processLogs.ts:47-81`).

`get_process_logs_with_eviction_info(options)`:
1. Build query, run `db.list(sql, params)` → rows in DESC order (newest first).
2. **Eviction detection**: only if `limit !== undefined` AND `rows.length >= limit`. Rebuild the same query with `limit: undefined`, wrap as `select count(*) as total from (<innerSql>)` with the inner params, run `db.get`. If `total > rows.length` ⇒ `logsWereEvicted = true`.
3. Reverse the result in place to **chronological order (oldest first)** for the return value (`const sorted = logItems.reverse()` in the original).
4. Returns `{ logs: sorted, logsWereEvicted }`.

`get_process_logs(options)` = `.logs` only.

Subtle: query fetches newest-N (DESC + limit) then reverses, so you always get the *most recent* N logs presented oldest-first. The count subquery embeds the inner SQL via string interpolation — params order is preserved (inner params only, no limit param). The `db.list` analog returns rows in the SQL order, then reverses.

## 7. LatestExecutionLogFilter

`rust/src/log_filters/latest_execution_log_filter.rs` (mirrors `src/log-filters/LatestExecutionLogFilter.ts`). Two-phase. Input logs MUST be chronological (oldest first).

State: `recentCommandLaunch: Map<commandName, {startLogId}>`, `showPastLogsBehavior`, optional `recentWindowMs`, derived `minTimestamp?` (stored as `min_timestamp: Option<f64>`).

`check_latest_launch_status(logs)`:
- Clears the map.
- If `recentWindowMs` set: `minTimestamp = (Date.now() - recentWindowMs) / 1000` (ms→seconds; **non-integer float allowed**, compared with `>=` against integer-second timestamps).
- For each log: if `log_type === process_start_initiated (3)`, set `map[command] = {startLogId: log.id}`. The LAST one wins (most recent launch per command).

`filter(logs)` iterates chronologically and decides per log:
- If a launch status exists for the command: include iff `log.id >= status.startLogId` **and** `passesTimestampWindow(log)`.
- Else (no recorded launch for command yet):
  - If `log.log_type === process_start_initiated (3)`: record `{startLogId: log.id}` now, include iff `passesTimestampWindow`. (This handles launch events that appear in the filtered batch after `check_latest_launch_status` was called on an earlier batch.)
  - Else if `showPastLogsBehavior === 'show_logs_from_previous_launch'`: include iff `passesTimestampWindow`.
  - Else (`'only_show_after_recent_launch'`): exclude.

`passesTimestampWindow(log)`: `true` if `minTimestamp === undefined`, else `log.timestamp >= minTimestamp`.

Behavior semantics:
- `show_logs_from_previous_launch`: used by `logs` and `watch` — shows old logs even with no launch marker. (`logs-command.ts:25`, `watchProcess.ts:44`)
- `only_show_after_recent_launch`: used by `run`/`wait-for-log` — suppresses everything until a `process_start_initiated` is seen. (`wait-for-log-command.ts:19,43`)

Note: `filter` is stateful across calls (the map persists and can be populated mid-stream), which matters for the streaming `watch`/`wait` loops that call `filter` repeatedly on successive batches.

## 8. ExecutionStatusTracker

`rust/src/log_filters/execution_status_tracker.rs` (mirrors `src/log-filters/ExecutionStatusTracker.ts`). Tracks, per command, the latest lifecycle event seen.

- `apply(logs)`: for each log whose `log_type ∈ {3,4,5,6}`, set `map[command] = {latestLifecycleEvent: log_type}` (last wins).
- `count_running_processes()`: count distinct commands whose `latestLifecycleEvent` is `process_started (5)` **or** `process_start_initiated (3)`. (Returns `Set.size`.) Used only by `watch` to print the "still running in the background" trailer (`watchProcess.ts:105-117`).

## 9. LogIterator

`rust/src/logs/log_iterator.rs` (mirrors `src/logs/LogIterator.ts`). Cursor over logs using `afterLogId`.

- Field `currentLogId: number | null = null` (Rust `current_log_id: Option<i64>`); holds `options: LogSearchOptions`.
- `copy()`: shallow copy sharing `options`, copies `currentLogId`.
- `reset_to_latest_log_message()`: sets `currentLogId = null`, queries with `limit: 1` (gets the single newest log), and if present sets `currentLogId = logs[0].id`. After this, iteration yields only logs strictly newer than that id (skips history).
- `peek_next_logs(partialOpts)` (private): merges `this.options`, `partialOpts`, and `afterLogId: this.currentLogId`; runs `get_process_logs`. Returns chronological logs with `id > currentLogId` (when set).
- `get_next_logs(partialOpts={})`: calls peek; if non-empty, advances `currentLogId = logs[last].id`; returns the batch.
- `it()` (async stream): infinite loop — yields each log from `peek_next_logs()` (advancing `currentLogId` per yielded log), then sleeps 100ms, repeat forever. Used by `startOneService` (`startOneService.ts:153`).

Subtle: `get_next_logs` advances by the **last** element id (batch granularity); `it()` advances per-yielded log. Both rely on the chronological-order guarantee from `get_process_logs`. When `afterLogId` is null the WHERE clause omits the id filter entirely (so the first `get_next_logs` returns up to `limit` most-recent logs).

Polling intervals: `LogIterator.it()` = 100ms; `watchProcess` poll = 200ms (`POLL_INTERVAL`), initial batch 100 (`INITIAL_LOG_COUNT`), recent window 10_000ms (`RECENT_LOG_WINDOW_MS`); `wait-for-log` poll = 200ms, search limit 1000.

## 10. Console formatting

`rust/src/logs/console_log.rs` (mirrors `src/logs.ts`).

`ConsoleLogOptions`: `format: 'pretty' | 'json'`, `prefix?: string`, `enableAppNamePrefix?: boolean`.

`console_log_row(row, options)` (mirrors `logs.ts:63-85`):
- If `enableAppNamePrefix`: `prefix = `[${row.command_name}] ${prefix || ''}`` (prepends `[command] ` to any existing prefix).
- Dispatch by `log_type`:
  - `stdout (1)` → `console_log_stdout`
  - `stderr (2)` → `console_log_stderr`
  - `process_exited (6)` and `process_start_failed (4)` → `console_log_system_message`
  - `process_start_initiated (3)` and `process_started (5)` → **hidden** (no output)

Exact output (all via `console.log`, i.e. each prints one line + `\n`):
- stdout pretty: `(prefix ?? '') + msg`
- stdout json: `JSON.stringify({ stdout: msg })`
- stderr pretty: `(prefix ?? '') + '[stderr] ' + msg`
- stderr json: `JSON.stringify({ stderr: msg })`
- system pretty: `(prefix ?? '') + '[' + msg + ']'` (i.e. wrapped in brackets)
- system json: `JSON.stringify({ message: msg })`

So a blended-mode stderr line is: `[<command>] [stderr] <content>`. A system message in blended mode: `[<command>] [<content>]`.

There are **no ANSI colors** in this code path — output is plain text. `prefix` in `watchProcess` blended mode includes a trailing space: `` `[${log.command_name}] ` `` (`watchProcess.ts:84`), whereas the `enableAppNamePrefix` path produces `[cmd] ` then concatenates raw content. These two prefix mechanisms produce slightly different spacing; `logs-command` uses `enableAppNamePrefix`, `watchProcess` uses `prefix`.

`info_log(...args)` (mirrors `logs.ts:22-43`): debug file logger gated by env `CANDLE_ENABLE_LOGS` (`'true'` or `'1'`). Writes `[<ISO timestamp>] <args joined by space>\n` appended to `./candle.log` (cwd). Non-string args are `JSON.stringify`'d. Failures only `console.error`. The enabled flag is memoized on first check. In Rust the ISO timestamp is RFC3339/ISO-8601 via `chrono`/`time`.

## 11. logs command

`rust/src/commands/logs.rs` (mirrors `src/logs-command.ts`).

`handle_logs_command({ projectDir, commandNames, limit=100, startAtId })`:
1. `isBlendedMode = commandNames.length !== 1` (so 0 names ⇒ blended too).
2. `get_process_logs_with_eviction_info({ projectDir, commandNames: len>0?commandNames:undefined, limit, afterLogId: startAtId })`.
3. New `LatestExecutionLogFilter({ showPastLogsBehavior: 'show_logs_from_previous_launch' })`; `check_latest_launch_status(allLogs)`; `logs = filter(allLogs)`.
4. If empty: print exactly `No logs found for command '<name>' in project '<projectDir>'.` (when exactly 1 name) else `No logs found for commands in project '<projectDir>'.` Return.
5. If `logsWereEvicted`: print exactly `-- older logs have been removed --`.
6. For each log: `console_log_row(log, { format: 'pretty', enableAppNamePrefix: isBlendedMode })`.

CLI flags map to: limit (default 100), start-at id. The handler takes `limit` and `startAtId`.

## 12. clear-logs command

`rust/src/commands/clear_logs.rs` (mirrors `src/clear-logs-command.ts`).

`handle_clear_logs_command({ projectDir, commandNames })`:
1. Print `Clearing logs for project: <projectDir>`.
2. For each `commandName`: `DELETE FROM process_output WHERE command_name = ? AND project_dir = ?` params `[commandName, projectDir]`; accumulate `result.changes || 0` into `clearedCount`.
3. If `clearedCount > 0`: print `✓ Cleared <n> log entries` (leading U+2713 CHECK MARK). Else: print `- No logs found to clear`.
4. Orphan cleanup: `DELETE FROM process_output WHERE (command_name, project_dir) NOT IN (SELECT command_name, project_dir FROM processes)`.
5. `VACUUM`.
6. Print `\nLogs cleared successfully!` (leading blank line).
7. On any thrown error: `console.error('Error clearing logs:', error)` then `process.exit(1)`.

Note: requires `result.changes` from the DELETE (SQLite `changes()` / rows-affected). The orphan delete references the `processes` table.

## 13. Eviction / retention (`rust/src/db/cleanup.rs`) — related subsystem

Not strictly "logs command" but governs log lifetime. `maybe_run_cleanup()` runs at most every `CLEANUP_INTERVAL_SECONDS = 600`s (gated by `process_last_cleanup.timestamp`). `run_cleanup(config)`:
- Time eviction: `delete from process_output where timestamp < ?` with `now - maxRetentionSeconds`.
- `cleanup_stale_processes()`.
- Per-service cap: group `process_output` by `(project_dir, command_name)` `having count(*) > maxLogsPerService`; for each, find `id` at `order by timestamp desc, id desc limit 1 offset maxLogsPerService`, then `delete ... where ... and id <= ?` (keeps newest `maxLogsPerService`).
- `vacuum`; upsert `process_last_cleanup`.

Defaults (mirrors `configFile.ts:221-224`): `maxLogsPerService = 1000`, `maxRetentionSeconds = 86400` (24h). Config overrides via `.candle.json` `logEviction.{maxLogsPerService,maxRetentionSeconds}`, validated as positive integers ≥ 1.

## 14. DB access layer

The original TS used a wrapper `db` with: `run(sql, params) -> { changes }`, `list(sql, params) -> rows[]`, `get(sql, params) -> row | undefined`, `upsert(table, keyObj, valueObj)`, over synchronous SQLite (better-sqlite3-style). The Rust implementation uses `rusqlite` (sync) with prepared statements; `?` positional params map directly. `db.run` `changes` ⇒ `Connection::execute` return value.

## 15. Dependency mapping (npm → Rust crate)

- SQLite (better-sqlite3 / node:sqlite synchronous) → `rusqlite` (bundled SQLite). `strftime('%s','now')` and `count(*)` subquery are plain SQLite, portable as-is.
- `fs`/`path` (info_log) → `std::fs`, `std::path`.
- ISO timestamp in `info_log` (`new Date().toISOString()`) → `chrono`/`time` formatted as RFC3339/ISO-8601.
- Console output via `console.log` → `println!` to stdout. JSON output uses `JSON.stringify` → `serde_json`. The async iterator/`setTimeout` polling → `tokio` with `tokio::time::sleep`, or a sync loop with `std::thread::sleep`.
- No color library is used in this subsystem.

## 16. Subtle / easy-to-get-wrong notes

1. Timestamps are **seconds**; `recentWindowMs/1000` yields a **float** cutoff compared with `>=` — the float/`f64` comparison is kept, or you'll off-by-one on boundary logs. Log timestamps are not converted to ms.
2. `get_process_logs` returns **chronological (reversed)** order despite the DESC SQL. Every downstream consumer assumes oldest-first. The reverse happens in app code, not SQL.
3. Eviction detection only triggers when `rows.length >= limit` AND `limit` is set; the count subquery must use the *limitless* query's params (no limit param appended).
4. `afterLogId` uses `!= null` (skips both null/undefined) and `> ?` (strict). `sinceTimestamp` uses `!== undefined` and `> ?` (strict).
5. `process_start_initiated (3)` — not `process_started (5)` — is the launch boundary for `LatestExecutionLogFilter`. But `ExecutionStatusTracker.count_running_processes` treats BOTH 3 and 5 as "running".
6. Hidden log types in console output: 3 and 5 produce no line. 4 and 6 render as bracketed system messages using their `content`.
7. `console_log_row` accesses `row.content` for system/stdout/stderr even though `content` is nullable — a null content for stdout would render the prefix + `null`-ish; JS coercion is replicated carefully (in practice stdout/stderr always have content).
8. Two distinct prefix mechanisms (`prefix` string vs `enableAppNamePrefix`) — `logs` uses the latter, `watch` uses the former; spacing differs subtly.
9. `LatestExecutionLogFilter.filter` is **stateful** and designed to be called repeatedly across streaming batches; the map is not reset between calls.
10. Exact user-facing strings (for tests): `'-- older logs have been removed --'`, `"No logs found for command '<name>' in project '<dir>'."`, `'✓ Cleared <n> log entries'` (Unicode checkmark), `'- No logs found to clear'`, `'\nLogs cleared successfully!'`, `'Clearing logs for project: <dir>'`, `'Must provide projectDir or commandNames'`.
