# Start flow

Scope: the `start` / `check-start` command path, the monitor process, transient vs configured services, success/failure detection, and the `process_tree` / `process_alive` helpers. The Rust implementation lives under `rust/src/`. `src/...` line references name files in the original Node/TypeScript implementation, which has been removed; they are historical pointers only.

## 1. High-level architecture

There are **two OS processes** per launched service:

1. **CLI process** (`candle start ...`) — resolves config, kills existing instances, spawns the monitor, then blocks watching the SQLite log table until it sees a success/failure marker and prints a result line. In non-interactive mode (or with `--bg`) it then prints a `Run 'candle logs ...' to see logs.` hint and exits; in interactive mode (or with `--watch`) it streams the new launch's logs until Ctrl+C (`watch_started_services`). Either way the service keeps running without it.
2. **Monitor process** (`candle --monitor`) — the same `candle` executable re-invoked in monitor mode: a detached, long-lived process that actually spawns the user's shell command, pipes its stdout/stderr into the SQLite `process_output` table, owns the DB `processes` row lifecycle, optionally feeds stdin, and exits when the service exits.

Communication between the two is **only** through the SQLite database (`candle.db`) plus a one-shot JSON handshake over the monitor's stdin.

Data flow:
```
candle start NAME
  → handle_start_command          (rust/src/start/start_command.rs)
    → start_one_service           (rust/src/start/start_one_service.rs)
        - acquire per-service start lock (rust/src/start/service_lock.rs)
        - check-start dedup
        - resolve ServiceConfig (transient or from config file)
        - handle_kill_command (kill existing)
        - wait_for_pids_to_exit (drain the previous instance, max 2s)
        - save_process_log(process_start_initiated)
        - launch_monitor            (rust/src/start/launch.rs)
            → spawn `candle --monitor` detached, write LaunchInfo JSON to its stdin, end stdin
              ┌─────────────── monitor process ───────────────┐
              │ candle --monitor                              │
              │   read launch info (stdin JSON or flags)       │
              │   monitor::run spawns `sh -c <shell>`          │
              │   create_process_entry()                       │
              │   500ms grace period                           │
              │   save_process_log(process_started / _failed)  │
              │   child exits → save_process_log(process_exited)│
              │   delete_process_entry()                       │
              └────────────────────────────────────────────────┘
        - poll log table for process_started / process_start_failed (10s timeout)
        - print "[Started process ...]"
        - (lock released)
  → watch_started_services, or print the `candle logs` hint (main.rs)
```

## 2. CLI surface

Two commands share `cmd_start` in `rust/src/main.rs` (originally `src/main-cli.ts:102-135`):

- `start [name...]` (alias `run [name...]`) → `handle_start_command` with `check_start = false`.
- `check-start [name...]` → `handle_start_command` with `check_start = true`.

Options (both):
- `--shell <string>` — shell command for a **transient** process.
- `--root <string>` — root dir for a transient process.
- `--enable-stdin` (boolean) — enable DB-driven stdin feeding.
- `--project-dir <dir>`: explicit project scope; must be a project with its own config (`configured_project_dir_or_exit`).

`start` only: `--watch` / `--bg` (force interactive / non-interactive; both together is an error) and `--exit-after-ms` (for the post-launch watch). `check-start` never watches.

Positional `name...` becomes `command_names`.

## 3. `handle_start_command` (`rust/src/start/start_command.rs`; originally `src/start-command.ts:25-63`)

`handle_start_command(conn, StartCommandOptions { project_dir, command_names, shell, root, enable_stdin, check_start }) -> Result<Vec<String>, CandleError>` (returns the started names).

1. `command_names = opts.command_names` (possibly empty).
2. **If no `--shell`**: `command_names = resolve_command_names_or_all(project_dir, command_names)` — if names are empty, loads **all** configured service names from `.candle.json`; raises `UsageError('No services configured in .candle.json')` if config has zero services (originally `configFile.ts:259-269`).
3. **If `--shell` is set** (transient): require exactly one name, else `UsageError('Exactly one service name is required when using --shell')`. Call `start_one_service` once with `shell/root/enable_stdin/check_start`.
4. **Else**: loop over resolved names, calling `start_one_service` for each (sequentially). Transient flags are NOT passed in this branch (`enable_stdin: false`).

