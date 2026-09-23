# Logs subsystem

The Rust logs subsystem lives under `rust/src/logs/` (`log_type.rs`, `process_logs.rs`, `log_iterator.rs`, `console_log.rs`), `rust/src/log_filters/` (`latest_run_filter.rs`), and the CLI handlers `rust/src/commands/logs.rs` and `rust/src/commands/clear_logs.rs`.

## 1. Storage model

All logs live in a single SQLite table `process_output`. Schema (`rust/src/db/mod.rs`):

```sql
create table process_output(
    id integer primary key autoincrement,
    command_name text not null,
    project_dir text not null,
    content text,                 -- nullable
    log_type integer not null,
    timestamp integer not null default (strftime('%s', 'now')),
    run_id integer                -- nullable; see "Runs" below
)
```

Indexes:
```sql
create index idx_process_output_command_name on process_output(command_name);
create index idx_process_output_project_dir on process_output(project_dir);
create index idx_process_output_lookup on process_output(project_dir, command_name, timestamp desc, id desc);
create index idx_process_output_run on process_output(project_dir, command_name, run_id);
create index idx_process_output_launches on process_output(project_dir, command_name, log_type, id);
```

Critical points:
- `timestamp` is **whole Unix seconds** (`strftime('%s','now')`), NOT milliseconds. Mixing with millisecond wall-clock time is a recurring foot-gun — see the `/1000` conversion in the filter (§7).
- `content` is nullable. Lifecycle events (types 3 and 5) typically have no content; stdout/stderr always have content. `console_log_row` passes `row.content` even for `process_exited`/`process_start_failed`, so content for those carries a human message.
- `id` is a monotonically increasing autoincrement integer and is the canonical ordering tiebreaker / cursor.

### Runs (`run_id`)

A **run** is one launch of a command. Its id is the id of its `process_start_initiated (3)` row, and every row belonging to it carries that id in `run_id` (the `processes` row has the same column). A command's latest run is simply its highest `run_id`.

- `start_one_service` records the launch with `start_run(conn, command, project_dir) -> run_id` and passes the id to the monitor (`MonitorLaunchInfo.run_id`). The monitor writes every row with `save_run_log(conn, Some(run_id), ...)`, so rows a previous instance's monitor writes after a restart keep the old `run_id` whatever order they land in.
- Rows inserted without a `run_id` (`save_process_log`, i.e. `save_run_log(.., None, ..)`) are assigned one by the trigger `process_output_assign_run`: the latest `process_start_initiated` at or before the row, by id (`run_of_row` in `rust/src/db/mod.rs`). That gives the launch marker its own id and covers a monitor from an older candle that is still running. Rows before a command's first launch stay `NULL`.
- When migration rebuilds `process_output` to add the column, a one-time backfill applies the same positional rule to existing rows (see [database.md](database.md)).
- The stale-process cleanup writes its `process_exited` "Process cleaned up" row with the `processes` row's `run_id`.

Order-based readers ("everything after the latest launch marker") used to show a previous instance's late output as part of the new run; every reader now selects by `run_id` instead. `formal/README.md` has the background.

## 2. ProcessLogType enum (exact integers)

`rust/src/logs/log_type.rs`:

| Name | Value |
|------|-------|
| `stdout` | `1` |
| `stderr` | `2` |
| `process_start_initiated` | `3` (saved immediately when launching begins) |
| `process_start_failed` | `4` |
| `process_started` | `5` (subprocess successfully started) |
| `process_exited` | `6` |

These integers are persisted in the DB and MUST be kept stable. The enum is `#[repr(i64)]` with explicit discriminants, plus a `TryFrom<i64>`.

The four "lifecycle" event types are `{3,4,5,6}`. `process_start_initiated` (3), NOT `process_started` (5), starts a run: its row id is the run id.

## 3. Data types

In `rust/src/logs/process_logs.rs`:

`ProcessLog` (row): `id: i64`, `command_name: String`, `project_dir: String`, `content: Option<String>`, `log_type: i64`, `timestamp: i64`, `run_id: Option<i64>` (all as above).

`LogSearchOptions` (derives `Default`):
- `project_dir: Option<String>` (primary; at least one of `project_dir` / `command_names` must be set)
- `command_names: Vec<String>`: empty ⇒ all commands in the project
- `limit: Option<i64>`
- `since_timestamp: Option<i64>` (seconds)
- `after_log_id: Option<i64>`
- `min_log_id: Option<i64>`: only rows with `id >= min_log_id`
- `log_types: Vec<i64>`: only rows of these types; empty ⇒ every type
- `latest_launch_only: bool`: only rows from each command's latest run (highest `run_id`)
- `run_id: Option<i64>`: only rows from this run

