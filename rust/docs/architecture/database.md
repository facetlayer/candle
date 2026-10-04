# Database subsystem

## 1. Overview

The subsystem is a single SQLite database that stores: (a) the registry of launched background processes, (b) captured process output (logs) with a table of the services they belong to, (c) a stdin message queue per service, and (d) a last-cleanup timestamp. It is accessed concurrently by multiple OS processes (the CLI, the per-service `candle --monitor` process, the MCP server), so WAL + busy-timeout are mandatory.

The Rust implementation lives under `rust/src/db/` (`mod.rs`, `process_table.rs`, `stdin_messages.rs`, `cleanup.rs`), with directory resolution in `rust/src/dirs.rs` and liveness checks in `rust/src/process_alive.rs`. It uses `rusqlite` (bundled SQLite) with hand-written SQL.

## 2. Database file location, naming, creation

`get_state_directory()` in `rust/src/dirs.rs` resolves the **state dir** with this precedence order:

1. `CANDLE_DATABASE_DIR` env var → used verbatim.
2. `XDG_STATE_HOME` env var → `join($XDG_STATE_HOME, "candle")`.
3. Default → `join(home, ".local", "state", "candle")` i.e. `~/.local/state/candle`.

An empty env value counts as unset. The pure `resolve_state_dir(...)` helper holds the precedence logic; `candle_db_path()` is `<state dir>/candle.db`.

DB bootstrap lives in `rust/src/db/mod.rs`. `get_database(override_dir)`:
- `state_dir` = `override_dir` if given, else `get_state_directory()`.
- `state_dir` is created recursively (`create_dir_all`).
- DB file = `join(state_dir, "candle.db")` — **filename is exactly `candle.db`**.
- It then calls `open_database_at(path)`, which is also used directly by monitor mode (it is handed the absolute `candle.db` path).
- After opening, every connection runs:
  ```sql
  PRAGMA journal_mode=WAL;
  PRAGMA busy_timeout=30000;    -- 30 seconds
  ```
  then runs the schema migration (§4).
- There is **no** process-wide singleton connection: each call opens a fresh connection and sets WAL + busy_timeout on it. This is simple and correct for the multi-process usage candle relies on, since each connection independently establishes the required pragmas.

Subtle / easy to get wrong:
- WAL mode creates sidecar files `candle.db-wal` and `candle.db-shm` next to `candle.db` (`erase-database` removes all three, §10).
- `busy_timeout=30000` (ms) is essential — multiple processes write concurrently; without it you get `SQLITE_BUSY`.
- `PRAGMA journal_mode=WAL` returns a row (`"wal"`); harmless.

## 3. Exact schema

Defined in `rust/src/db/mod.rs` (`TABLE_STATEMENTS`, `INDEX_STATEMENTS`). The Vitest suite opens the same database with raw SQL, so the schema is part of its contract. All `integer` timestamps are **Unix epoch seconds**. Default timestamp expression is `strftime('%s','now')` (returns a string in SQLite but stored in an integer column → stored as integer text/affinity integer; epoch seconds).

### Table `processes`
```sql
create table processes(
    id integer primary key autoincrement,
    command_name text not null,
    project_dir text not null,
    pid integer not null,
    log_collector_pid integer,            -- nullable
    start_time integer not null,
    created_at integer not null default (strftime('%s', 'now')),
    killed_at integer,                     -- nullable; NULL = still "running"
    shell text,                            -- nullable
    root text,                             -- nullable
    run_id integer,                        -- nullable
    transient integer                      -- nullable boolean
)
```
| column | type | nullable | notes |
|---|---|---|---|
| id | INTEGER PK AUTOINCREMENT | no | |
| command_name | TEXT | no | service name |
| project_dir | TEXT | no | absolute project dir |
| pid | INTEGER | no | OS pid of the service process |
| log_collector_pid | INTEGER | **yes** | pid of the supervising monitor process |
| start_time | INTEGER | no | epoch seconds, set by app code (`create_process_entry`) |
| created_at | INTEGER | no | default `strftime('%s','now')` |
| killed_at | INTEGER | **yes** | NULL ⇒ running; non-NULL ⇒ marked killed |
| shell | TEXT | yes | |
| root | TEXT | yes | |
| run_id | INTEGER | yes | the run this process belongs to (see `process_output.run_id`); set by the monitor |
| transient | INTEGER | yes | 1 when started with `--shell` (a transient process) rather than from `.candle.json`; set by the monitor from `MonitorLaunchInfo.transient`. NULL (rows from an older candle) reads as false. Used by `find-orphans` |

