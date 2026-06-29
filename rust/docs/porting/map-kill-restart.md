I have enough to produce the spec.

# Candle "kill-restart" Subsystem — Technical Spec for Rust Reimplementation

## 0. Scope & files
This subsystem covers three CLI commands and their shared helpers:
- `candle kill [name...]` (alias `stop`) → `src/kill-command.ts:handleKillCommand`
- `candle kill-all` → `src/kill-all-command.ts:handleKillAll`
- `candle restart [name...]` → `src/restart-command.ts:handleRestart`
- Shared kill helpers: `src/kill/killOneRunningProcess.ts`, `src/kill/killProcessTree.ts`, `src/process-tree.ts`
- DB layer: `src/database/processTable.ts`, schema in `src/database/database.ts`
- Liveness check: `src/process-alive.ts`

## 1. Data model

### 1.1 `processes` table schema (`src/database/database.ts:16-27`)
```sql
create table processes(
    id integer primary key autoincrement,
    command_name text not null,
    project_dir text not null,
    pid integer not null,
    log_collector_pid integer,
    start_time integer not null,
    created_at integer not null default (strftime('%s', 'now')),
    killed_at integer,
    shell text,
    root text
)
```
Key semantics:
- A process is considered **running** iff `killed_at IS NULL`.
- All timestamps are **Unix seconds** (`Math.floor(Date.now() / 1000)`), not millis. Rust: `SystemTime::now().duration_since(UNIX_EPOCH).as_secs()`.
- `pid` is the PID of the service's shell process (root of the tree). `log_collector_pid` is a separate supervising process (not killed by this subsystem — see §6).
- `ProcessEntry` TS interface (`processTable.ts:3-14`) maps 1:1 to columns. `killed_at` and `root` are nullable/optional.

### 1.2 Queries used (exact SQL)
- `findProcessesByCommandNameAndProjectDir(name, dir)` → `select * from processes where command_name = ? and project_dir = ?` (note: **does not filter on `killed_at`** — returns killed entries too).
- `findRunningProcessesByProjectDir(dir)` → `select * from processes where project_dir = ? and killed_at is null`
- `findAllProcesses()` → `select * from processes` (no filter at all — see §4 note).
- `updateProcessKilledAt`: `update processes set killed_at = ? where command_name = ? and project_dir = ? and pid = ?`
- `deleteProcessEntry`: `delete from processes where command_name = ? and project_dir = ? and pid = ?`

The natural key used for update/delete is the triple **(command_name, project_dir, pid)**, not `id`.

## 2. `handleKillCommand` (`src/kill-command.ts`)

Options:
```ts
{ projectDir: string; commandNames?: string[]; quietFailure?: boolean; quiet?: boolean }
```
`projectDir` comes from `findProjectDir()` (walks up from cwd to find `.candle.json`; throws `MissingSetupFileError` if none). CLI calls `assertValidCommandNames(commandNames)` **before** kill — this validates each name exists as a service in config and exits with stderr `No service ...<name>...` if not (this is why `kill nonexistent-service` fails with exit code != 0 and stderr mentioning the name; the kill body is never reached).

Control flow:
1. `commandNames = req.commandNames ?? []`.
2. **If names given**: dedupe via `new Set`, then for each unique name call `killByCommandName`:
   - `findProcessesByCommandNameAndProjectDir(name, projectDir)` (all matching entries, including already-killed).
   - For each, `await killOneRunningProcess(process, options)`; increment counter.
   - If counter == 0 and `!quietFailure`: print `No running processes found for service '<name>' in project '<projectDir>'`.
3. **If no names given**: `findRunningProcessesByProjectDir(projectDir)` (only `killed_at is null`). Kill each.
   - If counter == 0 and `!quietFailure`: print `No running processes found in project '<projectDir>'`.

Note the asymmetry: name-based kill queries **all** entries (incl. killed), while killing-all-in-project queries only running entries.

## 3. `killOneRunningProcess` (`src/kill/killOneRunningProcess.ts`) — core kill logic

Signature: `killOneRunningProcess(process: ServiceInfo, options: { quiet?: boolean })`. `ServiceInfo` = `{ command_name, project_dir, pid: number|null, killed_at?: number|null }`.

