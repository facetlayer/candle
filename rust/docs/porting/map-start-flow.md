I have everything needed. Here is the spec.

---

# Candle "start-flow" Subsystem — Reimplementation Spec

Scope: the `start` / `check-start` command path, the log-collector sidecar, transient vs configured services, success/failure detection, and the `process-tree` / `process-alive` helpers. File:line references are to the TypeScript source as of this reading.

## 1. High-level architecture

There are **two OS processes** per launched service:

1. **CLI process** (`candle start ...`) — resolves config, kills existing instances, spawns the sidecar, then blocks watching the SQLite log table until it sees a success/failure marker, prints a result line, and exits. It does **not** stay attached to the service.
2. **Log-collector sidecar** (`main-log-collector`) — a detached, long-lived process that actually spawns the user's shell command, pipes its stdout/stderr into the SQLite `process_output` table, owns the DB `processes` row lifecycle, optionally feeds stdin, and exits when the service exits.

Communication between the two is **only** through the SQLite database (`candle.db`) plus a one-shot JSON handshake over the sidecar's stdin.

Data flow:
```
candle start NAME
  → handleStartCommand            (src/start-command.ts)
    → startOneService             (src/start/startOneService.ts)
        - checkStart dedup
        - resolve ServiceConfig (transient or from config file)
        - handleKillCommand (kill existing)
        - saveProcessLog(process_start_initiated)
        - launchWithLogCollector  (src/log-collector/launchWithLogCollector.ts)
            → spawn sidecar detached, write LaunchInfo JSON to its stdin, end stdin
              ┌─────────────── sidecar process ───────────────┐
              │ main-log-collector.ts                         │
              │   getLaunchInfo() (stdin JSON or argv)         │
              │   startMonitoredService() spawns user shell    │
              │   createProcessEntry()                         │
              │   waitForStart / 500ms grace                   │
              │   saveProcessLog(process_started / _failed)    │
              │   waitForExit → saveProcessLog(process_exited) │
              │   deleteProcessEntry()                         │
              └────────────────────────────────────────────────┘
        - poll log table for process_started / process_start_failed (10s timeout)
        - print "[Started process ...]"
```

## 2. CLI surface

`src/main-cli.ts:102-135`. Two commands share the same option set:

- `start [name...]` (alias `run [name...]`) → `handleStartCommand({ ..., checkStart: undefined })`
- `check-start [name...]` → `handleStartCommand({ ..., checkStart: true })`

Options (all `strictOptions()`):
- `--shell <string>` — shell command for a **transient** process.
- `--root <string>` — root dir for a transient process.
- `--enable-stdin` (boolean) — enable DB-driven stdin feeding.

`consoleOutputFormat` is hard-coded to `'pretty'` for both (`main-cli.ts:344,351`).

Positional `name...` becomes `commandNames`.

## 3. `handleStartCommand` (src/start-command.ts:25-63)

1. Require `projectDir` (throws plain `Error` otherwise).
2. `let commandNames = req.commandNames || []`.
3. **If no `--shell`**: `commandNames = resolveCommandNamesOrAll(projectDir, commandNames)` — if names are empty, loads **all** configured service names from `.candle.json`; throws `UsageError('No services configured in .candle.json')` if config has zero services (`configFile.ts:259-269`).
4. **If `--shell` is set** (transient): require exactly one name, else `UsageError('Exactly one service name is required when using --shell')`. Call `startOneService` once with `shell/root/enableStdin/checkStart`.
5. **Else**: loop over resolved names, calling `startOneService` for each (sequentially, awaited). Transient flags are NOT passed in this branch.

## 4. `startOneService` (src/start/startOneService.ts:45-194)

`RunOptions`: `{ commandName, projectDir, consoleOutputFormat, shell?, root?, enableStdin?, checkStart? }`. Returns `StartResult { projectDir, serviceName }`.