### Table `services`
```sql
create table services(
    id integer primary key,
    project_dir text not null,
    command_name text not null,
    unique(project_dir, command_name)
)
```
One row per `(project_dir, command_name)` that has logs, so log rows carry a small integer instead of the project path. A writer creates it with its first log line (`insert_log` in `rust/src/logs/process_logs.rs`); cleanup deletes rows with no logs left (§6). Nothing else refers to `services.id`, so an id isn't stable across that: a service whose logs were all evicted gets a new id with its next line.

### Table `log_lines` (logs)
```sql
create table log_lines(
    id integer primary key autoincrement,
    service_id integer not null,           -- services.id
    run_id integer,                        -- nullable
    log_type integer not null,
    timestamp integer not null default (strftime('%s', 'now')),
    content text                           -- nullable
)
```
`id` is insertion order, and the order every reader uses. `autoincrement` keeps ids from ever being reused, which matters because ids are also run ids and `logs --start-at` cursors.

`run_id` is the id of the row's run: that launch's `process_start_initiated` row. NULL only for rows written before the service's first launch. A service's latest run is its highest `run_id`. Writers that know the run (the monitor, the stale-process cleanup, `start`'s missing-root failure) set it explicitly. A row written with NULL joins the service's latest run (`coalesce(?, latest_run_of(service))`, `latest_run_of` in `rust/src/db/mod.rs`), and the trigger below gives a launch row its own id. See [logs.md](logs.md) §1 "Runs".

Storage: about 40 bytes per short line, row and index entries together. The table before this one (`process_output`, below) repeated `project_dir` and `command_name` in every row and in five indexes: a 200k-line burst took 169 MB plus an 80 MB WAL, against 9 MB plus a 4 MB WAL now.

### View `process_output`
```sql
create view process_output as
  select l.id, s.command_name, s.project_dir, l.content, l.log_type, l.timestamp, l.run_id
  from log_lines l join services s on s.id = l.service_id
```
The columns of the old `process_output` table, which held the logs until `services` / `log_lines` replaced it. Its `instead of insert` trigger (`process_output_insert`) creates the `services` row if needed and inserts into `log_lines`, giving a NULL `run_id` the service's latest run; `instead of delete` (`process_output_delete`) deletes the `log_lines` row. It exists for monitors started by the previous candle, which insert into `process_output` until their service exits and run their own cleanup against it, and for reading the database by hand. Candle's own code uses the tables.

`log_type` enum (`rust/src/logs/log_type.rs`): `stdout=1, stderr=2, process_start_initiated=3, process_start_failed=4, process_started=5, process_exited=6`.

### Table `process_last_cleanup`
```sql
create table process_last_cleanup(
    timestamp integer not null
)
```
Single-row table (logically a singleton). `run_cleanup` writes via an update-then-insert upsert with no where-clause — see §6.

### Table `stdin_messages`
```sql
create table stdin_messages(
    id integer primary key autoincrement,
    command_name text not null,
    project_dir text not null,
    data text not null,
    encoding text not null default 'utf8',
    created_at integer not null default (strftime('%s', 'now'))
)
```

### Indexes
```sql
create index idx_log_lines_service     on log_lines(service_id);
create index idx_log_lines_run         on log_lines(service_id, run_id);
create index idx_stdin_messages_lookup on stdin_messages(project_dir, command_name, id);
```
plus `services`' unique `(project_dir, command_name)` index, which every log query starts from.

Both `log_lines` indexes end in the rowid (`id`). `idx_log_lines_service` is `(service_id, id)`: the id cursors of `watch` / `wait-for-log` / `start` (`id > ?`), newest-first reads of every run, and eviction's `offset`. `idx_log_lines_run` is `(service_id, run_id, id)`: `max(run_id)` per service, and reading one run (`logs`' default) newest-first without a sort. Check a new query against these with `EXPLAIN QUERY PLAN`; reading several services at once sorts, which is expected.