Insert (`save_run_log(conn, run_id, command_name, project_dir, log_type, content)`; `save_process_log` is the same with `run_id = NULL`):
```sql
insert into process_output(command_name, project_dir, content, log_type, run_id) values(?, ?, ?, ?, ?)
```
(`timestamp` and `id` use DB defaults; a `NULL` `run_id` is filled by the trigger, §1.)

Other helpers: `start_run` (insert a `process_start_initiated` row, return its id as the run id) and `latest_run_ids(conn, project_dir, command_names) -> Vec<(command, run_id)>` (`select po.command_name, max(po.run_id) ... where <scope> and po.run_id is not null group by po.command_name`).

## 4. Query accumulator

`build_log_search_query` in `rust/src/logs/process_logs.rs` accumulates the query as a `String` + `Vec<rusqlite::types::Value>` pair: each filter appends its SQL fragment and pushes its params. There is no spacing/escaping logic; fragments are concatenated **verbatim**, so leading spaces in fragments matter (e.g. `' and po.timestamp > ?'`). The project/command part of the WHERE clause comes from a separate `scope_clause(options)` helper (also reused by `latest_run_ids`, §3), and the log-type and latest-launch filters from `push_log_type_filter` / `push_latest_launch_filter`.

## 5. build_log_search_query — exact SQL

`build_log_search_query` in `rust/src/logs/process_logs.rs`. Base SELECT is always `select po.* from process_output po where <scope>`. `scope_clause` branch logic (`has names` = `!command_names.is_empty()`):

- `project_dir` set **and** has names: `... where po.project_dir = ? and po.command_name in (?, ?, ...)` params `[project_dir, ...names]` (one name is `in (?)`)
- `project_dir` set, no names: `... where po.project_dir = ?` params `[project_dir]`
- No `project_dir`, has names: `... where po.command_name in (?, ...)`
- Neither ⇒ the scope is `1 = 0`, so the query matches nothing (a caller error; it returns no rows rather than every row).

Then appended in this fixed order:
- if `since_timestamp` is set: `' and po.timestamp > ?'` (strictly greater)
- if `after_log_id` is set: `' and po.id > ?'` (strictly greater; `LogIterator` relies on `current_log_id: Option<i64>`, with `None` omitting the filter)
- if `min_log_id` is set: `' and po.id >= ?'`
- if `log_types` is non-empty: `' and po.log_type in (?, ...)'`
- if `run_id` is set: `' and po.run_id = ?'`
- if `latest_launch_only`: `' and po.run_id is (select max(p2.run_id) from process_output p2 where p2.project_dir = po.project_dir and p2.command_name = po.command_name)'` (no params). `is` rather than `=` so a command that has never been launched (every `run_id` NULL) keeps its rows.
- always: `' order by po.timestamp desc, po.id desc'` (**most recent first**)
- if `limit` is set: `' limit ?'`

The IN-clause placeholder string is `'?, ?, ?'` (comma-space). Examples:
- Single name + limit: `select po.* from process_output po where po.project_dir = ? and po.command_name in (?) order by po.timestamp desc, po.id desc limit ?`
- All filters, 2 names: `... where po.project_dir = ? and po.command_name in (?, ?) and po.timestamp > ? and po.id > ? order by po.timestamp desc, po.id desc limit ?`

## 6. get_process_logs / eviction info

`rust/src/logs/process_logs.rs`.

`get_process_logs_with_eviction_info(options)` returns `ProcessLogResult { logs, logs_were_evicted }`:
1. Build the query and run it → rows in DESC order (newest first).
2. **Eviction detection**: only if `limit` is set AND `rows.len() >= limit`. Rebuild the same query with `limit: None`, wrap as `select count(*) as total from (<inner_sql>)` with the inner params, run `query_row`. If `total > rows.len()` ⇒ `logs_were_evicted = true`.
3. Reverse the rows in place to **chronological order (oldest first)**.
4. Returns `ProcessLogResult { logs, logs_were_evicted }`.

`get_process_logs(options)` = `.logs` only.

