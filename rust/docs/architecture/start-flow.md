# Start flow

Scope: the `start` / `check-start` command path, the monitor process, transient vs configured services, success/failure detection, and the `process_tree` / `process_alive` helpers. The implementation lives under `rust/src/`.

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
        - start_run → process_start_initiated row; its id is the run id
        - launch_monitor            (rust/src/start/launch.rs)
            → spawn `candle --monitor` detached, write LaunchInfo JSON (incl. run_id) to its stdin, end stdin
              ┌─────────────── monitor process ───────────────┐
              │ candle --monitor                              │
              │   read launch info (stdin JSON or flags)       │
              │   monitor::run spawns `sh -c <shell>`          │
              │   create_process_entry(.., run_id)             │
              │   500ms grace period                           │
              │   save_run_log(process_started / _failed)      │
              │   child exits → save_run_log(process_exited)   │
              │   delete_process_entry()                       │
              └────────────────────────────────────────────────┘
        - poll this run's rows for process_started / process_start_failed (10s timeout)
        - print "[Started process ...]"
        - (lock released)
  → watch_started_services, or print the `candle logs` hint (main.rs)
```

## 2. CLI surface

Two commands share `cmd_start` in `rust/src/main.rs`:

- `start [name...]` (alias `run [name...]`) → `handle_start_command` with `check_start = false`.
- `check-start [name...]` → `handle_start_command` with `check_start = true`.

Options (both):
- `--shell <string>` — shell command for a **transient** process.
- `--root <string>` — root dir for a transient process.
- `--enable-stdin` (boolean) — enable DB-driven stdin feeding.
- `--project-dir <dir>`: explicit project scope; must be a project with its own config (`configured_project_dir_or_exit`).

`start` only: `--watch` / `--bg` (force interactive / non-interactive; both together is an error) and `--exit-after-ms` (for the post-launch watch). `check-start` never watches.

Positional `name...` becomes `command_names`.

## 3. `handle_start_command` (`rust/src/start/start_command.rs`)

`handle_start_command(conn, StartCommandOptions { project_dir, command_names, shell, root, enable_stdin, check_start }) -> Result<Vec<String>, CandleError>` (returns the started names).

1. `command_names = opts.command_names` (possibly empty).
2. **If no `--shell`**: `command_names = resolve_command_names_or_all(project_dir, command_names)` — if names are empty, loads **all** configured service names from `.candle.json`; raises `UsageError('No services configured in .candle.json')` if config has zero services.
3. **If `--root` is set without `--shell`**: resolve each name with `get_service_config_by_name` (so an unknown name still gets `MissingServiceWithName`), then fail with `UsageError("--root only applies to transient services started with --shell. ...")` rather than silently dropping the flag.
4. **If `--shell` is set** (transient): require exactly one name, else `UsageError('Exactly one service name is required when using --shell')`. Call `start_one_service` once with `shell/root/enable_stdin/check_start`.
5. **Else**: loop over resolved names, calling `start_one_service` for each (sequentially). Transient flags are NOT passed in this branch (`enable_stdin: false`).

## 4. `start_one_service` (`rust/src/start/start_one_service.rs`)

`start_one_service(conn, RunOptions { command_name, project_dir, shell: Option, root: Option, enable_stdin: bool, check_start: bool })`. Returns a `StartResult { project_dir, service_name }`.

### 4.0 Per-service start lock (`rust/src/start/service_lock.rs`)

Step 0 is `service_lock::acquire(project_dir, command_name)`, held (as the `ServiceStartLock` guard) until `start_one_service` returns. `start` is kill-then-launch, so without it two concurrent starts of the same service could each see "nothing running" (or each kill the same old instance) and each launch a new one, leaving duplicates. With it, the second start sees the first launch's row and kills it (or `check-start` skips it).
- The lock is a blocking advisory `flock(LOCK_EX)` (retried on `EINTR`) on `<state dir>/locks/start-<hex>.lock`, where `<hex>` is the 16-digit FNV-1a 64-bit hash of `project_dir + '\0' + service_name` (`lock_path`). The `locks/` dir is created on demand.
- The kernel releases it if the CLI dies. Rust opens files close-on-exec, so the detached monitor never inherits it.
- Failure to acquire → `Generic("Failed to acquire start lock: <e>")`.
- The lock is taken before the check-start dedup, so both checks and launches are serialized per service.
- Before the per-service lock, `acquire` takes `<state dir>/locks/database.lock` **shared** (`LOCK_SH`). `erase-database` takes the same file **exclusive** for its whole live-process check and erase (`erase_database_guarded`), so a start can't launch between the check and the deletion. Starts don't block each other. The fixed order (database lock, then service lock) can't deadlock because erase takes only the database lock.
- A start that waited on an erase may hold a connection to the deleted `candle.db`. So `start_one_service` stats the connection's DB path (dev, inode) before taking the lock and again after; if they differ, it fails with `Generic("The database was erased while this start was waiting. Run the command again.")`.

### 4.1 check-start dedup — runs BEFORE config resolution

```
if command_name is empty && (check_start || shell is set) → UsageError('Command name is required');
if check_start && is_service_running(project_dir, command_name) {
  println!("[Service '{command_name}' is already running]");
  return { project_dir, service_name: command_name };
}
```

Subtlety: `is_service_running` (`rust/src/process_alive.rs`) uses **both** `killed_at IS NULL` filtering **and** a liveness probe (`filter_alive_processes`). Reboots/external kills leave `killed_at=NULL` rows whose PIDs are dead; without the liveness check, `check-start` would wrongly skip. `filter_alive_processes` also **deletes** the dead rows as a side effect. Done before config resolution so dedup works for transient names not in config.

### 4.2 Resolve `ServiceConfig`

- Transient (`shell` set): require `command_name`; validate `root` with `is_valid_root_path` (absolute OK; relative must not start with `..` after normalize) else `UsageError('Invalid root path: "<root>". Root must be an absolute path or a relative path within the project.')`. Build `ServiceConfig { name, shell, root, enable_stdin }`.
- Configured: `get_service_config_by_name(command_name, Some(project_dir))` (`rust/src/config/file.rs`) — exact match by name, else **loose substring matching** that walks up directories matching `root` (see [config.md](config.md) §9); raises `MissingServiceWithName` (message `No service '<name>' configured for directory: <project_dir>`) if not found.

### 4.2a Launch directory check

`launch_dir = resolve_launch_dir(project_dir, service.root)`. If it isn't an existing directory → `UsageError("Process '<name>' failed to start: root directory does not exist: <launch_dir>")`, before the kill below, so a bad `root` never stops a running instance. (The monitor has the same check as a fallback when spawning `sh` fails.)

### 4.3 Kill existing

`handle_kill_command(conn, project_dir, [service.name], quiet_failure = true, quiet = false)`. Always kills any current instance before starting (so `start` = restart). See §8.

The kill waits for the old shell (escalating to SIGKILL after 5s, §8), but its monitor exits a moment later, so the old instance may still be writing its last output and `process_exited` row. `start_one_service` does **not** wait for it: those rows carry the old run's `run_id`, so no reader shows them as part of the new run (see [logs.md](logs.md) §1 "Runs").

### 4.4 Record the launch

`run_id = start_run(conn, name, project_dir)` (`rust/src/logs/process_logs.rs`): inserts the `process_start_initiated` row — by the **CLI**, not the monitor — and returns its id, which is the new run id. The DB trigger assigns the row its own id as `run_id`.

### 4.5 Database path

The monitor is handed `candle_db_path()`, i.e. `<state_dir>/candle.db`.

### 4.6 Launch

`launch_monitor(&MonitorLaunchInfo { command_name, project_dir, shell, root, enable_stdin, database_path, run_id: Some(run_id) })`. A spawn/write error → `Generic("Failed to launch monitor process: <e>")`. See §5.

### 4.7 Success / failure detection

A single synchronous poll loop over **this run's rows only** (`get_process_logs` with `run_id: Some(run_id)`), so a previous instance's late rows can't be mistaken for this launch's result:
- Every `POLL_INTERVAL` (100ms), fetch this run's `process_started` / `process_start_failed` rows:
  - `process_started` → break (success).
  - `process_start_failed` → `recent_logs` = the non-empty `content` of all this run's rows, joined with `\n`; return `CandleError::ProcessStartFailed { command_name, recent_logs }`.
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
  #[serde(default)] run_id: Option<i64>,
}
```

