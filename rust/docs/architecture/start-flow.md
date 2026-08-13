# Start flow

Scope: the `start` / `check-start` command path, the monitor process, transient vs configured services, success/failure detection, and the `process_tree` / `process_alive` helpers. The Rust implementation lives under `rust/src/`; `src/...` line references point at the original Node/TypeScript source, kept as the historical source of truth.

## 1. High-level architecture

There are **two OS processes** per launched service:

1. **CLI process** (`candle start ...`) — resolves config, kills existing instances, spawns the monitor, then blocks watching the SQLite log table until it sees a success/failure marker, prints a result line, and exits. It does **not** stay attached to the service.
2. **Monitor process** (`candle --monitor`) — the same `candle` executable re-invoked in monitor mode: a detached, long-lived process that actually spawns the user's shell command, pipes its stdout/stderr into the SQLite `process_output` table, owns the DB `processes` row lifecycle, optionally feeds stdin, and exits when the service exits.

Communication between the two is **only** through the SQLite database (`candle.db`) plus a one-shot JSON handshake over the monitor's stdin.

Data flow:
```
candle start NAME
  → handle_start_command          (rust/src/start/start_command.rs)
    → start_one_service           (rust/src/start/start_one_service.rs)
        - check-start dedup
        - resolve ServiceConfig (transient or from config file)
        - handle_kill_command (kill existing)
        - save_process_log(process_start_initiated)
        - launch_monitor            (rust/src/start/launch.rs)
            → spawn `candle --monitor` detached, write LaunchInfo JSON to its stdin, end stdin
              ┌─────────────── monitor process ───────────────┐
              │ candle --monitor                              │
              │   read launch info (stdin JSON or flags)       │
              │   monitor::run spawns user shell               │
              │   create_process_entry()                       │
              │   wait_for_start / 500ms grace                 │
              │   save_process_log(process_started / _failed)  │
              │   wait_for_exit → save_process_log(process_exited)│
              │   delete_process_entry()                       │
              └────────────────────────────────────────────────┘
        - poll log table for process_started / process_start_failed (10s timeout)
        - print "[Started process ...]"
```

## 2. CLI surface

Two commands share the same option set (mirrors `src/main-cli.ts:102-135`):

- `start [name...]` (alias `run [name...]`) → `handle_start_command` with `check_start = None`.
- `check-start [name...]` → `handle_start_command` with `check_start = true`.

Options:
- `--shell <string>` — shell command for a **transient** process.
- `--root <string>` — root dir for a transient process.
- `--enable-stdin` (boolean) — enable DB-driven stdin feeding.

`console_output_format` is hard-coded to `'pretty'` for both.

Positional `name...` becomes `command_names`.

## 3. `handle_start_command` (`rust/src/start/start_command.rs`; mirrors `src/start-command.ts:25-63`)

1. Require `project_dir` (errors otherwise).
2. `command_names = req.command_names` or `[]`.
3. **If no `--shell`**: `command_names = resolve_command_names_or_all(project_dir, command_names)` — if names are empty, loads **all** configured service names from `.candle.json`; raises `UsageError('No services configured in .candle.json')` if config has zero services (mirrors `configFile.ts:259-269`).
4. **If `--shell` is set** (transient): require exactly one name, else `UsageError('Exactly one service name is required when using --shell')`. Call `start_one_service` once with `shell/root/enable_stdin/check_start`.
5. **Else**: loop over resolved names, calling `start_one_service` for each (sequentially, awaited). Transient flags are NOT passed in this branch.

## 4. `start_one_service` (`rust/src/start/start_one_service.rs`; mirrors `src/start/startOneService.ts:45-194`)

Run options: `{ command_name, project_dir, console_output_format, shell?, root?, enable_stdin?, check_start? }`. Returns a `StartResult { project_dir, service_name }`.

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
- Configured: `get_service_config_by_name(command_name)` (`rust/src/config/commands.rs`) — exact match by name, else **loose substring matching** that walks up directories matching `root` (mirrors `configFile.ts:276-349`); raises `MissingServiceWithNameError` (message `No service '<name>' configured for directory: <projectDir>`) if not found.

### 4.3 Kill existing

`handle_kill_command({ project_dir, command_names: [service_config.name], quiet_failure: true })`. Always kills any current instance before starting (so `start` = restart). See §8.

### 4.4 Set up log watch position

