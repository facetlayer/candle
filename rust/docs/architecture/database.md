# Database subsystem

## 1. Overview

The subsystem is a single SQLite database that stores: (a) the registry of launched background processes, (b) captured process output (logs), (c) a stdin message queue per service, and (d) a last-cleanup timestamp. It is accessed concurrently by multiple OS processes (the CLI, the per-service "log collector" subprocess, the MCP server), so WAL + busy-timeout are mandatory.

The Rust implementation lives under `rust/candle-core/src/db/` (`mod.rs`, `process_table.rs`, `stdin_messages.rs`, `cleanup.rs`), with directory resolution in `rust/candle-core/src/dirs.rs` and liveness checks in `rust/candle-core/src/process_alive.rs`. It uses `rusqlite` (bundled SQLite) with hand-written SQL. It mirrors the original Node implementation built on `@facetlayer/sqlite-wrapper` (v1.3.0), which itself wrapped Node's built-in `node:sqlite` (`DatabaseSync`). Source-of-truth cross-references below point at the original `src/...` TypeScript files; all paths are repo-relative.

## 2. Database file location, naming, creation

`get_state_directory()` in `rust/candle-core/src/dirs.rs` (mirrors `src/dirs.ts`) resolves the **state dir** with this precedence order:

1. `CANDLE_DATABASE_DIR` env var → used verbatim.
2. `XDG_STATE_HOME` env var → `join($XDG_STATE_HOME, "candle")`.
3. Default → `join(home, ".local", "state", "candle")` i.e. `~/.local/state/candle`.

DB bootstrap lives in `rust/candle-core/src/db/mod.rs` (mirrors `src/database/database.ts:56-84`):
- `stateDir = overrideDirectory ?? get_state_directory()`.
- If `stateDir` does not exist, it is created recursively (`fs.mkdirSync(stateDir, { recursive: true })` in Node).
- DB file = `join(stateDir, "candle.db")` — **filename is exactly `candle.db`**.
- After opening, every connection runs:
  ```sql
  PRAGMA journal_mode=WAL;      -- database.ts:80
  PRAGMA busy_timeout=30000;    -- database.ts:81  (30 seconds)
  ```
- The original Node code keeps a module-level singleton connection (`let _db`); the first call wins and `overrideDirectory` on later calls is ignored once the singleton is set. The Rust implementation does **not** keep a process-wide singleton — it opens a fresh connection on each call and sets WAL + busy_timeout on every connection. This is simpler and correct for the multi-process usage candle relies on, since each connection independently establishes the required pragmas.

Subtle / easy to get wrong:
- WAL mode creates sidecar files `candle.db-wal` and `candle.db-shm` next to `candle.db`. Litestream-style names (`_litestream*`) are explicitly tolerated by the migration drift checker.
- `busy_timeout=30000` (ms) is essential — multiple processes write concurrently; without it you get `SQLITE_BUSY`.
- `PRAGMA journal_mode=WAL` returns a row (`"wal"`); harmless.

## 3. Exact schema

Defined in `rust/candle-core/src/db/mod.rs`, byte-parity with `src/database/database.ts:13-52` (`schema.name = 'CandleDatabase'`). All `integer` timestamps are **Unix epoch seconds**. Default timestamp expression is `strftime('%s','now')` (returns a string in SQLite but stored in an integer column → stored as integer text/affinity integer; epoch seconds).

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
    root text                              -- nullable
)
```
| column | type | nullable | notes |
|---|---|---|---|
| id | INTEGER PK AUTOINCREMENT | no | |
| command_name | TEXT | no | service name |
| project_dir | TEXT | no | absolute project dir |
| pid | INTEGER | no | OS pid of the service process |
| log_collector_pid | INTEGER | **yes** | pid of the supervising log-collector process |
| start_time | INTEGER | no | epoch seconds, set by app code (`Math.floor(Date.now()/1000)`) |
| created_at | INTEGER | no | default `strftime('%s','now')` |
| killed_at | INTEGER | **yes** | NULL ⇒ running; non-NULL ⇒ marked killed |
| shell | TEXT | yes | |
| root | TEXT | yes | |

### Table `process_output` (logs)
```sql
create table process_output(
    id integer primary key autoincrement,
    command_name text not null,
    project_dir text not null,
    content text,                          -- nullable
    log_type integer not null,
    timestamp integer not null default (strftime('%s', 'now'))
)
```
`log_type` enum (`rust/candle-core/src/logs/log_type.rs`, mirrors `src/logs/ProcessLogType.ts`): `stdout=1, stderr=2, process_start_initiated=3, process_start_failed=4, process_started=5, process_exited=6`.

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
create index idx_process_output_command_name on process_output(command_name);
create index idx_process_output_project_dir  on process_output(project_dir);
create index idx_process_output_lookup       on process_output(project_dir, command_name, timestamp desc, id desc);
create index idx_stdin_messages_lookup        on stdin_messages(project_dir, command_name, id);
```