### 4.1 check-start dedup (lines 64-79) — runs BEFORE config resolution
```ts
if (req.checkStart) {
  if (!req.commandName) throw new UsageError('Command name is required');
  const existingProcesses = findProcessesByCommandNameAndProjectDir(req.commandName, projectDir);
  const notKilled = existingProcesses.filter(p => p.killed_at === null);
  const runningProcesses = filterAliveProcesses(notKilled);
  if (runningProcesses.length > 0) {
    console.log(`[Service '${req.commandName}' is already running]`);
    return { projectDir, serviceName: req.commandName };
  }
}
```
Subtlety to preserve: dedup uses **both** `killed_at IS NULL` filtering **and** a liveness probe (`filterAliveProcesses`). Reboots/external kills leave `killed_at=NULL` rows whose PIDs are dead; without the liveness check, `check-start` would wrongly skip. `filterAliveProcesses` also **deletes** the dead rows as a side effect. Done before config resolution so dedup works for transient names not in config.

### 4.2 Resolve `ServiceConfig`
- Transient (`req.shell` set, lines 81-99): require `commandName`; validate `root` with `isValidRootPath` (absolute OK; relative must not start with `..` after normalize) else `UsageError('Invalid root path: "<root>". Root must be an absolute path or a relative path within the project.')`. Build `serviceConfig = { name, shell, root, enableStdin }`.
- Configured (lines 100-104): `getServiceConfigByName(commandName)` — exact match by name, else **loose substring matching** that walks up directories matching `root` (`configFile.ts:276-349`); throws `MissingServiceWithNameError` (message `No service '<name>' configured for directory: <projectDir>`) if not found.

### 4.3 Kill existing (line 107)
`await handleKillCommand({ projectDir, commandNames: [serviceConfig.name], quietFailure: true })`. Always kills any current instance before starting (so `start` = restart). See §8.

### 4.4 Set up log watch position (lines 111-124)
- Create `LogIterator({ commandNames: [name], projectDir })`, call `resetToLatestLogMessage()` (sets `currentLogId` to the id of the newest existing matching log, or null).
- `initialLogPosition = logIterator.copy()` — kept to fetch "recent logs" for an error message later.
- `saveProcessLog({ command_name, project_dir, log_type: process_start_initiated })` — inserted by the **CLI**, not the sidecar.

### 4.5 Choose collector implementation (lines 129-137)
```ts
let logCollector: 'node' | 'rust' | undefined;
try { logCollector = findConfigFile(projectDir)?.config?.logCollector; } catch {}
const databasePath = Path.join(getStateDirectory(), 'candle.db');
```

### 4.6 Launch (lines 140-147)
`await launchWithLogCollector({ commandName, projectDir, shell, root, enableStdin, databasePath }, { logCollector })`. See §5.

### 4.7 Success / failure detection (lines 152-176)
Two racing promises:
- `waitForSuccess`: iterate `logIterator.it()` (polls DB every 100ms, `LogIterator.ts:43-51`):
  - on `process_started` → `break` (success).
  - on `process_start_failed` → `recentLogs = initialLogPosition.getNextLogs()`; throw `ProcessStartFailedError({ commandName, recentLogs })` (message includes joined `recentLogs[].content`).
- `Promise.race` against a `setTimeout(10000)` that rejects with `new Error('Process failed to start (timed out while waiting)')`.