## 4. `start_one_service` (`rust/src/start/start_one_service.rs`; originally `src/start/startOneService.ts:45-194`)

`start_one_service(conn, RunOptions { command_name, project_dir, shell: Option, root: Option, enable_stdin: bool, check_start: bool })`. Returns a `StartResult { project_dir, service_name }`.

### 4.0 Per-service start lock (`rust/src/start/service_lock.rs`)

Step 0 is `service_lock::acquire(project_dir, command_name)`, held (as the `ServiceStartLock` guard) until `start_one_service` returns. `start` is kill-then-launch, so without it two concurrent starts of the same service could each see "nothing running" (or each kill the same old instance) and each launch a new one, leaving duplicates. With it, the second start sees the first launch's row and kills it (or `check-start` skips it).
- The lock is a blocking advisory `flock(LOCK_EX)` (retried on `EINTR`) on `<state dir>/locks/start-<hex>.lock`, where `<hex>` is the 16-digit FNV-1a 64-bit hash of `project_dir + '\0' + service_name` (`lock_path`). The `locks/` dir is created on demand.
- The kernel releases it if the CLI dies. Rust opens files close-on-exec, so the detached monitor never inherits it.
- Failure to acquire → `Generic("Failed to acquire start lock: <e>")`.
- The lock is taken before the check-start dedup, so both checks and launches are serialized per service.

### 4.1 check-start dedup — runs BEFORE config resolution

```
if check_start {
  if command_name is empty → UsageError('Command name is required');
  existing = find_processes_by_command_name_and_project_dir(command_name, project_dir);
  not_killed = existing.filter(killed_at is None);
  running = filter_alive_processes(not_killed);
  if running.len() > 0 {
    println!("[Service '{command_name}' is already running]");
    return { project_dir, service_name: command_name };
  }
}
```

Subtlety: dedup uses **both** `killed_at IS NULL` filtering **and** a liveness probe (`filter_alive_processes`). Reboots/external kills leave `killed_at=NULL` rows whose PIDs are dead; without the liveness check, `check-start` would wrongly skip. `filter_alive_processes` also **deletes** the dead rows as a side effect. Done before config resolution so dedup works for transient names not in config.

### 4.2 Resolve `ServiceConfig`

- Transient (`shell` set): require `command_name`; validate `root` with `is_valid_root_path` (absolute OK; relative must not start with `..` after normalize) else `UsageError('Invalid root path: "<root>". Root must be an absolute path or a relative path within the project.')`. Build `ServiceConfig { name, shell, root, enable_stdin }`.
- Configured: `get_service_config_by_name(command_name, Some(project_dir))` (`rust/src/config/file.rs`) — exact match by name, else **loose substring matching** that walks up directories matching `root` (originally `configFile.ts:276-349`); raises `MissingServiceWithNameError` (message `No service '<name>' configured for directory: <projectDir>`) if not found.

### 4.3 Kill existing

`handle_kill_command(conn, project_dir, [service.name], quiet_failure = true, quiet = false)`. Always kills any current instance before starting (so `start` = restart). See §8.

The kill waits for the old shell (escalating to SIGKILL after 5s, §8), but its monitor exits a moment later, so the old instance may still be shutting down. `start_one_service` therefore snapshots the non-killed entries' `pid` and `log_collector_pid` (`previous_instance_pids`) **before** killing, then calls `wait_for_pids_to_exit(pids, PREVIOUS_INSTANCE_DRAIN_TIMEOUT = 2s)` — a 20ms signal-0 poll — before recording `process_start_initiated`. Otherwise the dying shell's last output and its monitor's `process_exited` row land in `process_output` *after* the new launch row, and every log consumer that uses that row's id as the launch boundary (`LatestExecutionLogFilter`, §`watch-wait.md`) replays them as the new instance's output. The wait is bounded so a service that ignores SIGTERM can't block a start; `LatestExecutionLogFilter` covers that residual case by refusing to attribute a `process_exited` row to a launch that hasn't logged `process_started` yet.

### 4.4 Set up log watch position