Logic:
1. If `process.pid` is falsy (null/0), **do nothing** (no output, no DB change).
2. `result = await killProcessTree(process.pid)` (see §5). Three outcomes:

   **`'success'`:**
   - If `!quiet`: print `[Killed '<command_name>' process with PID: <pid>]` (test asserts stdout contains `Killed`).
   - **Stale-entry branch**: if `process.killed_at` is set AND `process.killed_at < now_secs - 300` (older than 5 minutes):
     - If `!quiet`: warn `[Cleaning up stale process entry for '<command_name>' with PID: <pid>]`
     - `deleteProcessEntry(...)` (hard delete).
   - **Else** (normal): `updateProcessKilledAt({ ..., killedAt: now_secs })` — sets `killed_at` to current time; row remains, awaiting reaper deletion.

   **`'process_not_found'`:**
   - If `!quiet`: warn `[Cleaning up stale process entry for '<command_name>' with PID: <pid>]`
   - `deleteProcessEntry(...)` (hard delete — the OS process is already gone so there is nothing the log collector will clean up).

   **`'error'`:**
   - If `!quiet`: print `Error killing process '<command_name>' with PID: <pid>` (note: via `console.log`, i.e. **stdout**, not stderr).
   - **No DB change.**

Subtle: in the success path, the row is normally only *marked* `killed_at`, not deleted. Actual deletion is done later by the log collector on child exit, or by `cleanupStaleProcesses` (§6). Don't delete on success in Rust unless replicating the stale branch.

## 4. `handleKillAll` (`src/kill-all-command.ts`)

```ts
handleKillAll(options = {})  // options: { quiet?: boolean }
```
- `findAllProcesses()` → `select * from processes` — **every row across every project on the system**, with **no `killed_at` filter**. So it will also re-process already-killed-but-not-yet-reaped rows (killProcessTree on a dead pid returns `process_not_found` → deletes the row, which is harmless cleanup).
- For each: `await killOneRunningProcess(process, options)`, count.
- If count == 0: print `No running processes found` (no project qualifier). No `quietFailure` concept here.
- No name validation, no projectDir. This is the system-wide nuke.

## 5. `killProcessTree` (`src/kill/killProcessTree.ts`) + `getProcessTree` (`src/process-tree.ts`)

Return type: `'success' | 'process_not_found' | 'error'`.

1. Guard: if `pid == null || pid == 0` → **throw** `internal error: killProcessTree called with invalid PID: <pid>` (should never happen; caller already guards).
2. `pids = await getProcessTree(pid)` — collect root + all descendants.
3. If `pids.length === 0` → return `'process_not_found'`. (Note: `getProcessTree` always includes the root pid itself, so length is 0 only in degenerate cases; in practice the not-found result is realized via the per-pid ESRCH loop below making `allNotFound` stay true — but the array always contains at least the root, so the real not-found signal is `allNotFound`, see below.)
4. **Kill order: children first, root last.** Implemented by `pids.reverse()` then iterate. The tree is built breadth/DFS with root at index 0 and descendants appended, so reversing puts deepest descendants first, root last.
5. For each pid: `process.kill(pid, 'SIGTERM')`.
   - On `ESRCH` (no such process): ignore, continue.
   - On any other error: `console.warn("Warning: Could not kill process <pid>: <msg>")`, set `hasError = true`.
   - On success: set `allNotFound = false`.
6. Result: if `allNotFound` (every pid threw ESRCH) → `'process_not_found'`. Else if `hasError` → `'error'`. Else → `'success'`.

### 5.1 `getProcessTree` (`src/process-tree.ts`) — building the tree
Iterative worklist (stack): start with `[rootPid]`, `allPids = [rootPid]`. Pop a pid, find its direct children via `getChildPids`, append children to both `allPids` and the stack. Continue until stack empty. Returns `allPids` (root first, then descendants in discovery order).

`getChildPids(parentPid)` is **platform-specific**:
- **macOS (`darwin`)**: `pgrep -P <pid>`
- **Linux**: `ps -o pid --no-headers --ppid <pid>`
- **Other platforms**: return `[]` (so on Windows only the root would be killed — but root itself is in `allPids`; however killing happens via `process.kill`, and Windows has no real SIGTERM... out of scope, only darwin/linux supported).

Parsing: spawn command with `stdio ['ignore','pipe','ignore']`, accumulate stdout, on close: `trim().split('\n')`, drop empty lines, `parseInt(line.trim(), 10)`, drop `NaN`. On spawn `error` event → resolve `[]`.

