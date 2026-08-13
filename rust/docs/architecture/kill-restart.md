# Kill & restart

## 0. Scope & files
This subsystem covers three CLI commands and their shared helpers. The Rust implementation lives under `rust/src/`; `src/...` references point at the original Node/TypeScript source, kept as the historical source of truth.

- `candle kill [name...]` (alias `stop`) → `handle_kill_command` (`rust/src/kill/mod.rs`; mirrors `src/kill-command.ts`)
- `candle kill-all` → `handle_kill_all` (`rust/src/kill/mod.rs`; mirrors `src/kill-all-command.ts`)
- `candle restart [name...]` → `handle_restart` (`rust/src/commands/restart.rs`; mirrors `src/restart-command.ts`)
- Shared kill helpers: `kill_one_running_process`, `kill_process_tree` (`rust/src/kill/mod.rs`), `get_process_tree` (`rust/src/process_tree.rs`)
- DB layer: `rust/src/db/process_table.rs`, schema in `rust/src/db/mod.rs`
- Liveness check: `rust/src/process_alive.rs`

## 1. Data model

### 1.1 `processes` table schema (`rust/src/db/mod.rs`; mirrors `src/database/database.ts:16-27`)
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
- All timestamps are **Unix seconds** (`Math.floor(Date.now() / 1000)` in the original), not millis. In Rust: `SystemTime::now().duration_since(UNIX_EPOCH).as_secs()`.
- `pid` is the PID of the service's shell process (root of the tree). `log_collector_pid` is a separate supervising process (not killed by this subsystem — see §6).
- The `ProcessEntry` struct maps 1:1 to columns. `killed_at` and `root` are nullable/optional.

### 1.2 Queries used (exact SQL)
- `find_processes_by_command_name_and_project_dir(name, dir)` → `select * from processes where command_name = ? and project_dir = ?` (note: **does not filter on `killed_at`** — returns killed entries too).
- `find_running_processes_by_project_dir(dir)` → `select * from processes where project_dir = ? and killed_at is null`
- `find_all_processes()` → `select * from processes` (no filter at all — see §4 note).
- `update_process_killed_at`: `update processes set killed_at = ? where command_name = ? and project_dir = ? and pid = ?`
- `delete_process_entry`: `delete from processes where command_name = ? and project_dir = ? and pid = ?`

The natural key used for update/delete is the triple **(command_name, project_dir, pid)**, not `id`.

## 2. `handle_kill_command` (`rust/src/kill/mod.rs`)

Options: `{ project_dir, command_names?, quiet_failure?, quiet? }`.

`project_dir` comes from `find_project_dir()` (walks up from cwd to find `.candle.json`; errors with `MissingSetupFileError` if none). The CLI calls `assert_valid_command_names(command_names)` **before** kill — this validates each name exists as a service in config and exits with stderr `No service ...<name>...` if not (this is why `kill nonexistent-service` fails with exit code != 0 and stderr mentioning the name; the kill body is never reached).

Control flow:
1. `command_names = req.command_names` or `[]`.
2. **If names given**: dedupe via a set, then for each unique name call `kill_by_command_name`:
   - `find_processes_by_command_name_and_project_dir(name, project_dir)` (all matching entries, including already-killed).
   - For each, `kill_one_running_process(process, options)`; increment counter.
   - If counter == 0 and `!quiet_failure`: print `No running processes found for service '<name>' in project '<projectDir>'`.
3. **If no names given**: `find_running_processes_by_project_dir(project_dir)` (only `killed_at is null`). Kill each.
   - If counter == 0 and `!quiet_failure`: print `No running processes found in project '<projectDir>'`.

Note the asymmetry: name-based kill queries **all** entries (incl. killed), while killing-all-in-project queries only running entries.

## 3. `kill_one_running_process` (`rust/src/kill/mod.rs`) — core kill logic

Signature: `kill_one_running_process(process, options { quiet? })`. The process value carries `{ command_name, project_dir, pid: Option<i32>, killed_at: Option<i64> }`.

Logic:
1. If `process.pid` is falsy (null/0), **do nothing** (no output, no DB change).
2. `result = kill_process_tree(process.pid)` (see §5). Three outcomes:

   **`'success'`:**
   - If `!quiet`: print `[Killed '<command_name>' process with PID: <pid>]` (test asserts stdout contains `Killed`).
   - **Stale-entry branch**: if `process.killed_at` is set AND `process.killed_at < now_secs - 300` (older than 5 minutes):
     - If `!quiet`: warn `[Cleaning up stale process entry for '<command_name>' with PID: <pid>]`
     - `delete_process_entry(...)` (hard delete).
   - **Else** (normal): `update_process_killed_at({ ..., killed_at: now_secs })` — sets `killed_at` to current time; row remains, awaiting reaper deletion.

   **`'process_not_found'`:**
   - If `!quiet`: warn `[Cleaning up stale process entry for '<command_name>' with PID: <pid>]`
   - `delete_process_entry(...)` (hard delete — the OS process is already gone so there is nothing the log collector will clean up).

   **`'error'`:**
   - If `!quiet`: print `Error killing process '<command_name>' with PID: <pid>` (note: via stdout, not stderr).
   - **No DB change.**