This JSON shape (camelCase keys: `commandName`, `projectDir`, `shell`, `root`, `enableStdin`, `databasePath`, `runId`) is the wire contract over stdin. `run_id` is the id of the `process_start_initiated` row `start` wrote; the monitor stamps it on every row it saves. It defaults to `None` (launch info from an older candle), in which case the DB trigger assigns the monitor's rows to a run by position.

## 7. Monitor mode: `candle --monitor` (`rust/src/cli/monitor_mode.rs`, `rust/src/monitor/{launch_info,run}.rs`)

### 7.1 Reading launch info
- If no flags beyond `--monitor` are passed (the production path, since the launcher passes none) → read launch info as JSON from stdin.
- Else parse flags (`--flag value` or `--flag=value`): `--command-name` (required), `--project-dir` (required), `--shell` (required), `--root`, `--enable-stdin` (bool, default false), `--database-path`, `--run-id` (integer; a non-integer prints an error and exits 1). `project-dir` is resolved to an absolute path; `database-path` defaults to `<state_dir>/candle.db`. An unknown flag or missing required flag prints an error and exits 1.

Reading stdin as JSON: `read_launch_info_from_stdin` reads all of stdin to **EOF** and parses the trimmed text as one JSON object; a read or parse error prints `Error: failed to ... launch info from stdin` and exits 1. Because it waits for EOF, the parent must close stdin.