- Create `LogIterator::new(project_dir, [name])` (`rust/src/logs/log_iterator.rs`), call `reset_to_latest_log_message(conn)` (sets `current_log_id` to the id of the newest existing matching log, or `None`).
- `initial_log_position = log_iterator.copy()` — kept to fetch "recent logs" for an error message later.
- `save_process_log(conn, name, project_dir, ProcessStartInitiated, None)` — inserted by the **CLI**, not the monitor.

### 4.5 Database path

The monitor is handed `candle_db_path()`, i.e. `<state_dir>/candle.db`. (Historically this step chose between the Node and Rust log collectors; there is only one monitor now.)

### 4.6 Launch

`launch_monitor(&MonitorLaunchInfo { command_name, project_dir, shell, root, enable_stdin, database_path })`. A spawn/write error → `Generic("Failed to launch monitor process: <e>")`. See §5.

### 4.7 Success / failure detection

A single synchronous poll loop (the Node original raced two promises):
- Every `POLL_INTERVAL` (100ms), `log_iterator.get_next_logs(conn, None)`:
  - on `process_started` → break (success).
  - on `process_start_failed` → `recent_logs = initial_log_position.get_next_logs()`; return `CandleError::ProcessStartFailed { command_name, recent_logs }` (`recent_logs` is the rows' `content` joined with `\n`).
- After `START_TIMEOUT` (10s) with neither → `Generic('Process failed to start (timed out while waiting)')`.

### 4.8 Success output

```
let launch_dir = crate::dirs::resolve_launch_dir(&project_dir, service.root.as_deref());
output::out(&format!("[Started process '{}'] $ {}", service.name, service.shell));
output::out(&format!("[With root directory: {launch_dir}]"));
```

`resolve_launch_dir` (`rust/src/dirs.rs`, shared with `list`): an absolute `root` replaces `project_dir`, a relative one is joined onto it, an empty/absent one means `project_dir`; the result is lexically normalized (`./sub` → `<project>/sub`).

These exact strings are test-observable. Returns `{ project_dir, service_name }`.

## 5. `launch_monitor` (`rust/src/start/launch.rs`)

- **Path resolution** (`resolve_monitor_path`): `std::env::current_exe()` — the monitor is this very binary, so there is nothing to locate and nothing that can be missing. `CANDLE_MONITOR_PATH` overrides it for tests.
- **Launch**: command = that path, args = `["--monitor"]`, stdin piped, stdout/stderr null, `setsid` via `pre_exec`. Then write the launch-info JSON to stdin and drop the handle to close it.

`MonitorLaunchInfo` is passed **only via stdin JSON**, never via argv (the argv form exists for manual debugging). `serde_json::to_string` produces a single line with **no trailing newline**.

### Subtleties / platform notes
- `setsid` puts the child in a **new session/process group**. This is what lets the monitor outlive the CLI. The CLI does not wait on or kill the child (`std::process::Child`'s drop does neither); the parent simply returns and the monitor is reparented to init when the CLI exits.
- Only stdin is piped; stdout/stderr are `/dev/null`. The CLI reads nothing back from the monitor. It just writes the JSON and closes stdin.
- `launch_monitor` returns once `Command::spawn()` succeeds and the JSON is written.

## 6. `MonitorLaunchInfo` (`rust/src/monitor/launch_info.rs`)

```
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MonitorLaunchInfo {
  command_name: String,
  project_dir: String,
  shell: String,
  #[serde(default)] root: Option<String>,
  #[serde(default)] enable_stdin: bool,
  database_path: PathBuf,
}
```

This JSON shape (camelCase keys: `commandName`, `projectDir`, `shell`, `root`, `enableStdin`, `databasePath`) is the wire contract over stdin.

## 7. Monitor mode: `candle --monitor` (`rust/src/cli/monitor_mode.rs`, `rust/src/monitor/{launch_info,run}.rs`)

### 7.1 Reading launch info
- If no flags beyond `--monitor` are passed (the production path, since the launcher passes none) → read launch info as JSON from stdin.
- Else parse flags (`--flag value` or `--flag=value`): `--command-name` (required), `--project-dir` (required), `--shell` (required), `--root`, `--enable-stdin` (bool, default false), `--database-path`. `project-dir` is resolved to an absolute path; `database-path` defaults to `<state_dir>/candle.db`. An unknown flag or missing required flag prints an error and exits 1.

Reading stdin as JSON: `read_launch_info_from_stdin` reads all of stdin to **EOF** and parses the trimmed text as one JSON object; a read or parse error prints `Error: failed to ... launch info from stdin` and exits 1. Because it waits for EOF, the parent must close stdin. (The Node collector instead read the first complete line, which with no trailing newline also only completed at EOF.)

`main.rs` checks for `--monitor` anywhere in argv before any other dispatch and calls `run_monitor_mode`, which exits with the service's exit code (0 when there is none).

### 7.2 Monitor lifecycle (`monitor::run`, std threads, no async runtime)
1. `open_database_at(database_path)`; on failure print an error and exit 1.
2. Spawn `sh -c <shell>` in `launch_dir` (§7.3). On spawn error → `save_process_log(process_start_failed, "Process failed to start: <e>")` and exit 1. No `processes` row exists yet in this branch.
3. `create_process_entry({ command_name, project_dir, pid: child_pid, log_collector_pid: Some(own_pid), shell, root })` — **pid = user shell pid; log_collector_pid = the monitor's own pid** (the DB column keeps its legacy name).
4. Reader threads (stdout, stderr), an optional stdin thread, and a wait thread forward events over one channel.
5. **Grace period**: collect events for `GRACE_PERIOD_MS` (500ms), writing output lines as they arrive, then drain anything already queued. If the child exited with a code other than `Some(0)` (a nonzero code or a signal) → `save_process_log(process_start_failed, "Process failed to start: exited with code <n>"` or `"Process failed to start: stopped by a signal")`, `delete_process_entry`, return.
6. Else `save_process_log(process_started)` (no content). If it already exited with 0 during the grace period, immediately log `process_exited` and delete the row.
7. Main loop: write output lines until the `Exit` event, calling `maybe_run_cleanup` about every 60s (`CLEANUP_INTERVAL_MS`). Then `save_process_log(process_exited, "Process exited with code <n>")` (or `"Process was stopped"` when killed by a signal), `delete_process_entry`, and return the exit code.

### 7.3 Supervising the service (`monitor::run`)
- `launch_dir = root ? Path::new(project_dir).join(root) : project_dir`. (Note: this uses Rust's `Path::join`, where an **absolute** root replaces the base, so in practice the cwd matches `resolve_launch_dir` except for lexical normalization. The Node original concatenated unconditionally.)
- Run `sh -c <shell>`, cwd = `launch_dir`, stdout/stderr piped, stdin piped only when `enable_stdin` (else null). Each stdout/stderr **line** → `save_process_log(conn, command_name, project_dir, Stdout|Stderr, Some(line))`.
- If `enable_stdin`: `clear_stdin_messages(command_name, project_dir)`, then a thread with its own connection, every `STDIN_POLL_INTERVAL_MS` (500ms) until the child exits:
  - `msg = pop_stdin_message(command_name, project_dir)` (oldest row, deletes it); if present, write `msg.data` bytes to the subprocess stdin (`encoding` is ignored). A write error stops the thread.

## 8. Kill path (used by start, and as `kill`/`stop`)

`handle_kill_command` (`rust/src/kill/mod.rs`): with names → dedupe set, `kill_by_command_name` each; without names → kill all `find_running_processes_by_project_dir`. `quiet_failure` suppresses the "No running processes" messages.

`kill_one_running_process` (`rust/src/kill/mod.rs`):
- `kill_process_tree_and_wait(pid, KILL_GRACE_PERIOD = 5s)` → `KillOutcome::{Terminated, Escalated, ProcessNotFound, Error}`.
- `Terminated` / `Escalated`: on `Escalated`, note `[Process '<name>' (PID <pid>) did not exit 5s after SIGTERM; sent SIGKILL]` on stderr; print `[Killed '<name>' process with PID: <pid>]` (unless quiet). If `killed_at` exists and is >5min old → `delete_process_entry` (+ warn `[Cleaning up stale process entry ...]`); else `update_process_killed_at` to now (unix seconds).
- `ProcessNotFound`: warn `[Cleaning up stale process entry ...]`, `delete_process_entry`.
- `Error`: print `Error killing process '<name>' with PID: <pid>`.

`kill_process_tree_and_wait`: `kill_process_tree(pid)`, then wait up to the grace period for the root to exit; if it hasn't, re-snapshot the tree, `SIGKILL` it children-first, and wait up to `SIGKILL_WAIT` (1s).

`kill_process_tree` (`rust/src/kill/mod.rs`): panics on pid <= 0; `get_process_tree(pid)`; if empty → `ProcessNotFound`; iterate **reversed** (children first, root last) sending `SIGTERM`; ESRCH ignored; other errors → warn + `has_error`. Returns `ProcessNotFound` if every kill found nothing, `Error` if any non-ESRCH error, else `Success`. It never waits. Full detail in [kill-restart.md](kill-restart.md).

## 9. `process_tree` (`rust/src/process_tree.rs`)
- `get_process_tree(root_pid) -> Vec<i64>` — worklist (stack) collecting root + all descendants, root first.
- `get_child_pids(parent_pid)`: macOS → `pgrep -P <pid>`; Linux → `ps -o pid --no-headers --ppid <pid>`; other platforms → `[]`.
- `run_command_for_pids`: `std::process::Command` with stdin/stderr null and stdout captured, split lines → parse ints, drop non-numeric; on spawn error → `[]`. Windows is unsupported here (returns no children).

## 10. `process_alive` (`rust/src/process_alive.rs`)
- `is_process_alive(pid)`: `pid <= 0` → dead; else signal-0 probe `libc::kill(pid, 0)` → `0` = alive, `EPERM` = alive (other user), `ESRCH`/other = dead.
- `filter_alive_processes(conn, entries)`: keep entry if `log_collector_pid` alive **OR** `pid` alive; otherwise `delete_process_entry(conn, command_name, project_dir, pid)` and drop it.

## 11. Database (`rust/src/db/mod.rs`, `rust/src/db/process_table.rs`)
SQLite at `<state_dir>/candle.db`. `state_dir` (`rust/src/dirs.rs`; originally `src/dirs.ts:9-22`): `$CANDLE_DATABASE_DIR`, else `$XDG_STATE_HOME/candle`, else `~/.local/state/candle`. Pragmas: `journal_mode=WAL`, `busy_timeout=30000` (multi-process access). Schema (`create ... if not exists`, run on every open; see [database.md](database.md)):

```sql
create table processes(
  id integer primary key autoincrement,
  command_name text not null,
  project_dir text not null,
  pid integer not null,
  log_collector_pid integer,
  start_time integer not null,
  created_at integer not null default (strftime('%s','now')),
  killed_at integer,
  shell text,
  root text);
create table process_output(
  id integer primary key autoincrement,
  command_name text not null,
  project_dir text not null,
  content text,
  log_type integer not null,
  timestamp integer not null default (strftime('%s','now')));
create table process_last_cleanup(timestamp integer not null);
create table stdin_messages(
  id integer primary key autoincrement,
  command_name text not null,
  project_dir text not null,
  data text not null,
  encoding text not null default 'utf8',
  created_at integer not null default (strftime('%s','now')));
create index idx_process_output_command_name on process_output(command_name);
create index idx_process_output_project_dir on process_output(project_dir);
create index idx_process_output_lookup on process_output(project_dir, command_name, timestamp desc, id desc);
create index idx_stdin_messages_lookup on stdin_messages(project_dir, command_name, id);
```

Key SQL used by start-flow:
- Insert process: `create_process_entry` sets `start_time` = now in unix seconds; `root` → `NULL` when absent. (`created_at` via default.)
- `save_process_log`: `insert into process_output(command_name, project_dir, content, log_type) values(?,?,?,?)`.
- `find_processes_by_command_name_and_project_dir`: `select * from processes where command_name=? and project_dir=?`.
- `update_process_killed_at`: `update processes set killed_at=? where command_name=? and project_dir=? and pid=?`.
- `delete_process_entry`: `delete from processes where command_name=? and project_dir=? and pid=?`.
- stdin (`rust/src/db/stdin_messages.rs`): `pop_stdin_message` = `select * ... order by id asc limit 1` then `delete ... where id=?`; `clear_stdin_messages` = `delete ... where command_name=? and project_dir=?`.

### `ProcessLogType` (`rust/src/logs/log_type.rs`) — integer values are persisted, must match exactly:
`stdout=1, stderr=2, process_start_initiated=3, process_start_failed=4, process_started=5, process_exited=6`.

### Log polling (`LogIterator` + `get_process_logs`, `rust/src/logs/{log_iterator,process_logs}.rs`)
`get_process_logs` builds a query with `after_log_id` / `limit`, returning rows reversed into chronological order. `start_one_service`'s poll loop calls `log_iterator.get_next_logs(conn, None)` (rows with `id > current_log_id`, advancing the cursor), then sleeps 100ms. `reset_to_latest_log_message` seeds `current_log_id` to the newest existing id so the CLI only reacts to logs produced after launch begins.

## 12. Crates used (replacing the original npm deps)
- `@facetlayer/subprocess` (`startShellCommand`, `Subprocess`, line-buffered stdout/stderr, `waitForStart`/`waitForExit`, detached spawn) → `std::process::Command`; `std::io::BufReader::lines()` on reader threads for line splitting; detach via `libc::setsid` in `pre_exec`.
- `@facetlayer/parse-stdout-lines` (`unixPipeToLines`, splits on `\n`, emits trailing partial line on EOF) → `BufRead::lines()`, which also yields a trailing partial line. The monitor's stdin handshake reads to EOF instead of line-by-line.
- `@facetlayer/sqlite-wrapper` (schema migrations, WAL) → `rusqlite`. Sets `PRAGMA journal_mode=WAL` and `PRAGMA busy_timeout=30000`.
- `yargs` → the hand-rolled parser in `rust/src/cli/parser.rs`.
- `@modelcontextprotocol/sdk`: not in start-flow (mcp command).
- Liveness/process tree/locking: `libc` (`kill`, `setsid`, `flock`) plus `pgrep`/`ps`.

## 13. Subtle behaviors
1. **stdin handshake needs EOF**: the monitor reads stdin to EOF before parsing, so the parent must close stdin after writing.
2. **Detached, never joined**: the monitor runs in its own session (`setsid`) and the CLI never waits on it.
3. **check-start dual liveness check**: must filter `killed_at IS NULL` *and* probe PIDs, *and* delete dead rows. Missing the probe causes false "already running".
4. **`filter_alive_processes` checks `log_collector_pid` first, then `pid`** — either alive keeps the row.
5. **Two PIDs per row**: `pid` = user shell; `log_collector_pid` = the monitor process. Kill targets `pid` (the shell tree) via SIGTERM, children-first, escalating to SIGKILL for any process in the tree that outlives the 5s grace period.
6. **Timestamps are unix seconds** (`strftime('%s','now')` and `SystemTime` seconds), not ms. The 5-minute stale check compares seconds.
7. **Grace period 500ms**: success = `process_started` only after surviving 500ms, or exiting with code 0 within it (a command that exits 0 within 500ms is still treated as started, then immediately logged as exited). A nonzero exit or a signal within 500ms → `process_start_failed` + row deleted.
   **The deadline must not discard queued events.** The grace loop reads stdout/stderr/exit events off a channel, writing each output line to `process_output` as it arrives. When the deadline expires it drains what has already arrived with `try_recv` before deciding. Without that drain, a process that dies instantly — which still emits its error output *before* its exit event — could be reported as started purely because persisting those lines took longer than the remaining window, leaving the exit event unread. The decision is about what the process did, not about how fast its logs were written.
8. **Spawn-failure branch creates no process row** (the spawn fails before `create_process_entry`), while the grace-period-failure branch deletes the row it created.
9. **10s CLI timeout** rejects with `'Process failed to start (timed out while waiting)'` independent of the monitor — the monitor keeps running even if the CLI times out.
10. **launchDir**: the banner uses `resolve_launch_dir` (normalized); the monitor's cwd uses a plain `Path::join`. Both let an absolute root win, so they differ only in normalization. (In Node the monitor concatenated even absolute roots.)
11. **Start lock**: concurrent starts of one service serialize on the `flock` from §4.0; different services (or projects) never contend.
12. **Exact output strings** (the two-line start banner `[Started process '<name>'] $ <shell>` / `[With root directory: <dir>]`, `[Service '<name>' is already running]`, `[Killed '<name>' process with PID: <pid>]`, cleanup/error variants) are asserted by tests — reproduced verbatim including backticks and brackets.
13. **Config resolution order**: `.candle.json` then deprecated `.candle-setup.json`; loose substring + directory-aware matching for service names; walk up parent dirs to find config.
