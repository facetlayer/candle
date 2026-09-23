# Kill & restart

## 0. Scope & files
This subsystem covers three CLI commands and their shared helpers, all under `rust/src/`.

- `candle kill [name...]` (alias `stop`) → `handle_kill_command` (`rust/src/kill/mod.rs`)
- `candle kill-all` → `handle_kill_all` (`rust/src/kill/mod.rs`)
- `candle restart [name...]` → `handle_restart` (`rust/src/commands/restart.rs`)
- Shared kill helpers: `kill_one_running_process`, `kill_process_tree_and_wait`, `kill_process_tree` (`rust/src/kill/mod.rs`), `get_process_tree` (`rust/src/process_tree.rs`)
- DB layer: `rust/src/db/process_table.rs`, schema in `rust/src/db/mod.rs`
- Liveness check: `rust/src/process_alive.rs`

## 1. Data model

### 1.1 `processes` table schema (`rust/src/db/mod.rs`)
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
    root text,
    run_id integer
)
```
Key semantics:
- A process is considered **running** iff `killed_at IS NULL`.
- All timestamps are **Unix seconds** (`SystemTime::now().duration_since(UNIX_EPOCH).as_secs()`), not millis.
- `pid` is the PID of the service's shell process (root of the tree). `log_collector_pid` is a separate supervising process (not killed by this subsystem — see §6).
- `run_id` identifies the service's current run (see [database.md](database.md)); this subsystem doesn't read it.
- The `ProcessEntry` struct maps 1:1 to columns. `log_collector_pid`, `killed_at`, `shell`, and `root` are `Option`s.

### 1.2 Queries used (exact SQL)
- `find_processes_by_command_name_and_project_dir(name, dir)` → `select * from processes where command_name = ? and project_dir = ?` (note: **does not filter on `killed_at`** — returns killed entries too).
- `find_running_processes_by_project_dir(dir)` → `select * from processes where project_dir = ? and killed_at is null`
- `find_all_processes()` → `select * from processes` (no filter at all — see §4 note).
- `update_process_killed_at`: `update processes set killed_at = ? where command_name = ? and project_dir = ? and pid = ?`
- `delete_process_entry`: `delete from processes where command_name = ? and project_dir = ? and pid = ?`

The natural key used for update/delete is the triple **(command_name, project_dir, pid)**, not `id`.

## 2. `handle_kill_command` (`rust/src/kill/mod.rs`)

Signature: `handle_kill_command(conn, project_dir, command_names, quiet_failure, quiet) -> rusqlite::Result<()>`.

`project_dir` comes from the command's `ProjectScope` (`--project-dir`, else walk up from cwd to find `.candle.json`; see [cli.md](cli.md)). Unless `--project-dir` was given explicitly, `cmd_kill` calls `assert_valid_command_names(conn, cwd, command_names)` **before** kill. A name is valid if it has any process row in the project or resolves to a configured service; otherwise it exits with stderr `No service '<name>' configured for directory: <dir>` (this is why `kill nonexistent-service` fails with exit code != 0 and stderr mentioning the name; the kill body is never reached).

Control flow:
1. `command_names` is the positional list (possibly empty).
2. **If names given**: dedupe via a set (first-occurrence order), then for each unique name call `kill_by_command_name`:
   - `find_processes_by_command_name_and_project_dir(name, project_dir)` (all matching entries, including already-killed).
   - For each, `kill_one_running_process(process, options)`; increment the counter **only when it returns `true`** (see below).
   - If counter == 0 and `!quiet_failure`: print `No running processes found for service '<name>' in project '<projectDir>'`.
3. **If no names given**: `find_running_processes_by_project_dir(project_dir)` (only `killed_at is null`). Kill each.
   - If counter == 0 and `!quiet_failure`: print `No running processes found in project '<projectDir>'`.

Note the asymmetry: name-based kill queries **all** entries (incl. killed), while killing-all-in-project queries only running entries.

**The counter counts kills, not rows.** Because the name-based query includes already-killed rows, a
row left over from a previous kill gets swept here — `kill_process_tree` on its dead pid returns
`ProcessNotFound`, and the row is deleted. That sweep is garbage collection, not a kill, and
`kill_one_running_process` returns `false` for it. Counting it would suppress the "No running
processes found" message, making the output of `candle kill <name>` depend on whether the reaper had
already cleared the previous kill's row — the message would silently vanish (the sweep's own notice
goes to *stderr*), which is exactly the flake this rule prevents.

## 3. `kill_one_running_process` (`rust/src/kill/mod.rs`) — core kill logic

Signature: `kill_one_running_process(conn, entry: &ProcessEntry, quiet: bool) -> rusqlite::Result<bool>`.

The returned `bool` is **"was there a live process to signal"**, which callers use as their kill
counter (see above): the terminated/escalated and error outcomes return `true`, while a zero pid or
`ProcessNotFound` returns `false`.

Logic:
1. If `entry.pid == 0`, **do nothing** (no output, no DB change; returns `false`).
2. If the row has no `killed_at`, mark it `killed_at = now_secs` **before** signalling, so a monitor whose process dies during its startup grace period can tell the stop was deliberate (see start-flow.md, monitor step 5) and doesn't log a failed start that `ps` would show as `FAILED`.
3. `outcome = kill_process_tree_and_wait(entry.pid, KILL_GRACE_PERIOD)` (see §5.2). Outcomes:

   **`Terminated` / `Escalated`:**
   - If `!quiet` and the outcome is `Escalated`: print to stderr `[Process '<command_name>' (PID <pid>) did not exit 5s after SIGTERM; sent SIGKILL]` (before the `[Killed ...]` line).
   - If `!quiet`: print `[Killed '<command_name>' process with PID: <pid>]` (test asserts stdout contains `Killed`).
   - **Stale-entry branch**: if `process.killed_at` is set AND `process.killed_at < now_secs - 300` (older than 5 minutes):
     - If `!quiet`: warn `[Cleaning up stale process entry for '<command_name>' with PID: <pid>]`
     - `delete_process_entry(...)` (hard delete).
   - **Else** (normal): `update_process_killed_at(conn, command_name, project_dir, pid, now_secs)` — sets `killed_at` to current time; row remains, awaiting reaper deletion.

   **`ProcessNotFound`:**
   - If `!quiet` and the row has no `killed_at` (it claimed to be running): warn `[Cleaning up stale process entry for '<command_name>' with PID: <pid>]`. A row already marked killed (the second kill inside `restart`) is swept silently.
   - `delete_process_entry(...)` (hard delete — the OS process is already gone so there is nothing the monitor will clean up).

   **`Error`** (a signal failed, or a process survived even `SIGKILL`):
   - If `!quiet`: print `Error killing process '<command_name>' with PID: <pid>` (note: via stdout, not stderr).
   - Undo the early `killed_at` mark from step 2 (`clear_process_killed_at`), since the process is still running.
   - **No DB change.**

Subtle: in the success path, the row is normally only *marked* `killed_at`, not deleted. Actual deletion is done later by the monitor on child exit, or by `cleanup_stale_processes` (§6). The success path does not delete except in the stale branch.

## 4. `handle_kill_all` (`rust/src/kill/mod.rs`)

`handle_kill_all(conn, quiet)`.

- `find_all_processes()` → `select * from processes` — **every row across every project on the system**, with **no `killed_at` filter**. So it will also re-process already-killed-but-not-yet-reaped rows (`kill_process_tree` on a dead pid returns `ProcessNotFound` → deletes the row, which is harmless cleanup).
- For each: `kill_one_running_process(process, options)`, counting only real kills (same rule as above).
- If count == 0: print `No running processes found` (no project qualifier). No `quiet_failure` concept here.
- No name validation, no project_dir. This is the system-wide nuke.

## 5. `kill_process_tree` (`rust/src/kill/mod.rs`) + `get_process_tree` (`rust/src/process_tree.rs`)

Return type: `KillResult::{Success, ProcessNotFound, Error}`.

1. Guard: if `pid <= 0` → **panic** `internal error: kill_process_tree called with invalid PID: <pid>` (an invariant violation; callers already guard).
2. `pids = get_process_tree(pid)` — collect root + all descendants.
3. If `pids.len() == 0` → return `ProcessNotFound`. (Note: `get_process_tree` always includes the root pid itself, so length is 0 only in degenerate cases; in practice the not-found result is realized via the per-pid ESRCH loop below making `all_not_found` stay true — but the array always contains at least the root, so the real not-found signal is `all_not_found`, see below.)
4. **Kill order: children first, root last.** Implemented by reversing `pids` then iterating. The tree is built breadth/DFS with root at index 0 and descendants appended, so reversing puts deepest descendants first, root last.
5. For each pid: send `SIGTERM`.
   - On `ESRCH` (no such process): ignore, continue.
   - On any other error: warn `Warning: Could not kill process <pid>: <msg>`, set `has_error = true`.
   - On success: set `all_not_found = false`.
6. Result: if `all_not_found` (every pid threw ESRCH) → `ProcessNotFound`. Else if `has_error` → `Error`. Else → `Success`.

`kill_process_tree` itself only sends `SIGTERM` and never waits. The wait and escalation live in `kill_process_tree_and_wait` (§5.2).

### 5.2 `kill_process_tree_and_wait(pid, grace) -> KillOutcome` (`rust/src/kill/mod.rs`)

Used by `kill_one_running_process` with `grace = KILL_GRACE_PERIOD` (5s). A service that traps or ignores `SIGTERM` would otherwise keep running while Candle's records said it was dead.
1. Snapshot the whole tree with `get_process_tree(pid)` **before** signalling.
2. `kill_process_tree(pid)`. `ProcessNotFound` → `KillOutcome::ProcessNotFound`; `Error` → `KillOutcome::Error`.
3. On `Success`, poll `is_process_alive` for **every pid in the snapshot** every `KILL_POLL_INTERVAL` (20ms) for up to `grace`. If they all exit → `Terminated`.
4. Otherwise, for each snapshot pid still alive, take its current tree (picking up anything it forked since), dedupe, and send `SIGKILL` to all of them, children first. Errors are ignored.
5. Wait up to `SIGKILL_WAIT` (1s) for every targeted pid. If they all exit → `Escalated`; else → `Error`.

`KillOutcome` is `Terminated | Escalated | ProcessNotFound | Error`. The snapshot matters: when the root shell exits on `SIGTERM`, a child that ignored it is reparented to init, so a tree re-read from the root would no longer find it.

### 5.1 `get_process_tree` (`rust/src/process_tree.rs`) — building the tree
Iterative worklist (stack): start with `[root_pid]`, `all_pids = [root_pid]`. Pop a pid, find its direct children via `get_child_pids`, append children to both `all_pids` and the stack. Continue until stack empty. Returns `all_pids` (root first, then descendants in discovery order).

`get_child_pids(parent_pid)` is **platform-specific**:
- **macOS (`darwin`)**: `pgrep -P <pid>`
- **Linux**: `ps -o pid --no-headers --ppid <pid>`
- **Other platforms**: return `[]` (so on Windows only the root would be killed — but root itself is in `all_pids`; only darwin/linux are supported).

Parsing (`run_command_for_pids`): run via `std::process::Command` with stdin/stderr null and stdout piped, then `trim().split('\n')`, drop empty lines, parse each as an int, drop non-numeric. On spawn failure → `[]`.

**Subtle / easy to get wrong:**
- `kill_process_tree` sends SIGTERM only (signal 15) and returns immediately. `kill` therefore blocks for up to `KILL_GRACE_PERIOD + SIGKILL_WAIT` (6s) per entry only when a service ignores SIGTERM; a well-behaved service returns as soon as its root exits.
- Order matters: deepest descendants first, root shell last (for both the SIGTERM and SIGKILL passes).
- The child-discovery for the SIGTERM pass is a *snapshot* taken before any kill; grandchildren forked after it are only reached if escalation re-snapshots the tree.
- The signal-0 existence probe (`is_process_alive`) is `libc::kill(pid, 0)`, treating `EPERM` as alive, `ESRCH` as dead.

## 6. DB lifecycle & reaping interaction (context, not in kill path)
- Normal `kill` only sets `killed_at`. Final deletion is performed by either the per-service monitor (`candle --monitor`) on child exit, or by `cleanup_stale_processes()` (`rust/src/db/cleanup.rs`), which (a) deletes running rows whose `pid` and `log_collector_pid` are both dead, and (b) **deletes every row where `killed_at is not null`**. This two-phase (mark then reap) behavior is what makes `candle list` stop showing `RUNNING` immediately after kill (test `kill.test.ts:74` asserts `not.toContain('RUNNING')`).
- This subsystem never kills `log_collector_pid`. Only the service tree rooted at `pid`.
- `start` reuses this path: `start_one_service` calls `handle_kill_command(conn, project_dir, [name], quiet_failure = true, quiet = false)` to replace a running instance, then waits up to 2s for the old shell and monitor pids to exit (see [start-flow.md](start-flow.md)).

## 7. `handle_restart` (`rust/src/commands/restart.rs`)

Signature: `handle_restart(conn, project_dir, command_names) -> Result<Vec<String>, CandleError>`, returning the resolved list of restarted names. `cmd_restart` resolves the project with `configured_project_dir_or_exit`, calls `assert_valid_command_names` first, and after the handler either watches the new launches (`watch_started_services`, interactive mode) or prints the `Run 'candle logs ...' to see logs.` hint, then exits 0.

Flow:
1. **If `command_names` empty**: load `find_running_processes_by_project_dir(project_dir)`. If none → `CandleError::UsageError("No running processes found in this project to restart")` (propagates; CLI prints to stderr, exit 1). Else `command_names` = the running rows' names, deduped in first-seen order.
2. Wrapped in a closure so any failure can be reported uniformly (an error is returned as `Generic("Failed to restart: <message>")`, which the CLI prints to **stderr** before exiting 1):
   a. **Snapshot phase** (before killing): for each name store the first row from `find_processes_by_command_name_and_project_dir(name, dir)`, if any. This captures `shell`/`root` before the kill marks/deletes rows.
   b. `handle_kill_command(conn, project_dir, names, false, false)` — kills (no quiet flags, so it prints `[Killed ...]`).
   c. **Restart phase**: for each name, decide command source:
      - `is_service_defined_in_config(project_dir, name)` = true (name found in `.candle.json` via `find_config_file` + `find_service_by_name`) → pass `shell=None, root=None` so `start_one_service` **reloads from config** (picks up edited `shell`/`root`).
      - Otherwise (transient process not in config) → use captured `shell`/`root` from the snapshot map.
      - Call `start_one_service(conn, RunOptions { command_name, project_dir, shell, root, enable_stdin: false, check_start: false })`.

**Subtle:** restart = (mark-kill all) then (start each), sequentially. The snapshot MUST be taken before kill because kill may delete the row (stale/not-found paths), losing `shell`/`root`. `is_service_defined_in_config` swallows all errors → treats "no config" as "not defined" → falls back to stored command.

## 8. CLI wiring (`rust/src/main.rs`)
- Commands: `restart [name...]`, `kill [name...]` (alias `stop`), `kill-all`. `kill` and `restart` accept `--project-dir`; `restart` also takes `--watch` / `--bg` / `--exit-after-ms`.
- Dispatch (after `open_db()` + `maybe_run_cleanup`):
  - `cmd_kill`: `project_dir_or_exit(scope)`; `assert_valid_command_names` unless `--project-dir` was explicit; `handle_kill_command(conn, project_dir, names, false, false)`. A DB error prints `candle: database error: <e>` and exits 1.
  - `cmd_kill_all`: `handle_kill_all(conn, false)` (no args).
  - `cmd_restart`: see §7.
- `command_names` is the variadic positional list (`[name...]`). Empty list means "all".
- Note: `quiet` / `quiet_failure` are plain `bool` parameters, **not** CLI flags; the CLI passes `false` for both (so kill output is visible). `start_one_service` passes `quiet_failure = true`.

## 9. Exact user-facing strings (test-load-bearing)
- `[Killed '<name>' process with PID: <pid>]`  (stdout; tests match substring `Killed`)
- `[Cleaning up stale process entry for '<name>' with PID: <pid>]` (stderr / warn)
- `Error killing process '<name>' with PID: <pid>` (stdout)
- `[Process '<name>' (PID <pid>) did not exit 5s after SIGTERM; sent SIGKILL]` (stderr, on escalation)
- `No running processes found for service '<name>' in project '<projectDir>'`
- `No running processes found in project '<projectDir>'`
- `No running processes found` (kill-all)
- `No running processes found in this project to restart` (UsageError, restart)
- `Failed to restart: <message>` (stderr, exit 1)
- `Warning: Could not kill process <pid>: <msg>` (stderr)
- assert_valid_command_names failure: stderr `No service '<name>' configured for directory: <dir>`; non-zero exit.

## 10. Crates used
- **`rusqlite`** (bundled sqlite), synchronous. The DB lives in a state dir (`get_state_directory()` → `rust/src/dirs.rs`), with env override `CANDLE_DATABASE_DIR`.
- CLI parsing is the hand-rolled parser in `rust/src/cli/parser.rs` (no `clap`). Variadic positional, alias `stop` → `kill`.
- `pgrep`/`ps` are run with **`std::process::Command`**.
- Signals go through **`libc`**: `libc::kill(pid, SIGTERM)` / `SIGKILL`, and `libc::kill(pid, 0)` for the signal-0 liveness probe, checking `errno` for `ESRCH` and `EPERM`. (There is no `nix` dependency.)

## 11. Cross-subsystem dependencies
- `restart` depends on the **start** subsystem (`start_one_service`) and the **config** subsystem (`find_config_file` / `find_service_by_name`, surfaced as `is_service_defined_in_config`).
- Correct post-kill `list` behavior depends on the **stale-cleanup/reaper** subsystem (`cleanup_stale_processes` deleting `killed_at IS NOT NULL` rows — see §6).