`main.rs` checks for `--monitor` anywhere in argv before any other dispatch and calls `run_monitor_mode`, which exits with the service's exit code (0 when there is none).

### 7.2 Monitor lifecycle (`monitor::run`, std threads, no async runtime)
1. `open_database_at(database_path)`; on failure print an error and exit 1.
2. Spawn `sh -c <shell>` in `launch_dir` (§7.3). Every row the monitor writes goes through `save_run_log(conn, run_id, ..)`, below written `save_run_log(type, ..)`. On spawn error → `save_run_log(process_start_failed, "Process failed to start: <e>")` and exit 1. No `processes` row exists yet in this branch.
3. `create_process_entry({ command_name, project_dir, pid: child_pid, log_collector_pid: Some(own_pid), shell, root, run_id })` — **pid = user shell pid; log_collector_pid = the monitor's own pid** (the DB column keeps its legacy name).
4. Reader threads (stdout, stderr), an optional stdin thread, and a wait thread forward events over one channel.
5. **Grace period**: collect events for `GRACE_PERIOD_MS` (500ms), writing output lines as they arrive, then drain anything already queued. If the child exited with a code other than `Some(0)` (a nonzero code or a signal) → `save_run_log(process_start_failed, "Process failed to start: exited with code <n>"` or, for a signal, `"Process failed to start: stopped by a signal"`), `delete_process_entry`, return. A signal counts as a deliberate stop when the `processes` row is already marked `killed_at` (kill marks it before signalling) or already gone; then the content is `STOPPED_WHILE_STARTING_MESSAGE` (`"Process was stopped while starting"`, `logs/log_type.rs`), which `list` does not report as `FAILED`.
6. Else `save_run_log(process_started)` (no content). If it already exited with 0 during the grace period, immediately log `process_exited` and delete the row.
7. Main loop: write output lines until the `Exit` event, calling `maybe_run_cleanup` about every 60s (`CLEANUP_INTERVAL_MS`). The reader threads and the wait thread share one channel, so `Exit` can arrive before the child's last lines. After `Exit` (here and in the grace period), `drain_after_exit` keeps writing lines until the channel disconnects (both pipes at EOF) or `POST_EXIT_DRAIN_MS` (500ms) passes. Normally the pipes close right after the exit and the drain ends at once; the limit only matters when a background grandchild holds the pipes open (possibly forever), and losing a few lines written after it in that case is acceptable. Then `save_run_log(process_exited, "Process exited with code <n>")` (or `"Process was stopped"` when killed by a signal), `delete_process_entry`, and return the exit code.

### 7.3 Supervising the service (`monitor::run`)
- `launch_dir = root ? Path::new(project_dir).join(root) : project_dir`. `Path::join` lets an **absolute** root replace the base, so the cwd matches `resolve_launch_dir` except for lexical normalization.
- Run `sh -c <shell>`, cwd = `launch_dir`, stdout/stderr piped, stdin piped only when `enable_stdin` (else null). Each stdout/stderr **line** → `save_run_log(conn, run_id, command_name, project_dir, Stdout|Stderr, Some(line))`.
- If `enable_stdin`: `clear_stdin_messages(command_name, project_dir)`, then a thread with its own connection, every `STDIN_POLL_INTERVAL_MS` (500ms) until the child exits:
  - `msg = pop_stdin_message(command_name, project_dir)` (oldest row, deletes it); if present, write `msg.data` bytes to the subprocess stdin (`encoding` is ignored). A write error stops the thread.

## 8. Kill path (used by start, and as `kill`/`stop`)

`handle_kill_command` (`rust/src/kill/mod.rs`): with names → dedupe set, `kill_by_command_name` each; without names → kill all `find_running_processes_by_project_dir`. `quiet_failure` suppresses the "No running processes" messages.