### Trigger `log_lines_launch_run`
```sql
create trigger log_lines_launch_run after insert on log_lines when new.log_type = 3 begin
  update log_lines set run_id = new.id where id = new.id;
end
```
A `process_start_initiated` row starts its own run, so its `run_id` is its own id. Doing it in the trigger means no reader sees the launch row without it. `start_run` returns that id (`last_insert_rowid()`).

## 4. Migration / open behavior

Every connection also sets `PRAGMA journal_size_limit = 4194304` (4 MB), so the WAL file is cut back after a checkpoint resets it instead of staying at its high-water mark.

The schema is applied additively and idempotently on every open (`run_migration` in `rust/src/db/mod.rs`), opening the file first and creating it if absent:
- A fresh DB simply runs all 5 `create table` + 3 `create index` statements, then creates the trigger, the `process_output` view and its two triggers.
- Migration is **idempotent and non-destructive**: safe to run on every startup. `run_migration` runs each `create table if not exists` in `TABLE_STATEMENTS`, then compares every table's columns (`PRAGMA table_info`) against the same DDL applied to an in-memory database. A table missing any column (for example a very old `candle.db` without `log_collector_pid`, `shell`, `root`, `killed_at`, or `process_output.timestamp`) is rebuilt inside `BEGIN IMMEDIATE`: rename the old table, create the current one, copy the shared columns, drop the old one. A rebuild is used instead of `ALTER TABLE ADD COLUMN` because SQLite can't add a column whose default is an expression such as `strftime('%s', 'now')`. Missing columns take their schema default; a `not null` column with no default gets `0` or `''`. Then the `INDEX_STATEMENTS` run, which also recreates indexes dropped with a rebuilt table, and the trigger and view are created. Extra columns, extra tables/indexes and type drift are not checked.
- **`process_output` table → `services` + `log_lines`** (`migrate_process_output`). A database from before `log_lines` has `process_output` as a table (`sqlite_master.type = 'table'`). In one `BEGIN IMMEDIATE` transaction, re-checked after taking the lock since another process may have done it: insert each distinct `(project_dir, command_name)` into `services`; copy every row into `log_lines` keeping its `id`, `run_id`, `log_type`, `timestamp` and `content`; raise `log_lines`' `sqlite_sequence` to the old table's, so no id the old table issued (even of a deleted row) is issued again; drop the table (its indexes and `process_output_assign_run` trigger go with it); create the view and triggers. A table from before `run_id` gets each row's run by position (the latest `process_start_initiated` at or before it), and one from before `timestamp` gets the migration time. 200k rows take under a second. The dropped table's pages stay in the file until cleanup's next `VACUUM` (§6).
- An older candle opening the migrated database fails to create its old indexes on the view. Downgrading isn't supported; its monitors that are already running keep working through the view.

## 5. CRUD functions and exact SQL

### Process table (`rust/src/db/process_table.rs`)
The `ProcessEntry` struct carries fields `id, command_name, project_dir, pid, log_collector_pid: Option, start_time, created_at, killed_at: Option, shell: Option, root: Option, run_id: Option`. Inserts take a `CreateProcessEntry { command_name, project_dir, pid, log_collector_pid, shell, root, run_id }`. Every function takes `conn: &Connection` first. `id` is the PK, but update/delete are keyed by `(command_name, project_dir, pid)`, not the row id.

- `create_process_entry(conn, &CreateProcessEntry)` inserts into `processes`:
  ```
  command_name, project_dir, pid,
  start_time = now (unix seconds),
  log_collector_pid, shell, root, run_id
  ```
  `created_at` and `killed_at` are left to default/NULL. Returns the last insert rowid.
- `update_process_killed_at(conn, command_name, project_dir, pid, killed_at)`:
  ```sql
  update processes set killed_at = ? where command_name = ? and project_dir = ? and pid = ?
  ```