Subtle: in the success path, the row is normally only *marked* `killed_at`, not deleted. Actual deletion is done later by the log collector on child exit, or by `cleanup_stale_processes` (§6). The success path does not delete except in the stale branch.

## 4. `handle_kill_all` (`rust/src/kill/mod.rs`)

`handle_kill_all(options { quiet? })`.

- `find_all_processes()` → `select * from processes` — **every row across every project on the system**, with **no `killed_at` filter**. So it will also re-process already-killed-but-not-yet-reaped rows (`kill_process_tree` on a dead pid returns `process_not_found` → deletes the row, which is harmless cleanup).
- For each: `kill_one_running_process(process, options)`, count.
- If count == 0: print `No running processes found` (no project qualifier). No `quiet_failure` concept here.
- No name validation, no project_dir. This is the system-wide nuke.

## 5. `kill_process_tree` (`rust/src/kill/mod.rs`) + `get_process_tree` (`rust/src/process_tree.rs`)

Return type: `'success' | 'process_not_found' | 'error'`.

1. Guard: if `pid == null || pid == 0` → **error** `internal error: killProcessTree called with invalid PID: <pid>` (should never happen; caller already guards).
2. `pids = get_process_tree(pid)` — collect root + all descendants.
3. If `pids.len() == 0` → return `'process_not_found'`. (Note: `get_process_tree` always includes the root pid itself, so length is 0 only in degenerate cases; in practice the not-found result is realized via the per-pid ESRCH loop below making `all_not_found` stay true — but the array always contains at least the root, so the real not-found signal is `all_not_found`, see below.)
4. **Kill order: children first, root last.** Implemented by reversing `pids` then iterating. The tree is built breadth/DFS with root at index 0 and descendants appended, so reversing puts deepest descendants first, root last.
5. For each pid: send `SIGTERM`.
   - On `ESRCH` (no such process): ignore, continue.
   - On any other error: warn `Warning: Could not kill process <pid>: <msg>`, set `has_error = true`.
   - On success: set `all_not_found = false`.
6. Result: if `all_not_found` (every pid threw ESRCH) → `'process_not_found'`. Else if `has_error` → `'error'`. Else → `'success'`.

### 5.1 `get_process_tree` (`rust/src/process_tree.rs`) — building the tree
Iterative worklist (stack): start with `[root_pid]`, `all_pids = [root_pid]`. Pop a pid, find its direct children via `get_child_pids`, append children to both `all_pids` and the stack. Continue until stack empty. Returns `all_pids` (root first, then descendants in discovery order).

`get_child_pids(parent_pid)` is **platform-specific**:
- **macOS (`darwin`)**: `pgrep -P <pid>`
- **Linux**: `ps -o pid --no-headers --ppid <pid>`
- **Other platforms**: return `[]` (so on Windows only the root would be killed — but root itself is in `all_pids`; only darwin/linux are supported).

Parsing: spawn command with `stdio ['ignore','pipe','ignore']`, accumulate stdout, on close: `trim().split('\n')`, drop empty lines, parse each as an int, drop non-numeric. On spawn `error` → `[]`.

**Subtle / easy to get wrong:**
- SIGTERM only (signal 15). No SIGKILL escalation, **no timeout, no wait** — the function does not wait for processes to actually die. It fires SIGTERM at the whole tree and returns immediately. Tests assert the command returns in `< 5000ms`. There is no blocking wait.
- Order matters: deepest descendants first, root shell last.
- The child-discovery is a *snapshot* taken before any kill; newly-forked grandchildren after the snapshot are not pursued.
- The signal-0 existence probe used elsewhere (`is_process_alive`) is `kill(pid, None)` via `nix::sys::signal::kill(Pid, None)`, treating `EPERM` as alive, `ESRCH` as dead.

## 6. DB lifecycle & reaping interaction (context, not in kill path)
- Normal `kill` only sets `killed_at`. Final deletion is performed by either the per-process log collector on child exit, or by `cleanup_stale_processes()` (`rust/src/db/cleanup.rs`; mirrors `src/database/staleProcessCleanup.ts`), which (a) deletes running rows whose `pid` and `log_collector_pid` are both dead, and (b) **deletes every row where `killed_at is not null`**. This two-phase (mark then reap) behavior is what makes `candle list` stop showing `RUNNING` immediately after kill (test `kill.test.ts:74` asserts `not.toContain('RUNNING')`).
- This subsystem never kills `log_collector_pid`. Only the service tree rooted at `pid`.