The `idx_process_output_lookup` ordering `(project_dir, command_name, timestamp desc, id desc)` matches the eviction and log-fetch sort order, and is kept exactly.

## 4. Migration / open behavior

The schema is applied additively and idempotently on every startup, mirroring the original `DatabaseLoader.load()` with `migrationBehavior: 'safe-upgrades'`. The original Node behavior (see node_modules `migration.js` / `DatabaseLoader.js`) was, in order:

1. Open the file (`new DatabaseSync(filename)`), creating it if absent.
2. `migrateToSchema(schema, { includeDestructive: false })`: for each statement in `schema.statements`:
   - **create table**: if table absent → run the full `create table`. If present → diff columns; only **additive, nullable** column adds are applied (`alter table ... add column`); NOT-NULL adds and any destructive drift (drop/modify column, rebuild, drop index) are **skipped with a warning**, never applied.
   - **create index**: if index absent → create it; if present → left as-is.
3. `runDatabaseSloppynessCheck` — logs warnings about extra tables/indexes but makes no changes.

The Rust implementation reproduces the safe, non-destructive outcome:
- A fresh DB simply runs all 4 `create table` + 4 `create index` statements.
- Migration is **idempotent and non-destructive**: safe to run on every startup. It uses `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS` equivalents, plus additive `ALTER TABLE ADD COLUMN` for forward-compat with older DBs missing newer nullable columns (`log_collector_pid`, `shell`, `root`, `killed_at` were added over time — they are handled as additive nullable columns). Full drift-diffing is not needed for parity.
- In the Node original, logging callbacks routed `info → console.log`, `warn → console.warn`, `error → console.error(err.errorMessage)`.

## 5. CRUD functions and exact SQL

### Process table (`rust/candle-core/src/db/process_table.rs`, mirrors `src/database/processTable.ts`)
The `ProcessEntry` struct carries fields `id, command_name, project_dir, pid, log_collector_pid, start_time, created_at, killed_at?, shell, root?`.
⚠️ **Historical gotcha (resolved in Rust):** the original TS `ProcessEntry` interface declared `launch_id`, but the table column is `id`. `select *` returns `id`, not `launch_id`, so `entry.launch_id` was effectively always `undefined` in TS. Code paths that delete/update use `command_name + project_dir + pid` as the key, not the row id. The Rust struct exposes `id` as the actual PK, and process rows are keyed by `(command_name, project_dir, pid)` for update/delete.

- `create_process_entry(entry)` inserts into `processes`:
  ```
  command_name, project_dir, pid,
  start_time = Math.floor(Date.now()/1000),
  log_collector_pid, shell, root (root ?? null)
  ```
  `created_at` and `killed_at` are left to default/NULL. Returns the last insert rowid.
- `update_process_killed_at({commandName, projectDir, pid, killedAt})`:
  ```sql
  update processes set killed_at = ? where command_name = ? and project_dir = ? and pid = ?
  ```
- `delete_process_entry({commandName, projectDir, pid})`:
  ```sql
  delete from processes where command_name = ? and project_dir = ? and pid = ?
  ```
- `find_processes_by_command_name_and_project_dir(commandName, projectDir)`:
  ```sql
  select * from processes where command_name = ? and project_dir = ?
  ```
- `find_processes_by_project_dir(projectDir)`: `select * from processes where project_dir = ?`
- `find_running_processes_by_project_dir(projectDir)`: `... where project_dir = ? and killed_at is null`
- `find_all_processes()`: `select * from processes`
- `find_all_running_processes()`: `... where killed_at is null`
- `find_all_killed_processes()`: `... where killed_at is not null`

### stdin messages (`rust/candle-core/src/db/stdin_messages.rs`, mirrors `src/database/stdinMessagesTable.ts`) — FIFO queue per service
- `create_stdin_message({commandName, projectDir, data, encoding?})` → insert into `stdin_messages` (`encoding ?? 'utf8'`). Returns the last insert rowid.
- `pop_stdin_message(commandName, projectDir)`:
  ```sql
  select * from stdin_messages where command_name = ? and project_dir = ? order by id asc limit 1
  ```
  then `delete from stdin_messages where id = ?`. Returns the row or `null`.
  The original TS performed the SELECT and DELETE separately and was **not transactional**. The Rust implementation wraps the select+delete in a transaction so the pop is atomic under concurrency.