**Subtle / easy to get wrong in Rust:**
- SIGTERM only (signal 15). No SIGKILL escalation, **no timeout, no wait** — the function does not wait for processes to actually die. It fires SIGTERM at the whole tree and returns immediately. Tests assert the command returns in `< 5000ms`. Do not add a blocking wait.
- Order matters: deepest descendants first, root shell last.
- The child-discovery is a *snapshot* taken before any kill; newly-forked grandchildren after the snapshot are not pursued.
- `process.kill(pid, 0)` is used elsewhere (`isProcessAlive`) as an existence probe — in Rust use `kill(pid, 0)` via `nix::sys::signal::kill(Pid, None)` and treat `EPERM` as alive, `ESRCH` as dead.

## 6. DB lifecycle & reaping interaction (context, not in kill path)
- Normal `kill` only sets `killed_at`. Final deletion is performed by either the per-process log collector on child exit, or by `cleanupStaleProcesses()` (`src/database/staleProcessCleanup.ts`), which (a) deletes running rows whose `pid` and `log_collector_pid` are both dead, and (b) **deletes every row where `killed_at is not null`**. A Rust port must keep this two-phase (mark then reap) behavior so `candle list` stops showing `RUNNING` immediately after kill (test `kill.test.ts:74` asserts `not.toContain('RUNNING')`).
- This subsystem never kills `log_collector_pid`. Only the service tree rooted at `pid`.

## 7. `handleRestart` (`src/restart-command.ts`)

Options: `{ projectDir: string; commandNames: string[]; consoleOutputFormat: 'pretty' | 'json' }`. CLI always passes `'pretty'` and calls `assertValidCommandNames` first, then `process.exit(0)` after.

Flow:
1. If `!projectDir` → throw `handleRestart: projectDir is required`.
2. **If `commandNames` empty**: load `findRunningProcessesByProjectDir(projectDir)`. If none → throw `UsageError('No running processes found in this project to restart')`. Else `commandNames = unique(runningProcesses.map(p => p.command_name))`.
3. Wrapped in try/catch (catch prints `Failed to restart: <message>` to **stderr** via `console.error`):
   a. **Snapshot phase** (before killing): build `Map<name, ProcessEntry|undefined>`; for each name store `findProcessesByCommandNameAndProjectDir(name, dir)[0]` (first match). This captures `shell`/`root` before the kill marks/deletes rows.
   b. `await handleKillCommand({ projectDir, commandNames })` — kills (no quiet flags, so it prints `[Killed ...]`).
   c. **Restart phase**: for each name, decide command source:
      - `isServiceDefinedInConfig(name, projectDir)` = true (name found in `.candle.json` via `findConfigFile`+`findServiceByName`) → pass `shell=undefined, root=undefined` so `startOneService` **reloads from config** (picks up edited `shell`/`root`).
      - Otherwise (transient process not in config) → use captured `shell`/`root` from the snapshot map.
      - Call `await startOneService({ projectDir, commandName, consoleOutputFormat, shell, root })`.

**Subtle:** restart = (mark-kill all) then (start each), sequentially. The snapshot MUST be taken before kill because kill may delete the row (stale/not-found paths), losing `shell`/`root`. `isServiceDefinedInConfig` swallows all errors → treats "no config" as "not defined" → falls back to stored command.

## 8. CLI wiring (`src/main-cli.ts`)
- Commands registered (lines 136-138): `restart [name]`, `['kill [name...]','stop [name...]']`, `kill-all`. All `strictOptions()`.
- Dispatch (lines 397-420):
  - `kill`/`stop`: `projectDir = findProjectDir(); assertValidCommandNames(commandNames); handleKillCommand({ projectDir, commandNames })`.
  - `kill-all`: `handleKillAll()` (no args).
  - `restart`: `findProjectDir()` + `assertValidCommandNames` + `handleRestart({projectDir, commandNames, consoleOutputFormat:'pretty'})` then `process.exit(0)`.
- `commandNames` is the variadic positional list (yargs `[name...]`). Empty array means "all".
- Note: `quiet` / `quietFailure` exist in the option types but are **not** exposed as CLI flags here; they are used by programmatic callers (e.g. restart passes neither, so kill output is visible). Replicate as struct fields defaulting to false.