## 7. `handle_restart` (`rust/src/commands/restart.rs`)

Options: `{ project_dir, command_names, console_output_format: 'pretty' | 'json' }`. The CLI always passes `'pretty'` and calls `assert_valid_command_names` first, then exits 0 after.

Flow:
1. If `project_dir` is empty → error `handleRestart: projectDir is required`.
2. **If `command_names` empty**: load `find_running_processes_by_project_dir(project_dir)`. If none → `UsageError('No running processes found in this project to restart')`. Else `command_names = unique(running.map(p => p.command_name))`.
3. Wrapped in try/catch (catch prints `Failed to restart: <message>` to **stderr**):
   a. **Snapshot phase** (before killing): build `Map<name, ProcessEntry?>`; for each name store `find_processes_by_command_name_and_project_dir(name, dir)[0]` (first match). This captures `shell`/`root` before the kill marks/deletes rows.
   b. `handle_kill_command({ project_dir, command_names })` — kills (no quiet flags, so it prints `[Killed ...]`).
   c. **Restart phase**: for each name, decide command source:
      - `is_service_defined_in_config(name, project_dir)` = true (name found in `.candle.json` via `find_config_file` + `find_service_by_name`) → pass `shell=None, root=None` so `start_one_service` **reloads from config** (picks up edited `shell`/`root`).
      - Otherwise (transient process not in config) → use captured `shell`/`root` from the snapshot map.
      - Call `start_one_service({ project_dir, command_name, console_output_format, shell, root })`.

**Subtle:** restart = (mark-kill all) then (start each), sequentially. The snapshot MUST be taken before kill because kill may delete the row (stale/not-found paths), losing `shell`/`root`. `is_service_defined_in_config` swallows all errors → treats "no config" as "not defined" → falls back to stored command.

## 8. CLI wiring (mirrors `src/main-cli.ts`)
- Commands registered: `restart [name]`, `['kill [name...]','stop [name...]']`, `kill-all`.
- Dispatch:
  - `kill`/`stop`: `project_dir = find_project_dir(); assert_valid_command_names(command_names); handle_kill_command({ project_dir, command_names })`.
  - `kill-all`: `handle_kill_all()` (no args).
  - `restart`: `find_project_dir()` + `assert_valid_command_names` + `handle_restart({ project_dir, command_names, console_output_format: 'pretty' })` then exit 0.
- `command_names` is the variadic positional list (`[name...]`). Empty list means "all".
- Note: `quiet` / `quiet_failure` exist in the option types but are **not** exposed as CLI flags; they are used by programmatic callers (e.g. restart passes neither, so kill output is visible). They are struct fields defaulting to false.

## 9. Exact user-facing strings (test-load-bearing)
- `[Killed '<name>' process with PID: <pid>]`  (stdout; tests match substring `Killed`)
- `[Cleaning up stale process entry for '<name>' with PID: <pid>]` (stderr / warn)
- `Error killing process '<name>' with PID: <pid>` (stdout)
- `No running processes found for service '<name>' in project '<projectDir>'`
- `No running processes found in project '<projectDir>'`
- `No running processes found` (kill-all)
- `No running processes found in this project to restart` (UsageError, restart)
- `Failed to restart: <message>` (stderr)
- `Warning: Could not kill process <pid>: <msg>` (stderr)
- assert_valid_command_names failure: stderr contains `No service` and the bad name; non-zero exit.

## 10. Crates used (replacing the original npm deps)
- `@facetlayer/sqlite-wrapper` (wraps `better-sqlite3`, synchronous) → **`rusqlite`** (bundled sqlite). The `.list/.run/.insert` helpers map to `prepare`+`query_map` / `execute` / `execute`+`last_insert_rowid`. The DB lives in a state dir (`get_state_directory()` → `rust/src/dirs.rs`), with env override `CANDLE_DATABASE_DIR`.
- `yargs` → **`clap`** (derive). Variadic positional, subcommand aliases (`kill`/`stop`).
- `child_process.spawn` for `pgrep`/`ps` → **`std::process::Command`**.
- `process.kill(pid, signal)` → **`nix`** crate: `nix::sys::signal::kill(Pid::from_raw(pid), Signal::SIGTERM)` and `kill(pid, None)` for the signal-0 liveness probe. Matches `nix::errno::Errno::ESRCH` and `EPERM`.

## 11. Cross-subsystem dependencies
- `restart` depends on the **start** subsystem (`start_one_service`) and the **config** subsystem (`find_config_file` / `find_service_by_name`, surfaced as `is_service_defined_in_config`).
- Correct post-kill `list` behavior depends on the **stale-cleanup/reaper** subsystem (`cleanup_stale_processes` deleting `killed_at IS NOT NULL` rows — see §6).