- Create `LogIterator({ command_names: [name], project_dir })` (`rust/src/logs/log_iterator.rs`), call `reset_to_latest_log_message()` (sets `current_log_id` to the id of the newest existing matching log, or null).
- `initial_log_position = log_iterator.copy()` — kept to fetch "recent logs" for an error message later.
- `save_process_log({ command_name, project_dir, log_type: process_start_initiated })` — inserted by the **CLI**, not the monitor.

### 4.5 Choose collector implementation

The database path is `<state_dir>/candle.db`.

### 4.6 Launch

`launch_monitor({ command_name, project_dir, shell, root, enable_stdin, database_path })`. See §5.

### 4.7 Success / failure detection

Two racing tasks:
- `wait_for_success`: iterate `log_iterator.it()` (polls DB every 100ms):
  - on `process_started` → break (success).
  - on `process_start_failed` → `recent_logs = initial_log_position.get_next_logs()`; raise `ProcessStartFailedError({ command_name, recent_logs })` (message includes joined `recent_logs[].content`).
- raced against a 10000ms timeout that rejects with `Error('Process failed to start (timed out while waiting)')`.

### 4.8 Success output

```
let mut launch_dir = project_dir;
if let Some(root) = service_config.root {
  launch_dir = if root.is_absolute() { root } else { project_dir.join(root) };
}
println!("[Started process '{}' (`{}`) in directory: '{}']", service_config.name, service_config.shell, launch_dir);
```

These exact strings are test-observable. Returns `{ project_dir, service_name }`.

## 5. `launch_monitor` (`rust/src/start/launch.rs`)

- **Path resolution** (`resolve_monitor_path`): `std::env::current_exe()` — the monitor is this very binary, so there is nothing to locate and nothing that can be missing. `CANDLE_MONITOR_PATH` overrides it for tests.
- **Launch**: command = that path, args = `["--monitor"]`, stdin piped, stdout/stderr null, `setsid` via `pre_exec`. Then write the launch-info JSON to stdin and drop the handle to close it.

`MonitorLaunchInfo` is passed **only via stdin JSON**, never via argv (the argv form exists for manual debugging). `serde_json::to_string` produces a single line with **no trailing newline**.

### Subtleties / platform notes
- `detached: true` on POSIX puts the child in a **new session/process group** (`setsid`). This is what lets the monitor outlive the CLI. The CLI does **not** call `unref()`; it survives only because the CLI process explicitly exits after the watch loop. The Rust implementation spawns with `setsid`/new process group and does not wait on or kill the child; the parent simply returns.
- stdio is fully piped (`pipe,pipe,pipe`). The CLI reads nothing back from the monitor (onStdout/onStderr are no-ops). It just writes the JSON and closes stdin.
- "spawn complete" resolves on successful process creation — in Rust, a successful `Command::spawn()`.

## 6. `LogCollectorLaunchInfo`

```
struct LogCollectorLaunchInfo {
  command_name: String,
  project_dir: String,
  shell: String,
  root: Option<String>,
  enable_stdin: Option<bool>,
  database_path: String,
}
```

This exact JSON shape is the wire contract over stdin.

## 7. Monitor mode: `candle --monitor` (`rust/src/cli/monitor_mode.rs`, `rust/src/monitor/{launch_info,run}.rs`)

### 7.1 Reading launch info
- If no flags beyond `--monitor` are passed (the production path, since the launcher passes none) → read launch info as JSON from stdin.
- Else parse flags: `--command-name` (required), `--project-dir` (required), `--shell` (required), `--root`, `--enable-stdin` (bool, default false), `--database-path`. `project-dir` is resolved to an absolute path; `database-path` defaults to `<state_dir>/candle.db`.

Reading stdin as JSON: read **the first complete line** as JSON. Because the handshake JSON has no trailing newline, the line is only delivered when stdin hits **EOF** (parent closes stdin). On EOF-before-message it rejects `'stdin closed before receiving any JSON message'`; non-JSON lines are logged and skipped (keeps waiting). The Rust implementation reads all of stdin to EOF and parses it as one JSON object.

### 7.2 Monitor lifecycle
1. Schedule `maybe_run_cleanup` every 60_000ms — periodic log/process cleanup.
2. `subprocess = monitor::start(launch_info)`; `pid = subprocess.proc.pid`.
3. `create_process_entry({ command_name, project_dir, pid, log_collector_pid: own_pid, shell, root })` — **pid = user shell pid; log_collector_pid = the monitor's own pid** (the DB column keeps its legacy name).
4. `wait_for_start()`; on reject → `save_process_log(process_start_failed, content: 'Process failed to start: ' + error.message)` and exit(1). (Note: does NOT delete the process row in this branch.)
5. **Grace period**: `sleep(500)` (`DEFAULT_GRACE_PERIOD_WAIT_MS`). If `exit_code != null && exit_code != 0` → `save_process_log(process_start_failed, content: 'Process failed to start: ' + exit_code)`, `delete_process_entry`, clear interval, exit(1).
6. Else `save_process_log(process_started)` (no content).
7. `wait_for_exit()`; then `save_process_log(process_exited, content: 'Process exited with code ' + exit_code)`, `delete_process_entry`, clear interval, exit(0).
8. Top-level error handler → log error; exit(1).