## 9. Exact user-facing strings (test-load-bearing)
- `[Killed '<name>' process with PID: <pid>]`  (stdout; tests match substring `Killed`)
- `[Cleaning up stale process entry for '<name>' with PID: <pid>]` (stderr via console.warn)
- `Error killing process '<name>' with PID: <pid>` (stdout)
- `No running processes found for service '<name>' in project '<projectDir>'`
- `No running processes found in project '<projectDir>'`
- `No running processes found` (kill-all)
- `No running processes found in this project to restart` (UsageError, restart)
- `Failed to restart: <message>` (stderr)
- `Warning: Could not kill process <pid>: <msg>` (stderr)
- assertValidCommandNames failure: stderr contains `No service` and the bad name; non-zero exit.

## 10. External npm deps and Rust equivalents
- `@facetlayer/sqlite-wrapper` (wraps `better-sqlite3`, synchronous) → Rust: **`rusqlite`** (bundled sqlite). The `.list/.run/.insert` helpers map to `prepare`+`query_map` / `execute` / `execute`+`last_insert_rowid`. DB lives in a state dir (`getStateDirectory()` → `dirs.ts`); replicate with `directories`/`dirs` crate or env override `CANDLE_DATABASE_DIR`.
- `yargs` → Rust: **`clap`** (derive). Variadic positional, subcommand aliases (`kill`/`stop`).
- `child_process.spawn` for `pgrep`/`ps` → **`std::process::Command`**.
- `process.kill(pid, signal)` → **`nix`** crate: `nix::sys::signal::kill(Pid::from_raw(pid), Signal::SIGTERM)` and `kill(pid, None)` for the signal-0 liveness probe. Match `nix::errno::Errno::ESRCH` and `EPERM`.

## 11. Rust reimplementation notes (modules/functions + ordering)
Build bottom-up:
1. **`db::process_table`** — `ProcessEntry` struct (fields exactly as §1.1), and functions: `find_by_name_and_dir`, `find_running_by_dir`, `find_all`, `find_all_running`, `find_all_killed`, `update_killed_at`, `delete_entry`. Use unix-seconds timestamps. (Depends on a `db` connection module + schema migration.)
2. **`process_tree`** — `get_process_tree(root_pid) -> Vec<i32>` (iterative worklist) and platform `get_child_pids` (darwin `pgrep -P`, linux `ps --ppid`, else empty). Parse stdout as in §5.1.
3. **`process_alive`** — `is_process_alive(pid)` via `kill(pid, None)` mapping EPERM→true, ESRCH→false.
4. **`kill::kill_process_tree(pid) -> KillResult{Success,ProcessNotFound,Error}`** — reverse order, SIGTERM each, ESRCH-ignore, track `all_not_found`/`has_error`, no wait/timeout. (Depends on 2.)
5. **`kill::kill_one_running_process(entry, opts{quiet})`** — the success/not-found/error branching incl. 5-minute (300s) stale-delete-vs-mark logic and exact output strings. (Depends on 1, 4.)
6. **`commands::kill::handle_kill_command(opts)`** — name dedupe + per-name vs all-running branching + the two "No running processes…" messages. (Depends on 5, 1.)
7. **`commands::kill_all::handle_kill_all(opts)`** — `find_all` + loop + `No running processes found`. (Depends on 5, 1.)
8. **`commands::restart::handle_restart(opts)`** — needs `config` lookup (`find_config_file`/`find_service_by_name` → `is_service_defined_in_config`), snapshot map, call handle_kill_command, then `start_one_service` per name with config-reload-vs-stored-shell logic. (Depends on 6, the config module, and `start_one_service` — which is outside this subsystem and must exist first.)
9. **CLI dispatch** (clap): wire `kill`/`stop` (alias), `kill-all`, `restart`; resolve `project_dir` via config walk-up; run `assert_valid_command_names` before kill/restart. `restart` exits 0 on completion.

Cross-subsystem dependency to flag: `restart` depends on the **start** subsystem (`start_one_service`) and the **config** subsystem (`find_config_file`/`find_service_by_name`); and correct post-kill `list` behavior depends on the **stale-cleanup/reaper** subsystem (`cleanup_stale_processes` deleting `killed_at IS NOT NULL` rows). Port those alongside or stub them.