Subtle: query fetches newest-N (DESC + limit) then reverses, so you always get the *most recent* N logs presented oldest-first. The count subquery embeds the inner SQL via string interpolation — params order is preserved (inner params only, no limit param).

### get_log_tail (used by `candle logs`)

`get_log_tail(conn, options, limit) -> LogTail { logs, truncated }` backs `logs --count`, `wait-for-log`'s recent-logs dump, and MCP `GetLogs`. It is `get_process_logs_with_eviction_info` with `limit: Some(limit)`, `log_types` = the printable types (`stdout 1`, `stderr 2`, `process_start_failed 4`, `process_exited 6`) and `latest_launch_only: true`; `logs` = the result's rows (chronological), `truncated` = `logs_were_evicted`. So the limit counts only the lines `logs` will actually print from each command's latest run. Counting marker rows or a previous run's rows made `--count N` print fewer than N lines. No filter pass is needed afterwards.

## 7. LatestRunFilter

`rust/src/log_filters/latest_run_filter.rs`. Keeps a row iff its `run_id` is the highest seen so far for its command (and, optionally, it is within a recent time window). 
State: `latest: HashMap<command_name, Option<i64>>` (highest `run_id` seen; `None` until a row with a run arrives) and `min_timestamp: Option<f64>`.

- `new(recent_window_ms)`: `min_timestamp = (now_unix_millis - window_ms) / 1000.0` (ms→seconds; **non-integer float allowed**, compared with `>=` against integer-second timestamps).
- `seed_latest_runs(conn, project_dir, command_names)`: records each command's latest run from `latest_run_ids` (§3), so a batch of existing rows from a superseded run is dropped even when the newer run's rows are not in the batch.
- `filter(logs)`: for each row, raise the command's latest to `max(latest, row.run_id)`, then keep the row iff `row.run_id == latest` and it passes the timestamp window.