- `clear_stdin_messages(commandName, projectDir)`:
  ```sql
  delete from stdin_messages where command_name = ? and project_dir = ?
  ```

### Log write (`rust/candle-core/src/logs/process_logs.rs`, mirrors `src/logs/processLogs.ts`)
- `save_process_log({command_name, project_dir, content?, log_type})`:
  ```sql
  insert into process_output(command_name, project_dir, content, log_type) values(?, ?, ?, ?)
  ```
  `timestamp` defaults to `strftime('%s','now')`.

### Log read (`rust/candle-core/src/logs/process_logs.rs`, mirrors `buildLogSearchQuery.ts` + `getProcessLogsWithEvictionInfo`)
Builds dynamic SQL on `process_output po`:
- WHERE clauses by `project_dir` and/or `command_name IN (...)` (single command uses `= ?`).
- Optional `and po.timestamp > ?` (sinceTimestamp), `and po.id > ?` (afterLogId).
- Always `order by po.timestamp desc, po.id desc`; optional `limit ?`.
- Throws `Error('Must provide projectDir or commandNames')` if neither given.
- Eviction detection: if returned rows `>= limit`, re-runs the same query without limit wrapped in `select count(*) as total from (<sql>)`; if total > returned ⇒ `logsWereEvicted = true`.
- Final list is reversed → returned in chronological (ascending) order.

## 6. Cleanup / eviction algorithm (`rust/candle-core/src/db/cleanup.rs`, mirrors `src/database/cleanup.ts`)

`CLEANUP_INTERVAL_SECONDS = 10 * 60` (600s).

`maybe_run_cleanup()`:
1. `now = floor(Date.now()/1000)`.
2. `lastCleanup = db.get('select timestamp from process_last_cleanup')`.
3. If `lastCleanup` exists **and** `now - lastCleanup.timestamp < 600` → return (skip).
4. Load eviction config: `findConfigFile(process.cwd())` → `getLogEvictionConfig(config)`. On any throw → defaults.
5. `run_cleanup(evictionConfig)`.

`run_cleanup(evictionConfig)`, in exact order:
1. `now = floor(Date.now()/1000)`.
2. **Time-based eviction**: `logCutoff = now - maxRetentionSeconds`; then
   ```sql
   delete from process_output where timestamp < ?    -- [logCutoff]
   ```
3. **Stale process cleanup**: `cleanup_stale_processes()` (§7).
4. **Per-service eviction**: find over-limit services:
   ```sql
   select project_dir, command_name, count(*) as log_count
   from process_output
   group by project_dir, command_name
   having count(*) > ?            -- [maxLogsPerService]
   ```
   For each such service, find the cutoff id (the id of the row at offset = maxLogsPerService when sorted newest-first):
   ```sql
   select id from process_output
   where project_dir = ? and command_name = ?
   order by timestamp desc, id desc
   limit 1 offset ?              -- [project_dir, command_name, maxLogsPerService]
   ```
   If found, delete everything at or below that id for the service:
   ```sql
   delete from process_output
   where project_dir = ? and command_name = ? and id <= ?
   ```
   ⚠️ Deletes by `id <= cutoff_id`, which is correct only if id order matches the `timestamp desc, id desc` order. Since id is autoincrement and timestamps are coarse (1-second), the cutoff is chosen by the sorted query but the delete is by id. This keeps the newest `maxLogsPerService` rows. The offset+id approach is replicated exactly.
5. `vacuum` — runs `VACUUM` on the whole DB.
6. Upsert into `process_last_cleanup` with `{ timestamp: now }`.
   - Upsert semantics (mirroring the sqlite-wrapper `upsert.js`): first `UPDATE process_last_cleanup SET timestamp = ?` with **no WHERE clause** (updates all rows); if `changes === 0` (i.e. table empty) → `INSERT INTO process_last_cleanup (timestamp) VALUES (?)`. Net effect: keeps a single row updated in place; inserts the first row if empty.

Eviction config (mirrors `src/configFile.ts`):
- `LOG_EVICTION_DEFAULTS = { maxLogsPerService: 1000, maxRetentionSeconds: 86400 }` (24h).
- Read from config file under `config.logEviction.{maxLogsPerService,maxRetentionSeconds}`. Validation: each, if present, must be an integer `>= 1`, else throws `Config file error: 'logEviction.<field>' must be a positive integer`.

## 7. Stale process cleanup (`rust/candle-core/src/db/cleanup.rs`, mirrors `staleProcessCleanup.ts`)

