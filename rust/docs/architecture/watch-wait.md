# Watch & wait-for-log

This covers the `candle watch` and `candle wait-for-log` CLI commands and their supporting log-tailing infrastructure. Both poll a SQLite `process_output` table for new log rows; they differ in what they do when a new row appears.

The Rust implementation lives in `rust/src/commands/{watch,wait_for_log}.rs`, with the shared tailing machinery in `rust/src/logs/log_iterator.rs`, `rust/src/logs/process_logs.rs`, `rust/src/logs/console_log.rs`, and the filters in `rust/src/log_filters/`. It was ported from the original Node implementation under `src/`. That implementation has been removed; the `src/...` files cross-referenced below are historical pointers only, not files in the repo.

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
    timestamp integer not null default (strftime('%s', 'now')),  -- UNIX SECONDS, not ms
    run_id integer                                 -- Rust-only: the run's process_start_initiated id
)
```
`run_id` ties each row to one launch; see [logs.md](logs.md) §1 "Runs". A command's latest run is its highest `run_id`.
Index used for tailing (`database.ts:49`):
`create index idx_process_output_lookup on process_output(project_dir, command_name, timestamp desc, id desc)`.

**Critical:** `timestamp` is stored in **whole seconds** (`strftime('%s','now')`), but all wall-clock math uses millisecond clocks. The one place this matters is the recency window — see §4.

### 1.2 `ProcessLog` row shape (`src/logs/processLogs.ts:13-20`)
`{ id: number, command_name: string, project_dir: string, content?: string, log_type: number, timestamp: number }`; the Rust `ProcessLog` adds `run_id: Option<i64>`.

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

`LogSearchOptions = { projectDir, commandNames?, limit?, sinceTimestamp?, afterLogId? }` in the original. The Rust struct adds `min_log_id`, `log_types`, `latest_launch_only` and `run_id` (see [logs.md](logs.md) §3-6). The tailing `LogIterator` leaves them at their defaults; `wait-for-log` uses `log_types` + `latest_launch_only` for its one-off lifecycle check (§8.2) and `get_log_tail` for its recent-logs dump.

Query construction (note the table alias `po`):
- 1 command name: `select po.* from process_output po where po.project_dir = ? and po.command_name = ?`
- N command names: `... and po.command_name in (?, ?, …)`
- No command names (defined projectDir): `... where po.project_dir = ?`
- If `sinceTimestamp` set: append ` and po.timestamp > ?`
- If `afterLogId != null`: append ` and po.id > ?` (the original JS `!= null` excludes both `null` and `undefined`; `0` is a valid id and passes — the Rust side models this as `Option<i64>` where `Some(0)` applies the filter and `None` does not)
- Always append: ` order by po.timestamp desc, po.id desc`
- If `limit` set: ` limit ?`

So the DB returns **newest-first**. `getProcessLogs` (`processLogs.ts:74`, Rust `logs/process_logs.rs`) then reverses the rows to hand back **chronological (oldest-first)** order. Every consumer assumes oldest-first.

`getProcessLogsWithEvictionInfo` also computes `logsWereEvicted` by re-running the query wrapped in `select count(*) as total from (<sql without limit>)` and comparing to the returned count. The tailing loops use `getProcessLogs`; in Rust, eviction info backs `get_log_tail`'s `truncated`.

Subtle ordering detail: ordering by `(timestamp desc, id desc)` then reversing is NOT the same as ordering by `id asc` when multiple rows share a timestamp (likely, since timestamps are 1-second granularity). The implementation orders descending by `(timestamp, id)`, then reverses the vector.

## 3. `LogIterator` (`src/logs/LogIterator.ts`, Rust `logs/log_iterator.rs`)

Stateful cursor over `get_process_logs`. Fields: `project_dir`, `command_names`, a default `limit`, and `current_log_id: Option<i64>` (starts `None`).

- `peek_next_logs(conn, limit_override)`: queries with `after_log_id: current_log_id` and `limit_override.or(default limit)`. When `current_log_id` is `None` → no `id >` filter → fetches the most recent `limit` rows.
- `get_next_logs(conn, limit_override)`: calls peek; if non-empty, sets `current_log_id` to the last row's id (last = newest since chronological). Returns the batch.

Key behavior: because the cursor advances by **max id seen**, and the query filters `id > current_log_id`, each `get_next_logs` returns strictly new rows. The limit caps batch size; `watch_process` passes `Some(INITIAL_LOG_COUNT)` (100) only on the first call and `None` afterward.

## 4. `LatestRunFilter` (Rust `log_filters/latest_run_filter.rs`)

Trims a log stream to "only each command's latest run, optionally within a recency window". Full description in [logs.md](logs.md) §7. In short:
- `LatestRunFilter::new(recent_window_ms)`: with a window, `min_timestamp = (now_unix_millis - window_ms) as f64 / 1000.0` — **converts ms→seconds to match DB timestamps**, no rounding.
- `seed_latest_runs(conn, project_dir, command_names)`: learn each command's latest `run_id` from the DB (`latest_run_ids`) before filtering existing rows.
- `filter(logs)`: keep a row iff its `run_id` equals the highest seen for its command (after counting this row) and `timestamp >= min_timestamp`. Order-independent, so a previous instance's rows that land after a restart's launch marker are dropped.

It is a mutable struct reused across poll iterations: a row from a newer run moves the filter on to that run.

This replaces the Node original's `LatestExecutionLogFilter` (which treated every row after the newest `process_start_initiated` as the latest launch, with `show_logs_from_previous_launch` / `only_show_after_recent_launch` modes) and `ExecutionStatusTracker` (which `watch` used to count running services). Neither exists in the Rust code.

## 5. (removed) `ExecutionStatusTracker`

`watch` now counts still-running services from the `processes` table at exit (§7.4).

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

### 7.1 CLI definition (`cmd_watch` in `rust/src/main.rs`; originally `src/main-cli.ts:154-164`)
`watch [name...]` — positional `name` (variadic, strings). Options `--exit-after-ms <ms>` (accepted by the hand-rolled parser but not listed in `watch --help`, so effectively hidden) and `--project-dir <dir>` (must contain its own config, `require_own_config`). Unknown flags are rejected (`Unknown argument`).

### 7.2 Agent-mode disabling (`run_context.rs`; originally `src/runContext.ts`, `main-cli.ts:434-441`)
`is_run_by_agent` is derived from the coding-agent marker environment variables (see `run_context`). In the `watch` case, when run by an agent `cmd_watch` prints to **stderr** and exits with code **1**, before touching the database:
```
Error: 'watch' blocks and is not available in agent mode. Use 'candle logs' to view process output.
```
That exact stderr string and exit code 1 are load-bearing. Agent mode also hides the `watch` line in grouped help (`cli/help.rs`), which is cosmetic.

`is_run_by_agent` is evaluated once: agent mode iff **any** of `CLAUDECODE` / `GEMINI_CLI` / `CURSOR_AGENT` is present and non-empty. The empty string means not-set; `"0"`/`"false"` are non-empty and therefore still count.

### 7.3 `handle_watch(conn, cwd, command_names, exit_after_ms)` (`rust/src/commands/watch.rs`; originally `src/watch-command.ts`)

`watch` **never launches processes** (the Node original ran `startOneService({ checkStart: true })` for each name first; the Rust command does not).
1. `project_dir = find_project_dir(cwd)` (searches up for the config file from the scope's base dir).
2. **No names**: print `Watching all processes in this project.` and watch every command in the project (the empty name list is passed straight through, so services that haven't launched yet show up when they do). Names are not expanded from config.
3. **With names**: `cmd_watch` first rejects an unknown name with `assert_valid_command_names` (`No service '<name>' configured for directory: <dir>`); then each must be running (`killed_at is null` rows filtered through `filter_alive_processes`, which also deletes dead rows). Otherwise → `UsageError("Process '<name>' is not running. Start it with: candle start <name>")` (stderr, exit 1). Then print the header:
   - 1 name: `Watching process '<name>'`
   - N names: `Watching <N> processes:` then for each `  - '<name>'`
4. Print `Press Ctrl+C to stop watching.` and a blank line.
5. Call `watch_process(conn, project_dir, command_names, exit_after_ms, Some(RECENT_LOG_WINDOW_MS))`.

`watch_started_services(conn, project_dir, names, exit_after_ms)` is the same loop used by interactive `start`/`restart`: it prints `[Now watching console logs. Press Ctrl+C to stop watching.]` and a blank line, then calls `watch_process` with no recency window. The seeded filter shows the whole fresh launch (the latest run) and nothing from earlier runs.

### 7.4 `watchProcess` — the tail loop
Constants: `INITIAL_LOG_COUNT = 100`, `POLL_INTERVAL = 200` (ms), `RECENT_LOG_WINDOW_MS = 10_000`.

`watch_process(conn, project_dir, command_names, exit_after_ms, recent_window_ms)`:
- `is_blended = command_names.len() != 1` (so the watch-everything case, with zero names, is blended too).
- `LogIterator::new(project_dir, command_names)`.
- `filter = LatestRunFilter::new(recent_window_ms)` (`watch`: 10_000ms; `watch_started_services`: none), then `filter.seed_latest_runs(conn, project_dir, command_names)`.
- `initial_logs = iterator.get_next_logs(conn, Some(100))`; this advances the cursor to the newest of those 100, which are the first printed batch — there is no double-fetch.

- Install `SIGINT`/`SIGTERM` handlers (`libc::signal`) that set a static `STOP: AtomicBool` (reset to false at the start of each call).
- `--exit-after-ms`: if `exit_after_ms > 0`, a deadline is computed. There is no timer thread; the loop checks the deadline each iteration and, once passed, prints `console_log_system_message(Pretty, 'Exiting watch mode after <exit_after_ms>ms timeout')` (→ `[Exiting watch mode after Nms timeout]`) and breaks.
- `print_batch(logs)`: `filter.filter(logs)`, then per filtered log `console_log_row(log, { Pretty, prefix })` where `prefix = is_blended ? "[<command_name>] " : None`.
- Print the initial batch once.
- Loop until `STOP` or the deadline: `print_batch(iterator.get_next_logs(conn, None))` (no limit), then sleep 200ms.
- After loop: `running = count_running_services(conn, project_dir, command_names)`: the number of distinct watched services (all in the project when no names) with a live `processes` row (`find_running_processes_by_project_dir` + `filter_alive_processes`):
  - `== 1`: `consoleLogSystemMessage(format, 'Stopped watching. Process is still running in the background.')`
  - `> 1`: `Stopped watching. <N> processes are still running in the background.`
  - `0`: nothing.
- Restore `SIG_DFL` for `SIGINT`/`SIGTERM`.

Subtlety: the function returns normally (no forced exit); the process exits naturally. Both `STOP` and the deadline are checked only at the top of each iteration, so actual stop latency is up to `POLL_INTERVAL` (200ms) plus one fetch.

## 8. `wait-for-log` command (`rust/src/commands/wait_for_log.rs`)

### 8.1 CLI definition (`cmd_wait_for_log` in `rust/src/main.rs`; originally `src/main-cli.ts:165-178`)
`wait-for-log [name]` — positional name(s), all passed through as `command_names`. Required option `--message <string>` (missing → stderr `Missing required argument: message`, exit 1). Option `--timeout <number>` in seconds, default `30`. Option `--project-dir <dir>`. Strict options. **Not** disabled in agent mode. Names are validated with `assert_known_service_names` (same rule as `logs`: stored logs, a process row, or a config entry), so an unknown name prints `No service '<name>' configured for directory: <dir>` to stderr and exits 1. Transient names with history are allowed.

Dispatch: resolve `project_dir`, call `handle_wait_for_log(conn, project_dir, command_names, message, timeout_ms = timeout * 1000)`, and exit `1` if `!result.success`. So **exit code 0 on success, 1 on failure.** Timeout is converted seconds→ms here.

### 8.2 `handle_wait_for_log` (`rust/src/commands/wait_for_log.rs`; originally `src/wait-for-log-command.ts`)
Constants: `POLL_INTERVAL = 200` (ms), `LOG_COUNT_SEARCH_LIMIT = 1000`, `RECENT_LOG_LINES = 20`, `LIVENESS_CHECK_EVERY = 5` (polls). The 30s default lives in the CLI layer.

"Running" (`is_any_running`) means a `processes` row for the project with `killed_at` null whose pid is alive (`filter_alive_processes`), for any of the named services (any service when no name is given). The monitor inserts the row before it writes `process_started` and deletes it just after `process_exited`, so once the start is reported a missing row means the service is gone.

Return shape: `WaitForLogResult { success: bool }` (the TS version also carried an unread `message`).

Algorithm:
1. `log_filter = LatestRunFilter::new(None)` (**no recency window**), seeded with `seed_latest_runs`. `LogIterator::with_limit(project_dir, command_names, 1000)`; `initial_logs = log_filter.filter(iterator.get_next_logs())` (advances the cursor to newest). Because every row carries its run, this is right even when the launch itself is older than the 1000-row window.
2. Scan `initial_logs`: if any `log.content?.includes(message)` (substring match; `content` may be null → skipped): print `Found message "<message>" in existing logs.` and return `{ success: true }`. This runs first, so a run that has already finished still satisfies the wait if it printed the message.
3. Gather state: `has_run` = `latest_run_ids` is non-empty (the service has ever launched); the latest run's lifecycle rows = `get_process_logs { log_types: [process_started, process_start_failed, process_exited], latest_launch_only: true }`; `running = is_any_running(..)`.
4. `!has_run && !running` → fail at once (`fail_not_running`, below, without recent logs).
5. The latest run already ended (a `process_exited` or `process_start_failed` among its lifecycle rows) and nothing is running → fail at once with recent logs.
6. `start_reported = !has_run || the latest run has a process_started row`.
7. Poll loop (`timeStarted = now`):
   - Once the start is reported, every `LIVENESS_CHECK_EVERY` polls: if nothing is running, fail with recent logs. Before the start is reported the launch is still in progress, so the missing row is expected.
   - If `now - timeStarted > timeoutMs`: print `wait-for-log failed: Timed out after <timeoutMs>ms and message "<message>" not found.`, call `print_recent_logs(...)`, return `{ success: false }`.
   - `raw_logs = iterator.get_next_logs()` (limit 1000); `logs = log_filter.filter(raw_logs)`.
   - For each log: if `content?.includes(message)` → print `Found message "<message>" in logs.` and return `{ success: true }`. A `process_started` row sets `start_reported`. Else if `log_type` is `process_exited (6)` or `process_start_failed (4)` → print `wait-for-log failed: Process exited before finding message "<message>"`, call `printRecentLogs(...)`, return `{ success: false }`.
   - Sleep 200ms.

The timeout is checked at the **top** of the loop before fetching; the first check happens immediately (0 elapsed, won't trip). The exit-before-found check is per-log within a batch and is evaluated **after** the message check, so a batch where the matching line and the exit line both appear returns success if the match comes first in chronological order.

### 8.3 `print_recent_logs`
Prints on failure paths, showing only the tail of the latest run (the Node version printed up to 100 rows, earlier runs included):
- `get_log_tail({ project_dir, command_names }, RECENT_LOG_LINES)` (§6 of logs.md: the newest 20 printable rows of each command's latest run), printed as-is.
- Header: `Last 20 lines of the latest run of '<names>':` when the tail was truncated, else `Logs from the latest run of '<names>':`.
- Each row via `console_log_row` (pretty; `[name] ` prefix when there isn't exactly one name).
- Footer: `Run 'candle logs <name>' to see more.` (`candle logs` with zero or several names).

`fail_not_running` prints `wait-for-log failed: Service '<name>' is not running and message "<m>" was not found.` (`Services '<a, b>' are not running` for several names, `No service in this project is running` for none), then the recent logs when the service has a recorded run.

### 8.4 Exact output strings (test-load-bearing)
- `Found message "<m>" in existing logs.` (stdout, success)
- `Found message "<m>" in logs.` (stdout, success)
- `wait-for-log failed: Timed out after <ms>ms and message "<m>" not found.` (stdout, failure)
- `wait-for-log failed: Process exited before finding message "<m>"` (stdout, failure)
- `wait-for-log failed: Service '<name>' is not running and message "<m>" was not found.` (stdout, failure)
- `Run 'candle logs <name>' to see more.` (stdout, after the recent logs)
- `<m>` is wrapped in literal double-quotes; `<ms>` is the raw timeout in **milliseconds**.

## 9. Implementation dependencies

- **SQLite access** (`getDatabase()`, list/get/run): the Rust implementation uses `rusqlite` (synchronous), which preserves the synchronous query semantics of the original `node:sqlite` code exactly.
- **CLI parsing**: the hand-rolled parser in `rust/src/cli/parser.rs` (no `clap`); `--exit-after-ms` is simply omitted from the help text.
- **Timers/sleep**: `std::thread::sleep` for the 200ms poll and an `Instant` deadline for exit-after-ms are the only timing primitives.
- **Signals** (`SIGINT`/`SIGTERM`): `libc::signal` handlers flip a static `STOP` flag.
- No other third-party dependencies in this subsystem.

## 10. Platform / correctness gotchas

1. **Timestamp unit mismatch:** DB `timestamp` is UNIX **seconds**; wall-clock math is **ms**. The recency window divides ms by 1000 (keeping the fraction) and compares against integer seconds. The float division is reproduced without rounding.
2. **Order-then-reverse:** `ORDER BY timestamp DESC, id DESC` then reverse, not a plain `id ASC`, because second-granularity timestamps tie frequently.
3. **`afterLogId` semantics:** absent → no filter; `0` is valid and applies `id > 0`. Modeled as `Option<i64>` with `Some(0)` distinct from `None`.
4. **`content` is nullable;** a null `content` skips the substring search. Substring match is plain `str::contains` (case-sensitive, no regex).
5. **Stateful filter mutation:** `LatestRunFilter.filter` updates the per-command latest `run_id`; the same instance is reused across poll iterations in both commands. It is a mutable struct, not a pure function.
6. **Exit codes:** `wait-for-log` → exit `1` on `!success`, `0` otherwise. `watch` in agent mode → exit `1` with the exact stderr message; otherwise exits 0 naturally.
7. **stdout vs stderr:** nearly everything is stdout. Exceptions: the agent-mode watch error and the `watch` "is not running" usage error go to **stderr**.
8. **Cursor pre-advance in watch:** `get_next_logs(Some(100))` advances the cursor; the initial 100 are the first printed batch, then the loop continues from the new cursor — no double-fetch. The filter's latest runs come from `seed_latest_runs`, not from that batch.
9. **`is_run_by_agent` is evaluated once** from the agent marker vars (`CLAUDECODE` / `GEMINI_CLI` / `CURSOR_AGENT`): any one present and non-empty = agent mode. `"0"`/`"false"` are non-empty and therefore still count; only unset or empty is non-agent.

## 11. Source files

Rust modules: `rust/src/commands/watch.rs`, `rust/src/commands/wait_for_log.rs`, `rust/src/logs/log_iterator.rs`, `rust/src/logs/process_logs.rs`, `rust/src/logs/console_log.rs`, `rust/src/logs/log_type.rs`, `rust/src/log_filters/latest_run_filter.rs`.

Historical Node sources (removed from the repo): `src/watch-command.ts`, `src/watchProcess.ts`, `src/wait-for-log-command.ts`, `src/logs/LogIterator.ts`, `src/log-filters/LatestExecutionLogFilter.ts`, `src/log-filters/ExecutionStatusTracker.ts`, `src/logs/processLogs.ts`, `src/logs/buildLogSearchQuery.ts`, `src/logs/SqlBuilder.ts`, `src/logs/ProcessLogType.ts`, `src/logs.ts`, `src/runContext.ts`, `src/main-cli.ts`, `src/database/database.ts`, `src/configFile.ts`.