- `delete_process_entry(conn, command_name, project_dir, pid)`:
  ```sql
  delete from processes where command_name = ? and project_dir = ? and pid = ?
  ```
- `find_processes_by_command_name_and_project_dir(conn, command_name, project_dir)`:
  ```sql
  select * from processes where command_name = ? and project_dir = ?
  ```
- `find_processes_by_project_dir(conn, project_dir)`: `select * from processes where project_dir = ?`
- `find_running_processes_by_project_dir(conn, project_dir)`: `... where project_dir = ? and killed_at is null`
- `find_all_processes()`: `select * from processes`
- `find_all_running_processes()`: `... where killed_at is null`
- `find_all_killed_processes()`: `... where killed_at is not null`

### stdin messages (`rust/src/db/stdin_messages.rs`) — FIFO queue per service
- `create_stdin_message(conn, command_name, project_dir, data, encoding: Option<&str>)` → insert into `stdin_messages` (`encoding` defaults to `'utf8'`). Returns the last insert rowid.
- `pop_stdin_message(&mut conn, command_name, project_dir)`:
  ```sql
  select * from stdin_messages where command_name = ? and project_dir = ? order by id asc limit 1
  ```
  then `delete from stdin_messages where id = ?`. Returns `Option<StdinMessage>`.
  The select+delete run in one transaction, so the pop is atomic under concurrency.
- `clear_stdin_messages(conn, command_name, project_dir)`:
  ```sql
  delete from stdin_messages where command_name = ? and project_dir = ?
  ```