### 4.8 Success output (lines 180-188)
```ts
let launchDir = projectDir;
if (serviceConfig.root)
  launchDir = Path.isAbsolute(serviceConfig.root) ? serviceConfig.root : Path.join(projectDir, serviceConfig.root);
console.log(`[Started process '${serviceConfig.name}' (\`${serviceConfig.shell}\`) in directory: '${launchDir}']`);
```
These exact strings are test-observable. Returns `{ projectDir, serviceName }`.

## 5. `launchWithLogCollector` (src/log-collector/launchWithLogCollector.ts)

Selects implementation by `options.logCollector === 'rust'`; default is **node**.

- **Rust path resolution** (`getRustCollectorPath`, lines 7-13): `<ProjectRootDir>/rust/target/release/candle-log-collector`; if missing, throws `'Rust log collector not found. Build it first: cd rust && cargo build --release'`. (`ProjectRootDir` = one level above `dist/`, `dirs.ts:5-6`.)
- **Node launch** (lines 34-55): command = `process.argv[0]` (the node binary); args = `[<ProjectRootDir>/dist/main-log-collector.js]`. Spawn options:
  ```ts
  { stdio: ['pipe','pipe','pipe'], detached: true }
  ```
  Then `await subprocess.waitForStart()`, `proc.stdin.write(JSON.stringify(launchInfo))`, `proc.stdin.end()`.
- **Rust launch** (lines 57-78): identical except command = rust binary, args = `[]`.

Both pass `LaunchInfo` **only via stdin JSON**, never via argv. `JSON.stringify` produces a single line with **no trailing newline**.

### Subtleties / platform notes for Rust
- `detached: true` on POSIX puts the child in a **new session/process group** (`setsid`). This is what lets the sidecar outlive the CLI. The CLI does **not** call `unref()`; it survives only because the CLI process explicitly `process.exit`s after the watch loop. In Rust: spawn with `setsid`/new process group and do not wait on/kill the child; let the parent return.
- stdio is fully piped (`pipe,pipe,pipe`). The CLI reads nothing back from the sidecar (onStdout/onStderr are no-ops). It just writes the JSON and closes stdin.
- `waitForStart()` resolves on the libuv `spawn` event (`Subprocess.js:66-71`) — i.e., the OS process was created. Rust: equivalent is a successful `Command::spawn()`.

## 6. `LogCollectorLaunchInfo` (src/log-collector/LogCollectorLaunchInfo.ts)
```ts
interface LogCollectorLaunchInfo {
  commandName: string;
  projectDir: string;
  shell: string;
  root?: string;
  enableStdin?: boolean;
  databasePath: string;
}
```
This exact JSON shape is the wire contract over stdin.

## 7. Sidecar: `main-log-collector.ts`

### 7.1 `getLaunchInfo()` (lines 18-64)
- If `hideBin(process.argv).length === 0` → `readStdinAsJson()` (the production path, since launchers pass no argv).
- Else parse argv via yargs: `--commandName` (required), `--projectDir` (required), `--shell` (required), `--root`, `--enableStdin` (bool, default false), `--databasePath`. `projectDir` is `Path.resolve`d; `databasePath` defaults to `<stateDir>/candle.db`.

`readStdinAsJson` (src/log-collector/readStdinJson.ts): uses `unixPipeToLines` to read **the first complete line** as JSON. Because the handshake JSON has no trailing newline, the line is only delivered when stdin hits **EOF** (parent calls `stdin.end()`). On EOF-before-message it rejects `'stdin closed before receiving any JSON message'`; non-JSON lines are logged and skipped (keeps waiting). Rust: read all of stdin to EOF, parse as one JSON object.

### 7.2 `main()` (lines 66-153)
1. `setInterval(maybeRunCleanup, 60_000)` — periodic log/process cleanup.
2. `subprocess = startMonitoredService(launchInfo)`; `pid = subprocess.proc.pid`.
3. `createProcessEntry({ commandName, projectDir, pid, logCollectorPid: process.pid, shell, root })` — **pid = user shell pid; log_collector_pid = sidecar's own pid.**
4. `await subprocess.waitForStart()`; on reject → `saveProcessLog(process_start_failed, content: 'Process failed to start: ' + error.message)` and `process.exit(1)`. (Note: does NOT delete the process row in this branch.)
5. **Grace period**: `await sleep(500)` (`DEFAULT_GRACE_PERIOD_WAIT_MS`). If `exitCode != null && exitCode !== 0` → `saveProcessLog(process_start_failed, content: 'Process failed to start: ' + exitCode)`, `deleteProcessEntry`, clear interval, `process.exit(1)`.
6. Else `saveProcessLog(process_started)` (no content).
7. `await subprocess.waitForExit()`; then `saveProcessLog(process_exited, content: 'Process exited with code ' + exitCode)`, `deleteProcessEntry`, clear interval, `process.exit(0)`.
8. Top-level `.catch` → `console.error(error); process.exit(1)`.

### 7.3 `startMonitoredService` (src/log-collector/startMonitoredService.ts)
- `launchDir = root ? Path.join(projectDir, root) : projectDir`. (Note: unlike startOneService's success message, this uses `Path.join` unconditionally — for an **absolute** root, `Path.join(projectDir, absRoot)` still concatenates; the user-visible message in §4.8 special-cases absolute, but the actual cwd here does not. Easy-to-get-wrong divergence; preserve current behavior or unify deliberately.)
- `startShellCommand(shell, [], { shell: true, cwd: launchDir, onStdout, onStderr })` — `shell:true` means the command string is run via the OS shell (`/bin/sh -c` on POSIX). Each stdout/stderr **line** → `saveProcessLog({ command_name, project_dir, content: line, log_type: stdout|stderr })`.
- If `enableStdin`: `clearStdinMessages(commandName, projectDir)`, then `setInterval(500ms)`:
  - if `subprocess.proc.exitCode !== null` → clearInterval, return.
  - `msg = popStdinMessage(commandName, projectDir)` (oldest row, deletes it); if present, `proc.stdin.write(msg.data, msg.encoding)`.
  - On `waitForExit` → clearInterval.

## 8. Kill path (used by start, and as `kill`/`stop`)

`handleKillCommand` (src/kill-command.ts): with names → dedupe set, `killByCommandName` each; without names → kill all `findRunningProcessesByProjectDir`. `quietFailure` suppresses the "No running processes" messages.

`killOneRunningProcess` (src/kill/killOneRunningProcess.ts):
- `killProcessTree(pid)` → result `'success' | 'process_not_found' | 'error'`.
- `'success'`: print `[Killed '<name>' process with PID: <pid>]` (unless quiet). If `killed_at` exists and is >5min old → `deleteProcessEntry` (+ warn `[Cleaning up stale process entry ...]`); else `updateProcessKilledAt` to now (unix seconds).
- `'process_not_found'`: warn `[Cleaning up stale process entry ...]`, `deleteProcessEntry`.
- `'error'`: print `Error killing process '<name>' with PID: <pid>`.

`killProcessTree` (src/kill/killProcessTree.ts): throws on pid null/0; `getProcessTree(pid)`; if empty → `'process_not_found'`; iterate **reversed** (children first, root last) sending `SIGTERM`; ESRCH ignored; other errors → warn + `hasError`. Returns `'process_not_found'` if every kill found nothing, `'error'` if any non-ESRCH error, else `'success'`.

## 9. `process-tree.ts`
- `getProcessTree(rootPid): number[]` — BFS/DFS collecting root + all descendants.
- `getChildPids(parentPid)`: macOS → `pgrep -P <pid>`; Linux → `ps -o pid --no-headers --ppid <pid>`; other platforms → `[]`.
- `runCommandForPids`: spawn with `stdio: ['ignore','pipe','ignore']`, collect stdout, on close split lines → `parseInt`, drop NaN; on spawn error → `[]`.

Rust: replace with `sysinfo` crate (cross-platform process tree) or shell out to the same commands. Note Windows is unsupported here (returns no children) — decide whether to implement.

## 10. `process-alive.ts`
- `isProcessAlive(pid)`: `process.kill(pid, 0)`; `EPERM` → alive (other user); `ESRCH`/other → dead.
- `filterAliveProcesses(entries)`: keep entry if `log_collector_pid` alive **OR** `pid` alive; otherwise `deleteProcessEntry({commandName, projectDir, pid})` and drop it.

Rust: signal-0 probe is `kill(pid, 0)` via `nix::sys::signal::kill(Pid, None)` → `Ok`=alive, `Err(EPERM)`=alive, `Err(ESRCH)`=dead.

## 11. Database (src/database/database.ts)
SQLite at `<stateDir>/candle.db`. `stateDir` (src/dirs.ts:9-22): `$CANDLE_DATABASE_DIR`, else `$XDG_STATE_HOME/candle`, else `~/.local/state/candle`. Pragmas: `journal_mode=WAL`, `busy_timeout=30000` (multi-process access). Schema (migration mode `safe-upgrades`):

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
- Insert process: `createProcessEntry` (`processTable.ts:38-52`) sets `start_time = floor(Date.now()/1000)`, `root` → `null` when absent. (`created_at` via default.)
- `saveProcessLog`: `insert into process_output(command_name, project_dir, content, log_type) values(?,?,?,?)`.
- `findProcessesByCommandNameAndProjectDir`: `select * from processes where command_name=? and project_dir=?`.
- `updateProcessKilledAt`: `update processes set killed_at=? where command_name=? and project_dir=? and pid=?`.
- `deleteProcessEntry`: `delete from processes where command_name=? and project_dir=? and pid=?`.
- stdin: `popStdinMessage` = `select * ... order by id asc limit 1` then `delete ... where id=?`; `clearStdinMessages` = `delete ... where command_name=? and project_dir=?`.

### `ProcessLogType` enum (src/logs/ProcessLogType.ts) — integer values are persisted, must match exactly:
`stdout=1, stderr=2, process_start_initiated=3, process_start_failed=4, process_started=5, process_exited=6`.

### Log polling (`LogIterator` + `getProcessLogs`)
`getProcessLogs` builds a query via `buildLogSearchQuery` with `afterLogId` / `limit`, returns rows reversed into chronological order (`processLogs.ts:74`). `LogIterator.it()` loops: fetch `peekNextLogs({ afterLogId: currentLogId })`, yield each (advancing `currentLogId`), sleep 100ms. `resetToLatestLogMessage` seeds `currentLogId` to newest existing id so the CLI only reacts to logs produced after launch begins.

## 12. External npm deps → Rust crate suggestions
- `@facetlayer/subprocess` (`startShellCommand`, `Subprocess`, line-buffered stdout/stderr, `waitForStart`/`waitForExit`, detached spawn): reimplement with `std::process::Command` / `tokio::process::Command`. Use `tokio::io::{BufReader, AsyncBufReadExt}.lines()` for line splitting. For detach use `command_group` crate or `nix::unistd::setsid` via `pre_exec`.
- `@facetlayer/parse-stdout-lines` (`unixPipeToLines`, splits on `\n`, emits trailing partial line on EOF as a final line, then `null` on close): replicate; the EOF-flush behavior is what makes the stdin JSON handshake (no trailing newline) work.
- `@facetlayer/sqlite-wrapper` (`DatabaseLoader`, schema migrations, WAL): use `rusqlite` (+ a migration runner) or `sqlx` (sqlite). Set `PRAGMA journal_mode=WAL` and `PRAGMA busy_timeout=30000`.
- `yargs`: use `clap`.
- `@modelcontextprotocol/sdk`: not in start-flow (mcp command), ignore here.
- Liveness/process tree: `nix` (kill, setsid) and/or `sysinfo`.

## 13. Subtle behaviors easy to get wrong in Rust
1. **stdin handshake needs EOF**: the collector parses the first complete *line*; the JSON has no newline, so it only completes at stdin close. Parent must close stdin after writing.
2. **Detached but not unref'd**: sidecar survival relies on the CLI explicitly exiting after the watch loop; sidecar runs in its own session. Rust: spawn with new session, do not join.
3. **check-start dual liveness check**: must filter `killed_at IS NULL` *and* probe PIDs, *and* delete dead rows. Missing the probe causes false "already running".
4. **`filterAliveProcesses` checks `log_collector_pid` first, then `pid`** — either alive keeps the row.
5. **Two PIDs per row**: `pid` = user shell; `log_collector_pid` = sidecar. Kill targets `pid` (the shell tree) via SIGTERM, children-first.
6. **Timestamps are unix seconds** (`strftime('%s','now')` and `floor(Date.now()/1000)`), not ms. The 5-minute stale check compares seconds.
7. **Grace period 500ms + waitForStart**: success = `process_started` only after surviving 500ms with a zero/None exit code. A command that exits 0 within 500ms is still treated as started (exitCode 0 passes the `!== 0` guard). A nonzero exit within 500ms → `process_start_failed` + row deleted.
8. **Start-failed branch (waitForStart reject) does NOT delete the process row**, but the grace-period-failure branch does. Preserve this asymmetry unless intentionally fixing.
9. **10s CLI timeout** rejects with `'Process failed to start (timed out while waiting)'` independent of the sidecar — the sidecar keeps running even if the CLI times out.
10. **launchDir divergence**: success message special-cases absolute root; `startMonitoredService` cwd does not (uses `Path.join` always). Verify intended behavior for absolute roots.
11. **Exact output strings** (`[Started process '<name>' (`<shell>`) in directory: '<dir>']`, `[Service '<name>' is already running]`, `[Killed '<name>' process with PID: <pid>]`, cleanup/error variants) are asserted by tests — reproduce verbatim including backticks and brackets.
12. **Config resolution order**: `.candle.json` then deprecated `.candle-setup.json`; loose substring + directory-aware matching for service names; walk up parent dirs to find config.

## 14. Rust reimplementation notes — module/function plan and ordering

Build bottom-up:

1. **`state_dir` / paths** — `get_state_directory()` (env `CANDLE_DATABASE_DIR` → `XDG_STATE_HOME/candle` → `~/.local/state/candle`); `candle.db` path; project-root resolution for the collector binary.
2. **`db`** — open SQLite, apply schema + WAL + busy_timeout. Then `process_table` (`create/update_killed_at/delete/find_by_name_and_dir/find_running_by_dir`), `process_logs` (`save_process_log`, `get_process_logs` with afterLogId/limit + reverse), `stdin_messages` (`create/pop/clear`), `ProcessLogType` constants 1–6.
3. **`process_alive`** — `is_process_alive(pid)` (kill 0; EPERM=alive), `filter_alive_processes(entries)` (log_collector_pid OR pid; delete dead).
4. **`process_tree`** — `get_process_tree(root)`, platform `get_child_pids`.
5. **`kill`** — `kill_process_tree(pid)` (SIGTERM children-first), `kill_one_running_process`, `handle_kill_command`.
6. **`config`** — `ServiceConfig`, `CandleSetupConfig` (incl. `logCollector`), `find_config_file`, `resolve_command_names_or_all`, `get_service_config_by_name` (exact + loose), validation + `is_valid_root_path`.
7. **`log_iterator`** — wraps `get_process_logs`, `reset_to_latest`, `copy`, polling `it()` (100ms), `get_next_logs`.
8. **`subprocess`** — line-buffered spawn abstraction with `wait_for_start`/`wait_for_exit`, detached/new-session spawn, stdin write.
9. **`log_collector` (sidecar binary `candle-log-collector`)** — `read_stdin_as_json` (read to EOF), `LogCollectorLaunchInfo`, `start_monitored_service` (spawn shell, pipe lines to DB, stdin polling 500ms), `main` lifecycle (create row → waitForStart → 500ms grace → process_started → waitForExit → process_exited → delete row; failure branches).
10. **`launch`** — `launch_with_log_collector` (spawn sidecar detached, write JSON, close stdin; node-vs-rust selection — for a pure-Rust impl this becomes "spawn the rust collector").
11. **`start`** — `start_one_service` (checkStart dedup → resolve config → kill existing → seed log iterator → save process_start_initiated → launch → race(watch, 10s timeout) → print result). Depends on everything above.
12. **`start_command` / CLI** — `handle_start_command`, clap wiring for `start`/`run`/`check-start` with `--shell/--root/--enable-stdin`.

Dependency order: 1→2→{3,4,6,7,8}→5→9→10→11→12.