`kill_one_running_process` (`rust/src/kill/mod.rs`):
- `kill_process_tree_and_wait(pid, KILL_GRACE_PERIOD = 5s)` → `KillOutcome::{Terminated, Escalated, ProcessNotFound, Error}`.
- `Terminated` / `Escalated`: on `Escalated`, note `[Process '<name>' (PID <pid>) did not exit 5s after SIGTERM; sent SIGKILL]` on stderr; print `[Killed '<name>' process with PID: <pid>]` (unless quiet). If `killed_at` exists and is >5min old → `delete_process_entry` (+ warn `[Cleaning up stale process entry ...]`); else `update_process_killed_at` to now (unix seconds).
- `ProcessNotFound`: `delete_process_entry`; warn `[Cleaning up stale process entry ...]` only when the row had no `killed_at` (it claimed to be running). A row already marked killed — e.g. the second kill inside `restart` — is swept silently.
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
SQLite at `<state_dir>/candle.db`. `state_dir` (`rust/src/dirs.rs`): `$CANDLE_DATABASE_DIR`, else `$XDG_STATE_HOME/candle`, else `~/.local/state/candle`. Pragmas: `journal_mode=WAL`, `busy_timeout=30000` (multi-process access). Schema (`create ... if not exists`, run on every open; see [database.md](database.md)):

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
  root text,
  run_id integer);
create table process_output(
  id integer primary key autoincrement,
  command_name text not null,
  project_dir text not null,
  content text,
  log_type integer not null,
  timestamp integer not null default (strftime('%s','now')),
  run_id integer);
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
create index idx_process_output_run on process_output(project_dir, command_name, run_id);
create index idx_process_output_launches on process_output(project_dir, command_name, log_type, id);
-- plus trigger process_output_assign_run (see database.md)
```

Key SQL used by start-flow:
- Insert process: `create_process_entry` sets `start_time` = now in unix seconds; `root` / `run_id` → `NULL` when absent. (`created_at` via default.)
- `save_run_log`: `insert into process_output(command_name, project_dir, content, log_type, run_id) values(?,?,?,?,?)`. `start_run` is `save_process_log` (`run_id` NULL) of a `process_start_initiated` row, returning `last_insert_rowid()`.
- `find_processes_by_command_name_and_project_dir`: `select * from processes where command_name=? and project_dir=?`.
- `update_process_killed_at`: `update processes set killed_at=? where command_name=? and project_dir=? and pid=?`.
- `delete_process_entry`: `delete from processes where command_name=? and project_dir=? and pid=?`.
- stdin (`rust/src/db/stdin_messages.rs`): `pop_stdin_message` = `select * ... order by id asc limit 1` then `delete ... where id=?`; `clear_stdin_messages` = `delete ... where command_name=? and project_dir=?`.

### `ProcessLogType` (`rust/src/logs/log_type.rs`) — integer values are persisted, must match exactly:
`stdout=1, stderr=2, process_start_initiated=3, process_start_failed=4, process_started=5, process_exited=6`.

### Log polling (`get_process_logs`, `rust/src/logs/process_logs.rs`)
`get_process_logs` returns rows reversed into chronological order. `start_one_service`'s poll loop queries with `run_id: Some(run_id)` and `log_types: [process_started, process_start_failed]`, then sleeps 100ms. Filtering by run (not by "rows after a cursor") means only this launch's result counts.

## 12. Crates and system facilities
- Subprocesses: `std::process::Command`; reader threads split output into lines (a trailing partial line is still emitted at EOF); detach via `libc::setsid` in `pre_exec`. The monitor's stdin handshake reads to EOF instead of line-by-line.
- SQLite: `rusqlite`, with `PRAGMA journal_mode=WAL` and `PRAGMA busy_timeout=30000`.
- CLI parsing: the hand-rolled parser in `rust/src/cli/parser.rs`.
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
10. **Launch directory**: the banner uses `resolve_launch_dir` (normalized); the monitor's cwd uses a plain `Path::join`. Both let an absolute root win, so they differ only in normalization.
11. **Start lock**: concurrent starts of one service serialize on the `flock` from §4.0; different services (or projects) never contend.
12. **Exact output strings** (the two-line start banner `[Started process '<name>'] $ <shell>` / `[With root directory: <dir>]`, `[Service '<name>' is already running]`, `[Killed '<name>' process with PID: <pid>]`, cleanup/error variants) are asserted by tests — reproduced verbatim including backticks and brackets.
13. **Config resolution order**: `.candle.json` then deprecated `.candle-setup.json`; loose substring + directory-aware matching for service names; walk up parent dirs to find config.