### 7.3 Supervising the service (`monitor::run`)
- `launch_dir = root ? project_dir.join(root) : project_dir`. (Note: unlike `start_one_service`'s success message, this joins unconditionally — for an **absolute** root, `project_dir.join(abs_root)` still concatenates; the user-visible message in §4.8 special-cases absolute, but the actual cwd here does not. This divergence is preserved deliberately.)
- Run `shell` via the OS shell (`/bin/sh -c` on POSIX), cwd = `launch_dir`. Each stdout/stderr **line** → `save_process_log({ command_name, project_dir, content: line, log_type: stdout|stderr })`.
- If `enable_stdin`: `clear_stdin_messages(command_name, project_dir)`, then every 500ms:
  - if the subprocess has exited → stop.
  - `msg = pop_stdin_message(command_name, project_dir)` (oldest row, deletes it); if present, write `msg.data` (with `msg.encoding`) to the subprocess stdin.
  - On exit → stop.

## 8. Kill path (used by start, and as `kill`/`stop`)

`handle_kill_command` (`rust/src/kill/mod.rs`): with names → dedupe set, `kill_by_command_name` each; without names → kill all `find_running_processes_by_project_dir`. `quiet_failure` suppresses the "No running processes" messages.

`kill_one_running_process` (`rust/src/kill/mod.rs`):
- `kill_process_tree(pid)` → result `'success' | 'process_not_found' | 'error'`.
- `'success'`: print `[Killed '<name>' process with PID: <pid>]` (unless quiet). If `killed_at` exists and is >5min old → `delete_process_entry` (+ warn `[Cleaning up stale process entry ...]`); else `update_process_killed_at` to now (unix seconds).
- `'process_not_found'`: warn `[Cleaning up stale process entry ...]`, `delete_process_entry`.
- `'error'`: print `Error killing process '<name>' with PID: <pid>`.

`kill_process_tree` (`rust/src/kill/mod.rs`): errors on pid null/0; `get_process_tree(pid)`; if empty → `'process_not_found'`; iterate **reversed** (children first, root last) sending `SIGTERM`; ESRCH ignored; other errors → warn + `has_error`. Returns `'process_not_found'` if every kill found nothing, `'error'` if any non-ESRCH error, else `'success'`.

## 9. `process_tree` (`rust/src/process_tree.rs`)
- `get_process_tree(root_pid) -> Vec<i32>` — BFS/DFS collecting root + all descendants.
- `get_child_pids(parent_pid)`: macOS → `pgrep -P <pid>`; Linux → `ps -o pid --no-headers --ppid <pid>`; other platforms → `[]`.
- Run command for pids: spawn with `stdio = ['ignore','pipe','ignore']`, collect stdout, on close split lines → parse ints, drop non-numeric; on spawn error → `[]`. Windows is unsupported here (returns no children).

## 10. `process_alive` (`rust/src/process_alive.rs`)
- `is_process_alive(pid)`: signal-0 probe `kill(pid, None)` via `nix::sys::signal::kill(Pid, None)` → `Ok` = alive, `Err(EPERM)` = alive (other user), `Err(ESRCH)`/other = dead.
- `filter_alive_processes(entries)`: keep entry if `log_collector_pid` alive **OR** `pid` alive; otherwise `delete_process_entry({command_name, project_dir, pid})` and drop it.

## 11. Database (`rust/src/db/mod.rs`, `rust/src/db/process_table.rs`)
SQLite at `<state_dir>/candle.db`. `state_dir` (`rust/src/dirs.rs`; mirrors `src/dirs.ts:9-22`): `$CANDLE_DATABASE_DIR`, else `$XDG_STATE_HOME/candle`, else `~/.local/state/candle`. Pragmas: `journal_mode=WAL`, `busy_timeout=30000` (multi-process access). Schema (safe-upgrade migrations):

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
- Insert process: `create_process_entry` sets `start_time = floor(now/1000)` seconds, `root` → `null` when absent. (`created_at` via default.)
- `save_process_log`: `insert into process_output(command_name, project_dir, content, log_type) values(?,?,?,?)`.
- `find_processes_by_command_name_and_project_dir`: `select * from processes where command_name=? and project_dir=?`.
- `update_process_killed_at`: `update processes set killed_at=? where command_name=? and project_dir=? and pid=?`.
- `delete_process_entry`: `delete from processes where command_name=? and project_dir=? and pid=?`.
- stdin (`rust/src/db/stdin_messages.rs`): `pop_stdin_message` = `select * ... order by id asc limit 1` then `delete ... where id=?`; `clear_stdin_messages` = `delete ... where command_name=? and project_dir=?`.

### `ProcessLogType` (`rust/src/logs/log_type.rs`) — integer values are persisted, must match exactly:
`stdout=1, stderr=2, process_start_initiated=3, process_start_failed=4, process_started=5, process_exited=6`.

### Log polling (`LogIterator` + `get_process_logs`, `rust/src/logs/{log_iterator,process_logs}.rs`)
`get_process_logs` builds a query with `after_log_id` / `limit`, returning rows reversed into chronological order. `LogIterator.it()` loops: fetch `peek_next_logs({ after_log_id: current_log_id })`, yield each (advancing `current_log_id`), sleep 100ms. `reset_to_latest_log_message` seeds `current_log_id` to the newest existing id so the CLI only reacts to logs produced after launch begins.

## 12. Crates used (replacing the original npm deps)
- `@facetlayer/subprocess` (`startShellCommand`, `Subprocess`, line-buffered stdout/stderr, `waitForStart`/`waitForExit`, detached spawn) → `std::process::Command` / `tokio::process::Command`; `tokio::io::{BufReader, AsyncBufReadExt}.lines()` for line splitting; detach via `command_group` / `nix::unistd::setsid` (`pre_exec`).
- `@facetlayer/parse-stdout-lines` (`unixPipeToLines`, splits on `\n`, emits trailing partial line on EOF as a final line, then `null` on close) → replicated; the EOF-flush behavior is what makes the stdin JSON handshake (no trailing newline) work.
- `@facetlayer/sqlite-wrapper` (schema migrations, WAL) → `rusqlite`. Sets `PRAGMA journal_mode=WAL` and `PRAGMA busy_timeout=30000`.
- `yargs` → `clap`.
- `@modelcontextprotocol/sdk`: not in start-flow (mcp command).
- Liveness/process tree: `nix` (kill, setsid) and/or `sysinfo`.

## 13. Subtle behaviors
1. **stdin handshake needs EOF**: the collector parses the first complete *line*; the JSON has no newline, so it only completes at stdin close. The parent must close stdin after writing.
2. **Detached but not unref'd**: monitor survival relies on the CLI explicitly exiting after the watch loop; the monitor runs in its own session. The Rust implementation spawns with a new session and does not join.
3. **check-start dual liveness check**: must filter `killed_at IS NULL` *and* probe PIDs, *and* delete dead rows. Missing the probe causes false "already running".
4. **`filter_alive_processes` checks `log_collector_pid` first, then `pid`** — either alive keeps the row.
5. **Two PIDs per row**: `pid` = user shell; `log_collector_pid` = the monitor process. Kill targets `pid` (the shell tree) via SIGTERM, children-first.
6. **Timestamps are unix seconds** (`strftime('%s','now')` and `floor(now/1000)`), not ms. The 5-minute stale check compares seconds.
7. **Grace period 500ms + wait_for_start**: success = `process_started` only after surviving 500ms with a zero/None exit code. A command that exits 0 within 500ms is still treated as started (exitCode 0 passes the `!= 0` guard). A nonzero exit within 500ms → `process_start_failed` + row deleted.
8. **Start-failed branch (wait_for_start reject) does NOT delete the process row**, but the grace-period-failure branch does. This asymmetry is preserved.
9. **10s CLI timeout** rejects with `'Process failed to start (timed out while waiting)'` independent of the monitor — the monitor keeps running even if the CLI times out.
10. **launchDir divergence**: the success message special-cases absolute root; `monitor::start`'s cwd does not (joins always). Behavior preserved for absolute roots.
11. **Exact output strings** (`[Started process '<name>' (`<shell>`) in directory: '<dir>']`, `[Service '<name>' is already running]`, `[Killed '<name>' process with PID: <pid>]`, cleanup/error variants) are asserted by tests — reproduced verbatim including backticks and brackets.
12. **Config resolution order**: `.candle.json` then deprecated `.candle-setup.json`; loose substring + directory-aware matching for service names; walk up parent dirs to find config.
