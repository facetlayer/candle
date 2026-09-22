# Logs subsystem

The Rust logs subsystem lives under `rust/src/logs/` (`log_type.rs`, `process_logs.rs`, `log_iterator.rs`, `console_log.rs`), `rust/src/log_filters/` (`latest_execution_log_filter.rs`, `execution_status_tracker.rs`), and the CLI handlers `rust/src/commands/logs.rs` and `rust/src/commands/clear_logs.rs`. It was ported from the former Node implementation. That implementation has been removed; the `src/...` (TypeScript) references below are historical pointers to where each piece came from, not files that exist in the repo today.

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

`LogSearchOptions` (derives `Default`; originally `processLogs.ts:22-31`):
- `project_dir: Option<String>` (primary)
- `command_names: Vec<String>`: empty ⇒ all commands in the project
- `limit: Option<i64>`
- `since_timestamp: Option<i64>` (seconds)
- `after_log_id: Option<i64>`
- `min_log_id: Option<i64>`: only rows with `id >= min_log_id`
- `log_types: Vec<i64>`: only rows of these types; empty ⇒ every type
- `latest_launch_only: bool`: drop rows older than each command's latest `process_start_initiated` (rows from a previous run)

Insert (mirrors `processLogs.ts:33-39`):
```sql
insert into process_output(command_name, project_dir, content, log_type) values(?, ?, ?, ?)
```
(`timestamp` and `id` use DB defaults.)

## 4. Query accumulator

The original `src/logs/SqlBuilder.ts` was a trivial accumulator: `add(sqlFragment, params[])` appends a string to `sql` and pushes params; `getSql()`/`getParams()` return them. No spacing/escaping logic. Fragments are concatenated **verbatim**, so leading spaces in fragments matter (e.g. `' and po.timestamp > ?'`). The Rust implementation uses the equivalent `String` + `Vec<rusqlite::types::Value>` pair, built inside `build_log_search_query` in `rust/src/logs/process_logs.rs`. The project/command part of the WHERE clause comes from a separate `scope_clause(options)` helper (also reused by `get_log_tail`, §6), and the log-type and latest-launch filters from `push_log_type_filter` / `push_latest_launch_filter`.

## 5. build_log_search_query — exact SQL

`build_log_search_query` in `rust/src/logs/process_logs.rs` (ported from `src/logs/buildLogSearchQuery.ts`). Base SELECT is always `select po.* from process_output po where <scope>`. `scope_clause` branch logic (`has names` = `!command_names.is_empty()`):

- `projectDir` set **and** has names:
  - 1 name: `... where po.project_dir = ? and po.command_name = ?` params `[projectDir, name]`
  - N names: `... where po.project_dir = ? and po.command_name in (?, ?, ...)` params `[projectDir, ...names]`
- `projectDir` set, no names: `... where po.project_dir = ?` params `[projectDir]`
- No `projectDir`, has names:
  - 1 name: `... where po.command_name = ?`
  - N names: `... where po.command_name in (?, ...)`
- Neither ⇒ the scope is `1 = 0`, so the query matches nothing. (The Node original threw `Error('Must provide projectDir or commandNames')`; Rust returns no rows rather than panicking.)

Then appended in this fixed order:
- if `since_timestamp` is set: `' and po.timestamp > ?'` (strictly greater)
- if `after_log_id` is set: `' and po.id > ?'` (strictly greater; `LogIterator` relies on `current_log_id: Option<i64>`, with `None` omitting the filter)
- if `min_log_id` is set: `' and po.id >= ?'`
- if `log_types` is non-empty: `' and po.log_type in (?, ...)'`
- if `latest_launch_only`: `' and po.id >= coalesce((select max(p2.id) from process_output p2 where p2.project_dir = po.project_dir and p2.command_name = po.command_name and p2.log_type = ?), 0)'` with param `3` (`process_start_initiated`)
- always: `' order by po.timestamp desc, po.id desc'` (**most recent first**)
- if `limit` is set: `' limit ?'`

The IN-clause placeholder string is `'?, ?, ?'` (comma-space), matching the strings the original `__tests__/buildLogSearchQuery.test.ts` asserted. Examples:
- Single name + limit: `select po.* from process_output po where po.project_dir = ? and po.command_name = ? order by po.timestamp desc, po.id desc limit ?`
- All filters, 2 names: `... where po.project_dir = ? and po.command_name in (?, ?) and po.timestamp > ? and po.id > ? order by po.timestamp desc, po.id desc limit ?`