Consequences:
- Order-independent: a previous instance's rows that arrive after the new run's rows keep their old `run_id` and are dropped. (`formal/Candle/RunFilter.lean` proves this.)
- Unseeded and streaming, rows of an old run show until a newer run's row appears, then only the newer run shows.
- Rows with no run (`NULL`, written before the command's first launch) show only until a run appears.
- Stateful across calls: `watch` and `wait-for-log` reuse one instance across poll batches.

Used by `watch` / `watch_started_services` and `wait-for-log`. `logs` needs no filter: `get_log_tail` already selects the latest run in SQL.

## 9. LogIterator

`rust/src/logs/log_iterator.rs`. Cursor over logs using `after_log_id`.

- Fields: `project_dir`, `command_names`, a default `limit: Option<i64>`, and `pub current_log_id: Option<i64>` (starts `None`). Constructors `new(project_dir, command_names)` and `with_limit(project_dir, command_names, limit)`.
- `copy()`: a clone at the current cursor position.
- `reset_to_latest_log_message(conn)`: sets `current_log_id = None`, queries with `limit: 1` (gets the single newest log), and if present sets `current_log_id` to its id. After this, iteration yields only logs strictly newer than that id (skips history).
- `peek_next_logs(conn, limit_override)`: runs `get_process_logs` with `after_log_id: current_log_id` and the effective limit (`limit_override`, else the constructor limit) WITHOUT advancing. Returns chronological logs with `id > current_log_id` (when set).
- `get_next_logs(conn, limit_override)`: calls peek; if non-empty, advances `current_log_id` to the last row's id; returns the batch.
- There is no async stream; the polling loop lives in the caller (`watch`, `wait-for-log`).

Subtle: `get_next_logs` advances by the **last** element id (batch granularity), relying on the chronological-order guarantee from `get_process_logs`. When `current_log_id` is `None` the WHERE clause omits the id filter entirely (so the first `get_next_logs` returns up to `limit` most-recent logs).

Polling intervals: `start_one_service` = 100ms (it polls this run's rows with `get_process_logs { run_id }`, not a `LogIterator`); `watch` poll = 200ms (`POLL_INTERVAL`), initial batch 100 (`INITIAL_LOG_COUNT`), recent window 10_000ms (`RECENT_LOG_WINDOW_MS`); `wait-for-log` poll = 200ms, search limit 1000.

## 10. Console formatting

`rust/src/logs/console_log.rs`.

`ConsoleLogOptions`: `format: Option<OutputFormat>` (`Pretty` | `Json`, default `Pretty`), `prefix: Option<String>`, `enable_app_name_prefix: bool`.

`console_log_row(row, options)`:
- If `enable_app_name_prefix`: `prefix = format!("[{}] {}", row.command_name, prefix.unwrap_or_default())` (prepends `[command] ` to any existing prefix).
- Dispatch by `log_type`:
  - `stdout (1)` → `console_log_stdout`
  - `stderr (2)` → `console_log_stderr`
  - `process_exited (6)` and `process_start_failed (4)` → `console_log_system_message`
  - `process_start_initiated (3)` and `process_started (5)` → **hidden** (no output)

Exact output (all via `output::out`, i.e. stdout, one line each, even for stderr-typed rows). A `None` content renders as the empty string:
- stdout pretty: `prefix + msg`
- stdout json: `{"stdout": msg}` (compact `serde_json`)
- stderr pretty: `prefix + "[stderr] " + msg`
- stderr json: `{"stderr": msg}`
- system pretty: `prefix + "[" + msg + "]"` (i.e. wrapped in brackets)
- system json: `{"message": msg}`

So a blended-mode stderr line is: `[<command>] [stderr] <content>`. A system message in blended mode: `[<command>] [<content>]`.

There are **no ANSI colors** in this code path — output is plain text. `watch_process` in blended mode passes `prefix = "[<command_name>] "`; `logs` and `wait-for-log`'s recent-logs dump use `enable_app_name_prefix` instead, which produces the same `[cmd] ` text followed by any `prefix`.

The debug file logger is separate from console output: `debug::debug_log(msg)` in `rust/src/debug.rs` (see [cli.md](cli.md)): when `CANDLE_ENABLE_LOGS` is non-empty it appends `msg` plus a newline to `./candle.log` (cwd), with no timestamp, and swallows IO errors.

## 11. logs command

`rust/src/commands/logs.rs`.

`handle_logs_command(conn, project_dir, command_names, &LogsCommandOptions { limit, start_at_id, json, more_hint })`:
1. `is_blended_mode = command_names.len() != 1` (so 0 names ⇒ blended too).
2. Fetch: in single mode, one `get_log_tail(conn, { project_dir, command_names, after_log_id: start_at_id }, limit)` (§6). In blended mode the limit applies **per service**: the names (or, with none given, `command_names_with_logs(project_dir, start_at_id)`) are fetched one `get_log_tail` each, and the results are merged in `id` order. A DB error yields an empty tail.
   (`fetch_latest_run_tail`; the tail is already restricted to the latest run, so there is no filter pass.)
3. With `json`: print a pretty JSON array of `{ id, service, type, content, timestamp }` for the printable rows (`type` is `stdout`, `stderr`, `start_failed` or `exited`; launch markers are skipped) and return. No hint; an empty result is `[]`.
4. If empty: print exactly `No logs found for service '<name>' in project '<project_dir>'.` (when exactly 1 name) else `No logs found for services in project '<project_dir>'.` Return.
5. If any tail was `truncated`: single mode prints `-- showing the last <N> lines; <more_hint> --`; blended mode prints `-- showing the last <N> lines per service (<truncated names> had more); <more_hint> --` (`the last line` when `N == 1`). A previous run's hidden lines never trigger it. The CLI's `more_hint` is `use --count to see more`; MCP `GetLogs` passes ``pass a larger `limit` to see more``.
6. For each log: `console_log_row(log, { format: Pretty, enable_app_name_prefix: is_blended_mode })`.

CLI flags map to: `--count` (limit, default 100), `--start-at` (id), `--json`. `cmd_logs` in `main.rs` parses them (a `--count` below 1 or a non-numeric value is a fatal usage error), runs `maybe_run_cleanup`, then validates names with `assert_known_service_names_in_scope`: a name is accepted if it has stored logs or a process row in the project, or is configured; anything else is `No service '<name>' configured for directory: <dir>` on stderr, exit 1.

## 12. clear-logs command

`rust/src/commands/clear_logs.rs`.

`cmd_clear_logs` first validates any names with `assert_known_service_names_in_scope` (the same rule as `logs`), so an unknown name is `No service '<name>' configured for directory: <dir>` on stderr, exit 1.

`handle_clear_logs_command(conn, project_dir, command_names)`:
1. Print `Clearing logs for project: <project_dir>`.
2. With no names: `DELETE FROM process_output WHERE project_dir = ?`, which clears every service in the project, including transient ones and ones no longer in `.candle.json`. With names, for each name: `DELETE FROM process_output WHERE command_name = ? AND project_dir = ?` params `[command_name, project_dir]`. The rows-affected counts (`Connection::execute`) are summed into `cleared_count`.
3. If `cleared_count > 0`: print `Cleared <n> log entries`. Else: print `No logs found to clear`.
4. Orphan cleanup: `DELETE FROM process_output WHERE (command_name, project_dir) NOT IN (SELECT command_name, project_dir FROM processes)`.
5. `VACUUM`.
6. On a database error the handler returns `Err`; `cmd_clear_logs` in `main.rs` prints `Error: Could not clear logs: <e>` to stderr and exits 1.

Note: the orphan delete references the `processes` table, so it also removes logs of other projects' services that have no process row.

## 13. Eviction / retention (`rust/src/db/cleanup.rs`) — related subsystem

Not strictly "logs command" but governs log lifetime. `maybe_run_cleanup(conn)` runs at most every `CLEANUP_INTERVAL_SECONDS = 600`s (gated by `process_last_cleanup.timestamp`). `run_cleanup(conn)` resolves the eviction config per `project_dir` (from that directory's `.candle.json`, cached per pass, defaults on any error):
- Time eviction, per project dir: `delete from process_output where project_dir = ? and timestamp < ?` with `now - maxRetentionSeconds`.
- `cleanup_stale_processes()`.
- Per-service cap: group `process_output` by `(project_dir, command_name)` with `count(*)`, and skip (in Rust, not a SQL `having`) services at or under their project's `maxLogsPerService`; for each over-limit one, find `id` at `order by timestamp desc, id desc limit 1 offset maxLogsPerService`, then `delete ... where ... and id <= ?` (keeps newest `maxLogsPerService`).
- `vacuum`; upsert `process_last_cleanup`.

Defaults (`LOG_EVICTION_DEFAULTS` in `config/model.rs`): `maxLogsPerService = 1000`, `maxRetentionSeconds = 86400` (24h). Config overrides via `.candle.json` `logEviction.{maxLogsPerService,maxRetentionSeconds}`, validated as positive integers ≥ 1.

## 14. DB access layer

`rusqlite` (synchronous, bundled SQLite) with prepared statements and `?` positional params. `Connection::execute` returns the rows-affected count; `strftime('%s','now')` and the `count(*)` subquery are plain SQLite. Output goes through `output::out` (stdout in the CLI, captured under MCP); JSON output uses `serde_json`. Polling is a sync loop with `std::thread::sleep` (no async runtime). No color library is used in this subsystem.

## 15. Subtle / easy-to-get-wrong notes

1. Timestamps are **seconds**; `recent_window_ms / 1000.0` yields a **float** cutoff compared with `>=` — the float/`f64` comparison is kept, or you'll off-by-one on boundary logs. Log timestamps are not converted to ms.
2. `get_process_logs` returns **chronological (reversed)** order despite the DESC SQL. Every downstream consumer assumes oldest-first. The reverse happens in app code, not SQL.
3. Eviction detection (in `get_process_logs_with_eviction_info`, which `get_log_tail` uses for `truncated`) only triggers when `limit` is set AND `rows.len() >= limit`; the count subquery must use the *limitless* query's params (no limit param appended).
4. `after_log_id` and `since_timestamp` apply only when `Some` (`Some(0)` still filters), and both use `> ?` (strict).
5. A command's latest run is its highest `run_id`, never "rows after the newest launch marker": a previous instance's monitor can write rows after the new `process_start_initiated`. `process_start_initiated (3)`, not `process_started (5)`, starts a run.
6. Hidden log types in console output: 3 and 5 produce no line. 4 and 6 render as bracketed system messages using their `content`.
7. `console_log_row` reads `row.content` for system/stdout/stderr even though `content` is nullable; a `None` renders as the empty string (in practice stdout/stderr always have content).
8. Two prefix mechanisms (`prefix` string vs `enable_app_name_prefix`) — `logs` uses the latter, `watch` the former; both render `[cmd] ` when used alone.
9. `LatestRunFilter.filter` is **stateful** and designed to be called repeatedly across streaming batches; the map is not reset between calls.
10. Exact user-facing strings (for tests): `-- showing the last <N> lines; use --count to see more --`, `No logs found for service '<name>' in project '<dir>'.`, `Cleared <n> log entries`, `No logs found to clear`, `Clearing logs for project: <dir>`.