`cleanup_stale_processes()`:
1. `find_all_running_processes()` (killed_at IS NULL). For each `proc`:
   - If `proc.log_collector_pid` truthy **and** `is_process_alive(log_collector_pid)` → skip (collector is managing it).
   - Else if `is_process_alive(proc.pid)` → skip (service still alive).
   - Else (both dead) → it's stale:
     - `save_process_log({command_name, project_dir, log_type: process_exited(6), content: 'Process cleaned up (stale entry after restart or crash)'})`.
     - `delete_process_entry({commandName, projectDir, pid})`.
2. `find_all_killed_processes()` (killed_at IS NOT NULL). For each → `delete_process_entry(...)` unconditionally (collector died before deleting; clean them up).

`is_process_alive(pid)` (`rust/candle-core/src/process_alive.rs`, mirrors `src/process-alive.ts:7`). The original TS was:
```js
try { process.kill(pid, 0); return true; }
catch (err) { if (err.code === 'EPERM') return true; return false; }
```
Semantics: signal 0 = existence check, no signal delivered.
- Process exists, we own it → no error → alive.
- `EPERM` → process exists but owned by another user → **treated as alive**.
- `ESRCH` (no such process) → dead.

The Rust implementation uses `libc::kill(pid, 0)` directly: `Ok` → alive, `EPERM` → alive, `ESRCH` / anything else → dead. Candle targets macOS/Linux, so this Unix `kill` semantics is the only path; Windows (`OpenProcess`/`GetExitCodeProcess`) is out of scope.

`filter_alive_processes(entries)` (`process_alive.rs`, mirrors `process-alive.ts:26`) is a related helper used by callers (not by cleanup): keeps entries whose collector or pid is alive, deletes the rest from the DB. Same alive logic.

## 8. Dependency mapping (npm → Rust crate)

| npm dep (original) | role | Rust crate |
|---|---|---|
| `@facetlayer/sqlite-wrapper` (1.3.0) | thin wrapper over `node:sqlite`: `run/get/list/insert/update/upsert/count/exists`, schema loader + additive migrations, drift detection | `rusqlite` (bundled SQLite) — the same helper methods + an additive migration runner. `upsert` = update-then-insert-if-zero-changes (no `ON CONFLICT` needed since `process_last_cleanup` has no unique key). |
| `node:sqlite` (`DatabaseSync`) | underlying SQLite engine, synchronous | `rusqlite` (feature `bundled`) |
| `@facetlayer/parse-stdout-lines`, `@facetlayer/subprocess`, `@modelcontextprotocol/sdk`, `yargs` | not part of this subsystem (process spawning / MCP / CLI) | n/a for the database module |

No ORM; everything is hand-written SQL strings. The original wrapper's `insert`/`update` builders validated identifiers with `/^[a-zA-Z_][a-zA-Z0-9_]*$/` and built `INSERT INTO t (cols) VALUES (?...)` / `UPDATE t SET c=? WHERE c=?`. The Rust code uses `rusqlite` prepared statements directly.

## 9. Subtle / platform-specific gotchas

- **Timestamps are epoch seconds**, mixing two sources: SQLite `strftime('%s','now')` (defaults) and app code `Math.floor(Date.now()/1000)` (start_time, cutoffs). Seconds, not millis. Both must agree on UTC seconds.
- **WAL + busy_timeout(30s)** are set per connection. Concurrent writers rely on this. VACUUM during cleanup briefly takes an exclusive lock.
- **Connection model**: the original TS honored `overrideDirectory` only on the first `getDatabase` call (module-level singleton); tests relied on a fresh process/module per workspace. The Rust implementation opens a fresh connection per call, each establishing WAL + busy_timeout.
- **`launch_id` vs `id` mismatch** in the original `ProcessEntry` (see §5) — the real PK column is `id`; lookups/mutations are keyed on `(command_name, project_dir, pid)`. Rust exposes `id`.
- **`pop_stdin_message` is atomic** in Rust — the select+delete run in a transaction (the original TS was not transactional).
- **Per-service eviction deletes by `id <=`** the offset-selected cutoff; the exact two-query approach keeps the newest N.
- **Upsert on `process_last_cleanup`** uses an unconditional UPDATE (no WHERE) — works only because the table is logically single-row. If two rows ever exist, both get the same timestamp; the `select timestamp from process_last_cleanup` (no LIMIT) returns the first row.
- **Stale cleanup writes a `process_exited` log line** with the exact string `'Process cleaned up (stale entry after restart or crash)'` — preserved verbatim for test parity.
- **State dir creation** is recursive; the parent `~/.local/state` may not exist.