### Log write (`rust/src/logs/process_logs.rs`)
- `save_run_log(conn, run_id: Option<i64>, command_name, project_dir, log_type, content: Option<&str>)` calls `insert_log`, which runs one statement per line:
  ```sql
  insert into log_lines(service_id, run_id, log_type, content)
    select s.id, coalesce(?3, <latest_run_of(s.id)>), ?4, ?5 from services s
    where s.project_dir = ?1 and s.command_name = ?2
  ```
  If it inserted nothing (the service's first line), `insert or ignore into services(project_dir, command_name)` and run it again, looping in case cleanup deleted the new service row in between. `timestamp` defaults to `strftime('%s','now')`.
- `save_process_log(conn, command_name, project_dir, log_type, content)` = `save_run_log` with `run_id` NULL.
- `start_run(conn, command_name, project_dir) -> run_id`: `insert_log` of a `process_start_initiated` row, returning its id (which `log_lines_launch_run` made its own `run_id`).

### Log read (`rust/src/logs/process_logs.rs`: `build_log_search_query` + `get_process_logs_with_eviction_info`)
Builds dynamic SQL on `services s join log_lines l on l.service_id = s.id`, selecting the `ProcessLog` columns:
- WHERE scope (`scope_clause`) by `s.project_dir` and/or `s.command_name in (...)` (a single command is `in (?)`).
- Optional `and l.timestamp > ?` (`since_timestamp`), `and l.id > ?` (`after_log_id`), `and l.id >= ?` (`min_log_id`), `and l.log_type in (...)` (`log_types`), `and l.run_id = ?` (`run_id`), `and l.run_id is <latest_run_of(s.id)>` (`latest_launch_only`), and the run below that (`previous_launch_only`).
- Always `order by l.id desc`; optional `limit ?`.
- If neither `project_dir` nor command names is given, the scope is `1 = 0` (no rows).
- Eviction detection (`get_process_logs_with_eviction_info`): if a `limit` is set and returned rows `>= limit`, re-runs the same query without limit wrapped in `select count(*) as total from (<sql>)`; if total > returned ⇒ `logs_were_evicted = true`.
- Final list is reversed → returned in chronological (ascending) order.
- `get_log_tail` (used by `logs --count`) builds on these; `latest_run_ids` returns each command's `max(run_id)`. See [logs.md](logs.md) §3-6.

## 6. Cleanup / eviction algorithm (`rust/src/db/cleanup.rs`)

`CLEANUP_INTERVAL_SECONDS = 10 * 60` (600s).

`maybe_run_cleanup(conn)` is called by most CLI command handlers in `main.rs` after opening the DB, and by each monitor every 60s (`CLEANUP_INTERVAL_MS` in `monitor/run.rs`):
1. `now` = current unix seconds.
2. `last_cleanup = select timestamp from process_last_cleanup` (first row, if any).
3. If `last_cleanup` exists **and** `now - last_cleanup < 600` → return (skip).
4. `run_cleanup(conn)`.

`run_cleanup(conn)` resolves the eviction config **per `project_dir`**: `find_config_file(project_dir)` → `get_log_eviction_config(...)`, defaults on any error, cached for the pass, so limits stay correct when the database holds logs from several projects. In exact order:
1. `now` = current unix seconds.
2. **Time-based eviction**, for each `select distinct project_dir from services`: `cutoff = now - maxRetentionSeconds`; then
   ```sql
   delete from log_lines where service_id in (select id from services where project_dir = ?) and timestamp < ?    -- [project_dir, cutoff]
   ```
3. **Stale process cleanup**: `cleanup_stale_processes()` (§7).
4. **Per-service eviction**: count every service:
   ```sql
   select s.id, s.project_dir, (select count(*) from log_lines l where l.service_id = s.id) as log_count
   from services s
   ```
   and skip those with `log_count <= maxLogsPerService` for their project (filtered in Rust, since the threshold is per project). For each over-limit service, find the cutoff id (the id of the row at offset = maxLogsPerService, newest first) and delete everything at or below it, keeping the newest `maxLogsPerService` rows:
   ```sql
   select id from log_lines where service_id = ? order by id desc limit 1 offset ?   -- [service_id, maxLogsPerService]
   delete from log_lines where service_id = ? and id <= ?
   ```
5. **Forget empty services**: `delete from services where not exists (select 1 from log_lines l where l.service_id = services.id)`. A writer that loses the race recreates the row (§5 "Log write").
6. `reclaim_space` (`rust/src/db/mod.rs`): `VACUUM`, then `PRAGMA wal_checkpoint(TRUNCATE)`, since `VACUUM` writes a copy of the whole database into the WAL. The truncate is best-effort: a reader holding an old snapshot (a running `watch`) keeps the WAL in use, and `journal_size_limit` trims it later. `clear-logs` ends the same way.
7. Upsert into `process_last_cleanup` with `{ timestamp: now }`.
   - Upsert semantics: first `UPDATE process_last_cleanup SET timestamp = ?` with **no WHERE clause** (updates all rows); if it changed 0 rows (i.e. table empty) → `INSERT INTO process_last_cleanup (timestamp) VALUES (?)`. Net effect: keeps a single row updated in place; inserts the first row if empty.

Eviction config (`rust/src/config/`):
- `LOG_EVICTION_DEFAULTS` (`config/model.rs`) = `{ max_logs_per_service: 1000, max_retention_seconds: 86400 }` (24h).
- Read from config file under `config.logEviction.{maxLogsPerService,maxRetentionSeconds}` (`get_log_eviction_config`). Validation: each, if present, must be an integer `>= 1`, else `validate_config` fails with `Config file error: 'logEviction.<field>' must be a positive integer`.

## 7. Stale process cleanup (`rust/src/db/cleanup.rs`)

`cleanup_stale_processes()`:
1. `find_all_running_processes()` (killed_at IS NULL). For each `proc`:
   - If `proc.log_collector_pid` is set **and** `is_process_alive(log_collector_pid)` → skip (the monitor is managing it).
   - Else if `is_process_alive(proc.pid)` → skip (service still alive).
   - Else (both dead) → it's stale:
     - `save_run_log(conn, proc.run_id, command_name, project_dir, ProcessExited (6), Some("Process cleaned up (stale entry after restart or crash)"))`: the exit is stamped with the process row's run, so it never lands in a newer run.
     - `delete_process_entry(conn, command_name, project_dir, pid)`.
2. `find_all_killed_processes()` (killed_at IS NOT NULL). For each → `delete_process_entry(...)` unconditionally (the monitor died before deleting; clean them up).

`is_process_alive(pid)` (`rust/src/process_alive.rs`) sends signal 0 with `libc::kill(pid, 0)`: an existence check, no signal delivered.
- Process exists, we own it → no error → alive.
- `EPERM` → process exists but owned by another user → **treated as alive**.
- `ESRCH` (no such process) → dead.
- Any other error, or `pid <= 0` (dead without calling `kill`) → dead.

Only Unix `kill` semantics are implemented; there is no Windows (`OpenProcess`/`GetExitCodeProcess`) path.

`filter_alive_processes(conn, entries)` (`process_alive.rs`) is a related helper used by callers (not by cleanup): keeps entries whose monitor or pid is alive, deletes the rest from the DB. Same alive logic. `erase-database`'s `live_processes_in` (§10) applies the same test read-only.

## 8. Dependencies

The only crate in this subsystem is `rusqlite` (feature `bundled`, so SQLite is compiled in): synchronous prepared statements with `?` positional params, plus the migration runner of §4. No ORM; everything is hand-written SQL strings. The `process_last_cleanup` upsert is update-then-insert-if-zero-changes (no `ON CONFLICT` needed, since the table has no unique key).

## 9. Subtle / platform-specific gotchas

- **Timestamps are epoch seconds**, mixing two sources: SQLite `strftime('%s','now')` (defaults) and app code `SystemTime::now()` as unix seconds (start_time, cutoffs). Seconds, not millis. Both must agree on UTC seconds.
- **WAL + busy_timeout(30s)** are set per connection. Concurrent writers rely on this. VACUUM during cleanup briefly takes an exclusive lock.
- **Connection model**: a fresh connection per `get_database` call (no singleton), each establishing WAL + busy_timeout; `override_dir` is honored on every call.
- **Process rows are keyed on `(command_name, project_dir, pid)`** for update/delete, not on the `id` PK.
- **`pop_stdin_message` is atomic** — the select+delete run in a transaction.
- **Per-service eviction deletes by `id <=`** the offset-selected cutoff; the two-query approach keeps the newest N.
- **Upsert on `process_last_cleanup`** uses an unconditional UPDATE (no WHERE) — works only because the table is logically single-row. If two rows ever exist, both get the same timestamp; the `select timestamp from process_last_cleanup` (no LIMIT) returns the first row.
- **Stale cleanup writes a `process_exited` log line** with the exact string `Process cleaned up (stale entry after restart or crash)`; tests match it verbatim.
- **State dir creation** is recursive; the parent `~/.local/state` may not exist.

## 10. Erasing the database (`rust/src/commands/erase_database.rs`)

`candle erase-database [--force]` → `handle_erase_database_command(force)` → `erase_database_guarded(get_state_directory(), force)`, which returns `EraseOutcome::{Erased, RefusedLiveProcesses(Vec<ProcessEntry>)}`.
- Without `--force`, it first calls `live_processes_in(state_dir)`: if `candle.db` exists, it opens it and returns the `find_all_running_processes` rows (`killed_at is null`) whose `pid` **or** `log_collector_pid` is alive. It deletes nothing (unlike `filter_alive_processes`). Erasing under live processes would orphan them, so a non-empty result refuses: nothing is touched and `format_refusal` produces the stderr text (`Error: Refusing to erase the database: N Candle-managed process is/processes are still running.`, then `Erasing now would leave them running with no way for Candle to stop them.`, one `  <name> (pid <pid>) in <project_dir>` line each, then `Run 'candle kill-all' first, or pass --force to erase anyway.`), and `cmd_erase_database` exits 1.
- If the DB can't be read (corruption is a main reason to erase it), it prints to stderr `Warning: could not read the database to check for running processes (<e>); erasing anyway.` and proceeds.
- `erase_database_in(state_dir)` then removes `candle.db`, `candle.db-wal`, and `candle.db-shm`, treating `NotFound` (including a file vanishing mid-removal) as already gone. Output: `Clearing database at: <path>`, `Removed database file` or `Database file not found`, `Removed WAL file` / `Removed shared memory file` only when present, then `Database erased. A new one will be created on next use.` An unexpected I/O error surfaces as `Error: Could not erase database: <e>` on stderr (exit 1).