## 6. getProcessLogs / eviction info

`rust/src/logs/process_logs.rs` (ported from `src/logs/processLogs.ts:47-81`).

`get_process_logs_with_eviction_info(options)` returns `ProcessLogResult { logs, logs_were_evicted }`:
1. Build query, run `db.list(sql, params)` → rows in DESC order (newest first).
2. **Eviction detection**: only if `limit !== undefined` AND `rows.length >= limit`. Rebuild the same query with `limit: undefined`, wrap as `select count(*) as total from (<innerSql>)` with the inner params, run `db.get`. If `total > rows.length` ⇒ `logsWereEvicted = true`.
3. Reverse the result in place to **chronological order (oldest first)** for the return value (`const sorted = logItems.reverse()` in the original).
4. Returns `{ logs: sorted, logsWereEvicted }`.

`get_process_logs(options)` = `.logs` only.

Subtle: query fetches newest-N (DESC + limit) then reverses, so you always get the *most recent* N logs presented oldest-first. The count subquery embeds the inner SQL via string interpolation — params order is preserved (inner params only, no limit param). The `db.list` analog returns rows in the SQL order, then reverses.

### get_log_tail (used by `candle logs`)

`get_log_tail(conn, options, limit) -> LogTail { logs, truncated }` backs `logs --count`. The limit counts only the lines `logs` will actually print from each command's latest run; counting marker rows or a previous run's rows made `--count N` print fewer than N lines.
1. **Printable window**: `get_process_logs` with `limit: Some(limit)`, `log_types` = the printable types (`stdout 1`, `stderr 2`, `process_start_failed 4`, `process_exited 6`), and `latest_launch_only: true`. If this is empty, return an empty `LogTail`.
2. **Launch boundaries**: `select max(po.id) from process_output po where <scope> and po.log_type = 3 [and po.id > after_log_id] group by po.command_name`; take the oldest of those per-command boundaries.
3. **Marker rows**: fetch `process_start_initiated (3)` and `process_started (5)` rows (no limit) with `min_log_id` = min(oldest boundary, window's min id), append them, and sort everything by `id`. These are what `LatestExecutionLogFilter` needs to tell this launch's rows from the previous one's.
4. **`truncated`**: count printable, latest-launch rows in scope with `id <` the window's min id (and `> after_log_id` if set). `truncated = count > 0`. Rows from a previous launch never count.

## 7. LatestExecutionLogFilter

`rust/src/log_filters/latest_execution_log_filter.rs` (ported from `src/log-filters/LatestExecutionLogFilter.ts`). Two-phase. Input logs MUST be chronological (oldest first).

State: `recent_command_launch: HashMap<command_name, LaunchStatus { start_log_id, reported_start_result }>`, `show_past_logs_behavior`, optional `recent_window_ms`, derived `min_timestamp: Option<f64>`.

`check_latest_launch_status(logs)`:
- Clears the map.
- If `recentWindowMs` set: `minTimestamp = (Date.now() - recentWindowMs) / 1000` (ms→seconds; **non-integer float allowed**, compared with `>=` against integer-second timestamps).
- For each log: if `log_type === process_start_initiated (3)`, set `map[command] = {start_log_id: log.id, reported_start_result: false}`. The LAST one wins (most recent launch per command). A later `process_started (5)` or `process_start_failed (4)` for that command sets `reported_start_result = true`.

`filter(logs)` iterates chronologically and decides per log:
- If the log is a `process_start_initiated (3)` with a higher id than the recorded boundary (or there is none), it becomes the new boundary (`reported_start_result: false`). The boundary only moves **forward**: an older launch replayed in the batch does not undo the one `check_latest_launch_status` found. (This handles relaunches seen while streaming.)
- If a launch status exists for the command:
  - A `process_exited (6)` row seen before this launch's start result is **excluded**. A monitor only writes `process_exited` after `process_started`, so such an exit belongs to the previous (just-killed) instance.
  - Otherwise include iff `log.id >= status.start_log_id` **and** `passes_timestamp_window(log)`.
- Else (no recorded launch for command):
  - If `show_past_logs_behavior == ShowLogsFromPreviousLaunch`: include iff `passes_timestamp_window`.
  - Else (`OnlyShowAfterRecentLaunch`): exclude.
- After deciding, a `process_started`/`process_start_failed` row marks the command's status as `reported_start_result = true`.

`passesTimestampWindow(log)`: `true` if `minTimestamp === undefined`, else `log.timestamp >= minTimestamp`.

Behavior semantics:
- `show_logs_from_previous_launch`: used by `logs` (including the MCP `GetLogs` tool, which calls `handle_logs_command`) and `watch` — shows old logs even with no launch marker. (`logs-command.ts:25`, `watchProcess.ts:44`)
- `only_show_after_recent_launch`: used by `wait-for-log` and by `watch_started_services` (the post-launch watch in interactive `start`/`restart`) — suppresses everything until a `process_start_initiated` is seen. (`wait-for-log-command.ts:19,43`)

Note: `filter` is stateful across calls (the map persists and can be populated mid-stream), which matters for the streaming `watch`/`wait` loops that call `filter` repeatedly on successive batches.

## 8. ExecutionStatusTracker

`rust/src/log_filters/execution_status_tracker.rs` (mirrors `src/log-filters/ExecutionStatusTracker.ts`). Tracks, per command, the latest lifecycle event seen.

- `apply(logs)`: for each log whose `log_type ∈ {3,4,5,6}`, set `map[command] = {latestLifecycleEvent: log_type}` (last wins).
- `count_running_processes()`: count distinct commands whose `latestLifecycleEvent` is `process_started (5)` **or** `process_start_initiated (3)`. (Returns `Set.size`.) Used only by `watch` to print the "still running in the background" trailer (`watchProcess.ts:105-117`).

## 9. LogIterator

`rust/src/logs/log_iterator.rs` (ported from `src/logs/LogIterator.ts`). Cursor over logs using `after_log_id`.

- Fields: `project_dir`, `command_names`, a default `limit: Option<i64>`, and `pub current_log_id: Option<i64>` (starts `None`). Constructors `new(project_dir, command_names)` and `with_limit(project_dir, command_names, limit)`.
- `copy()`: a clone at the current cursor position.
- `reset_to_latest_log_message(conn)`: sets `current_log_id = None`, queries with `limit: 1` (gets the single newest log), and if present sets `current_log_id` to its id. After this, iteration yields only logs strictly newer than that id (skips history).
- `peek_next_logs(conn, limit_override)`: runs `get_process_logs` with `after_log_id: current_log_id` and the effective limit (`limit_override`, else the constructor limit) WITHOUT advancing. Returns chronological logs with `id > current_log_id` (when set).
- `get_next_logs(conn, limit_override)`: calls peek; if non-empty, advances `current_log_id` to the last row's id; returns the batch.
- There is no async `it()` stream as in the Node original. The polling loop lives in the caller: `start_one_service` calls `get_next_logs` every `POLL_INTERVAL` (100ms) until the 10s start timeout.

Subtle: `get_next_logs` advances by the **last** element id (batch granularity), relying on the chronological-order guarantee from `get_process_logs`. When `current_log_id` is `None` the WHERE clause omits the id filter entirely (so the first `get_next_logs` returns up to `limit` most-recent logs).

Polling intervals: `start_one_service` = 100ms; `watch` poll = 200ms (`POLL_INTERVAL`), initial batch 100 (`INITIAL_LOG_COUNT`), recent window 10_000ms (`RECENT_LOG_WINDOW_MS`); `wait-for-log` poll = 200ms, search limit 1000.

## 10. Console formatting

`rust/src/logs/console_log.rs` (ported from `src/logs.ts`).

`ConsoleLogOptions`: `format: Option<OutputFormat>` (`Pretty` | `Json`, default `Pretty`), `prefix: Option<String>`, `enable_app_name_prefix: bool`.

`console_log_row(row, options)` (mirrors `logs.ts:63-85`):
- If `enableAppNamePrefix`: `prefix = `[${row.command_name}] ${prefix || ''}`` (prepends `[command] ` to any existing prefix).
- Dispatch by `log_type`:
  - `stdout (1)` → `console_log_stdout`
  - `stderr (2)` → `console_log_stderr`
  - `process_exited (6)` and `process_start_failed (4)` → `console_log_system_message`
  - `process_start_initiated (3)` and `process_started (5)` → **hidden** (no output)

Exact output (all via `output::out`, i.e. stdout, one line each, even for stderr-typed rows). A `None` content renders as the empty string:
- stdout pretty: `(prefix ?? '') + msg`
- stdout json: `JSON.stringify({ stdout: msg })`
- stderr pretty: `(prefix ?? '') + '[stderr] ' + msg`
- stderr json: `JSON.stringify({ stderr: msg })`
- system pretty: `(prefix ?? '') + '[' + msg + ']'` (i.e. wrapped in brackets)
- system json: `JSON.stringify({ message: msg })`

So a blended-mode stderr line is: `[<command>] [stderr] <content>`. A system message in blended mode: `[<command>] [<content>]`.

There are **no ANSI colors** in this code path — output is plain text. `prefix` in `watchProcess` blended mode includes a trailing space: `` `[${log.command_name}] ` `` (`watchProcess.ts:84`), whereas the `enableAppNamePrefix` path produces `[cmd] ` then concatenates raw content. These two prefix mechanisms produce slightly different spacing; `logs-command` uses `enableAppNamePrefix`, `watchProcess` uses `prefix`.

The Node `info_log(...)` debug file logger from `logs.ts` has no counterpart in `console_log.rs`. Its Rust equivalent is `debug::debug_log(msg)` in `rust/src/debug.rs` (see [cli.md](cli.md)): when `CANDLE_ENABLE_LOGS` is non-empty it appends `msg` plus a newline to `./candle.log` (cwd), with no timestamp, and swallows IO errors.

## 11. logs command

`rust/src/commands/logs.rs` (ported from `src/logs-command.ts`).

`handle_logs_command(conn, project_dir, command_names, limit, start_at_id)`:
1. `is_blended_mode = command_names.len() != 1` (so 0 names ⇒ blended too).
2. `get_log_tail(conn, { project_dir, command_names, after_log_id: start_at_id }, limit)` (§6). A DB error yields an empty tail.
3. New `LatestExecutionLogFilter::new(ShowLogsFromPreviousLaunch, None)`; `check_latest_launch_status(&all_logs)`; `logs = filter(&all_logs)`.
4. If empty: print exactly `No logs found for command '<name>' in project '<projectDir>'.` (when exactly 1 name) else `No logs found for commands in project '<projectDir>'.` Return.
5. If `truncated`: print exactly `-- showing the last <N> lines; use --count to see more --` (`the last line;` when `N == 1`). A previous run's hidden lines never trigger it.
6. For each log: `console_log_row(log, { format: Pretty, enable_app_name_prefix: is_blended_mode })`.

CLI flags map to: `--count` (limit, default 100), `--start-at` (id). `cmd_logs` in `main.rs` parses both and runs `maybe_run_cleanup` first.

## 12. clear-logs command

`rust/src/commands/clear_logs.rs` (ported from `src/clear-logs-command.ts`).

`handle_clear_logs_command({ projectDir, commandNames })`:
1. Print `Clearing logs for project: <projectDir>`.
2. With no names: `DELETE FROM process_output WHERE project_dir = ?`, which clears every service in the project, including transient ones and ones no longer in `.candle.json`. With names, for each `commandName`: `DELETE FROM process_output WHERE command_name = ? AND project_dir = ?` params `[commandName, projectDir]`; accumulate `result.changes || 0` into `clearedCount`.
3. If `clearedCount > 0`: print `✓ Cleared <n> log entries` (leading U+2713 CHECK MARK). Else: print `- No logs found to clear`.
4. Orphan cleanup: `DELETE FROM process_output WHERE (command_name, project_dir) NOT IN (SELECT command_name, project_dir FROM processes)`.
5. `VACUUM`.
6. Print `\nLogs cleared successfully!` (leading blank line).
7. On a database error the handler returns `Err`; `cmd_clear_logs` in `main.rs` prints `Error clearing logs: <e>` to stderr and exits 1.

Note: requires `result.changes` from the DELETE (SQLite `changes()` / rows-affected). The orphan delete references the `processes` table.

## 13. Eviction / retention (`rust/src/db/cleanup.rs`) — related subsystem

Not strictly "logs command" but governs log lifetime. `maybe_run_cleanup(conn)` runs at most every `CLEANUP_INTERVAL_SECONDS = 600`s (gated by `process_last_cleanup.timestamp`). `run_cleanup(conn)` resolves the eviction config per `project_dir` (from that directory's `.candle.json`, cached per pass, defaults on any error):
- Time eviction, per project dir: `delete from process_output where project_dir = ? and timestamp < ?` with `now - maxRetentionSeconds`.
- `cleanup_stale_processes()`.
- Per-service cap: group `process_output` by `(project_dir, command_name)` with `count(*)`, and skip (in Rust, not a SQL `having`) services at or under their project's `maxLogsPerService`; for each over-limit one, find `id` at `order by timestamp desc, id desc limit 1 offset maxLogsPerService`, then `delete ... where ... and id <= ?` (keeps newest `maxLogsPerService`).
- `vacuum`; upsert `process_last_cleanup`.

Defaults (`LOG_EVICTION_DEFAULTS` in `config/model.rs`, originally `configFile.ts:221-224`): `maxLogsPerService = 1000`, `maxRetentionSeconds = 86400` (24h). Config overrides via `.candle.json` `logEviction.{maxLogsPerService,maxRetentionSeconds}`, validated as positive integers ≥ 1.

## 14. DB access layer

The original TS used a wrapper `db` with: `run(sql, params) -> { changes }`, `list(sql, params) -> rows[]`, `get(sql, params) -> row | undefined`, `upsert(table, keyObj, valueObj)`, over synchronous SQLite (better-sqlite3-style). The Rust implementation uses `rusqlite` (sync) with prepared statements; `?` positional params map directly. `db.run` `changes` ⇒ `Connection::execute` return value.

## 15. Dependency mapping (npm → Rust crate)

- SQLite (better-sqlite3 / node:sqlite synchronous) → `rusqlite` (bundled SQLite). `strftime('%s','now')` and `count(*)` subquery are plain SQLite, portable as-is.
- `fs`/`path` (info_log) → `std::fs`, `std::path`.
- Console output via `console.log` → `output::out` (stdout in the CLI, captured under MCP). JSON output uses `JSON.stringify` → `serde_json`. The async iterator/`setTimeout` polling → a sync loop with `std::thread::sleep` (no async runtime).
- No color library is used in this subsystem.

## 16. Subtle / easy-to-get-wrong notes

1. Timestamps are **seconds**; `recentWindowMs/1000` yields a **float** cutoff compared with `>=` — the float/`f64` comparison is kept, or you'll off-by-one on boundary logs. Log timestamps are not converted to ms.
2. `get_process_logs` returns **chronological (reversed)** order despite the DESC SQL. Every downstream consumer assumes oldest-first. The reverse happens in app code, not SQL.
3. Eviction detection (in `get_process_logs_with_eviction_info`, no longer used by `logs`) only triggers when `rows.length >= limit` AND `limit` is set; the count subquery must use the *limitless* query's params (no limit param appended).
4. `afterLogId` uses `!= null` (skips both null/undefined) and `> ?` (strict). `sinceTimestamp` uses `!== undefined` and `> ?` (strict).
5. `process_start_initiated (3)` — not `process_started (5)` — is the launch boundary for `LatestExecutionLogFilter`. But `ExecutionStatusTracker.count_running_processes` treats BOTH 3 and 5 as "running".
6. Hidden log types in console output: 3 and 5 produce no line. 4 and 6 render as bracketed system messages using their `content`.
7. `console_log_row` reads `row.content` for system/stdout/stderr even though `content` is nullable; a `None` renders as the empty string (in practice stdout/stderr always have content).
8. Two distinct prefix mechanisms (`prefix` string vs `enableAppNamePrefix`) — `logs` uses the latter, `watch` uses the former; spacing differs subtly.
9. `LatestExecutionLogFilter.filter` is **stateful** and designed to be called repeatedly across streaming batches; the map is not reset between calls.
10. Exact user-facing strings (for tests): `'-- showing the last <N> lines; use --count to see more --'`, `"No logs found for command '<name>' in project '<dir>'."`, `'✓ Cleared <n> log entries'` (Unicode checkmark), `'- No logs found to clear'`, `'\nLogs cleared successfully!'`, `'Clearing logs for project: <dir>'`.
