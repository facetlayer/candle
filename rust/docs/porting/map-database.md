# Candle "database" subsystem — technical spec for Rust reimplementation

## 1. Overview

The subsystem is a single SQLite database that stores: (a) the registry of launched background processes, (b) captured process output (logs), (c) a stdin message queue per service, and (d) a last-cleanup timestamp. It is accessed concurrently by multiple OS processes (the CLI, the per-service "log collector" subprocess, the MCP server), so WAL + busy-timeout are mandatory.

In the TS code it is built on top of `@facetlayer/sqlite-wrapper` (v1.3.0), which itself wraps Node's built-in `node:sqlite` (`DatabaseSync`). All file paths below are absolute under `/Users/andy/candle`.

---

## 2. Database file location, naming, creation

`src/dirs.ts` — `getStateDirectory()` resolves the **state dir** (precedence order):

1. `CANDLE_DATABASE_DIR` env var → used verbatim.
2. `XDG_STATE_HOME` env var → `join($XDG_STATE_HOME, "candle")`.
3. Default → `join(os.homedir(), ".local", "state", "candle")` i.e. `~/.local/state/candle`.

`src/database/database.ts:56-84` — `getDatabase({ overrideDirectory? })`:
- `stateDir = overrideDirectory ?? getStateDirectory()`.
- If `stateDir` does not exist, `fs.mkdirSync(stateDir, { recursive: true })`.
- DB file = `join(stateDir, "candle.db")` — **filename is exactly `candle.db`**.
- Opens via `DatabaseLoader` with `migrationBehavior: 'safe-upgrades'`, then runs:
  ```js
  _db.run('PRAGMA journal_mode=WAL');      // database.ts:80
  _db.run('PRAGMA busy_timeout=30000');    // database.ts:81  (30 seconds)
  ```
- `_db` is a module-level singleton (`let _db`). First call wins; `overrideDirectory` on later calls is ignored once the singleton is set. **In Rust, replicate single-open-per-process semantics (e.g. a `OnceCell`/lazy connection pool), but note WAL + busy_timeout must be set on every connection if you use a pool.**

Subtle / easy to get wrong:
- WAL mode creates sidecar files `candle.db-wal` and `candle.db-shm` next to `candle.db`. Litestream-style names (`_litestream*`) are explicitly tolerated by the migration drift checker.
- `busy_timeout=30000` (ms) is essential — multiple processes write concurrently; without it you get `SQLITE_BUSY`.
- `PRAGMA journal_mode=WAL` returns a row (`"wal"`); harmless.

---

## 3. Exact schema

Defined in `src/database/database.ts:13-52` (`schema.name = 'CandleDatabase'`). All `integer` timestamps are **Unix epoch seconds**. Default timestamp expression is `strftime('%s','now')` (returns a string in SQLite but stored in an integer column → stored as integer text/affinity integer; epoch seconds).

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
`log_type` enum (`src/logs/ProcessLogType.ts`): `stdout=1, stderr=2, process_start_initiated=3, process_start_failed=4, process_started=5, process_exited=6`.

### Table `process_last_cleanup`
```sql
create table process_last_cleanup(
    timestamp integer not null
)
```
Single-row table (logically a singleton). `runCleanup` writes via `upsert` with empty where-clause — see §6.

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

The `idx_process_output_lookup` ordering `(project_dir, command_name, timestamp desc, id desc)` matches the eviction and log-fetch sort order; keep it exactly.

---

## 4. Migration / open behavior (`safe-upgrades`)

The TS `DatabaseLoader.load()` with `migrationBehavior: 'safe-upgrades'` does, in order (see node_modules `migration.js` / `DatabaseLoader.js`):

1. Open the file (`new DatabaseSync(filename)`), creating it if absent.
2. `migrateToSchema(schema, { includeDestructive: false })`: for each statement in `schema.statements`:
   - **create table**: if table absent → run the full `create table`. If present → diff columns; only **additive, nullable** column adds are applied (`alter table ... add column`); NOT-NULL adds and any destructive drift (drop/modify column, rebuild, drop index) are **skipped with a warning**, never applied.
   - **create index**: if index absent → create it; if present → left as-is.
3. `runDatabaseSloppynessCheck` — logs warnings about extra tables/indexes but makes no changes.

Implications for Rust:
- A fresh DB simply runs all 4 `create table` + 4 `create index` statements.
- Migration is **idempotent and non-destructive**: safe to run on every startup. A minimal Rust impl can run `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS` equivalents, plus additive `ALTER TABLE ADD COLUMN` for forward-compat. Full drift-diffing is not required for parity unless you must upgrade older DBs that are missing the newer nullable columns (`log_collector_pid`, `shell`, `root`, `killed_at` were likely added over time — handle them as additive nullable columns).
- Logging callbacks: `info → console.log`, `warn → console.warn`, `error → console.error(err.errorMessage)`.

---

## 5. CRUD functions and exact SQL

### `processTable.ts`
`ProcessEntry` interface fields: `launch_id, command_name, project_dir, pid, log_collector_pid, start_time, created_at, killed_at?, shell, root?`.
⚠️ **Subtle bug to preserve/decide on:** the interface declares `launch_id`, but the table column is `id`. `select *` returns `id`, not `launch_id`, so `entry.launch_id` is effectively always `undefined` in TS. Code paths that delete/update use `command_name + project_dir + pid` as the key, not the row id. In Rust, key process rows by `(command_name, project_dir, pid)` for update/delete; expose `id` as the actual PK.

- `createProcessEntry(entry)` → `processTable.ts:38`. Inserts into `processes`:
  ```
  command_name, project_dir, pid,
  start_time = Math.floor(Date.now()/1000),
  log_collector_pid, shell, root (root ?? null)
  ```
  `created_at` and `killed_at` left to default/NULL. Returns `lastInsertRowid`.
- `updateProcessKilledAt({commandName, projectDir, pid, killedAt})`:
  ```sql
  update processes set killed_at = ? where command_name = ? and project_dir = ? and pid = ?
  ```
- `deleteProcessEntry({commandName, projectDir, pid})`:
  ```sql
  delete from processes where command_name = ? and project_dir = ? and pid = ?
  ```
- `findProcessesByCommandNameAndProjectDir(commandName, projectDir)`:
  ```sql
  select * from processes where command_name = ? and project_dir = ?
  ```
- `findProcessesByProjectDir(projectDir)`: `select * from processes where project_dir = ?`
- `findRunningProcessesByProjectDir(projectDir)`: `... where project_dir = ? and killed_at is null`
- `findAllProcesses()`: `select * from processes`
- `findAllRunningProcesses()`: `... where killed_at is null`
- `findAllKilledProcesses()`: `... where killed_at is not null`

### `stdinMessagesTable.ts` (FIFO queue per service)
- `createStdinMessage({commandName, projectDir, data, encoding?})` → insert into `stdin_messages` (`encoding ?? 'utf8'`). Returns `lastInsertRowid`.
- `popStdinMessage(commandName, projectDir)`:
  ```sql
  select * from stdin_messages where command_name = ? and project_dir = ? order by id asc limit 1
  ```
  then `delete from stdin_messages where id = ?`. Returns the row or `null`.
  ⚠️ Not transactional in TS (separate SELECT then DELETE). For correctness under concurrency, Rust should wrap the select+delete in a transaction (or use `DELETE ... RETURNING` on the oldest id). Document this as an intentional improvement.
- `clearStdinMessages(commandName, projectDir)`:
  ```sql
  delete from stdin_messages where command_name = ? and project_dir = ?
  ```

### Log write (`src/logs/processLogs.ts`)
- `saveProcessLog({command_name, project_dir, content?, log_type})`:
  ```sql
  insert into process_output(command_name, project_dir, content, log_type) values(?, ?, ?, ?)
  ```
  `timestamp` defaults to `strftime('%s','now')`.

### Log read (`buildLogSearchQuery.ts` + `getProcessLogsWithEvictionInfo`)
Builds dynamic SQL on `process_output po`:
- WHERE clauses by `project_dir` and/or `command_name IN (...)` (single command uses `= ?`).
- Optional `and po.timestamp > ?` (sinceTimestamp), `and po.id > ?` (afterLogId).
- Always `order by po.timestamp desc, po.id desc`; optional `limit ?`.
- Throws `Error('Must provide projectDir or commandNames')` if neither given.
- Eviction detection: if returned rows `>= limit`, re-runs the same query without limit wrapped in `select count(*) as total from (<sql>)`; if total > returned ⇒ `logsWereEvicted = true`.
- Final list is `reverse()`d → returned in chronological (ascending) order.

---

## 6. Cleanup / eviction algorithm (`cleanup.ts`)

`CLEANUP_INTERVAL_SECONDS = 10 * 60` (600s) — `cleanup.ts:5`.

`maybeRunCleanup()` (`cleanup.ts:7`):
1. `now = floor(Date.now()/1000)`.
2. `lastCleanup = db.get('select timestamp from process_last_cleanup')`.
3. If `lastCleanup` exists **and** `now - lastCleanup.timestamp < 600` → return (skip).
4. Load eviction config: `findConfigFile(process.cwd())` → `getLogEvictionConfig(config)`. On any throw → defaults.
5. `runCleanup(evictionConfig)`.

`runCleanup(evictionConfig)` (`cleanup.ts:29`), in exact order:
1. `now = floor(Date.now()/1000)`.
2. **Time-based eviction**: `logCutoff = now - maxRetentionSeconds`; then
   ```sql
   delete from process_output where timestamp < ?    -- [logCutoff]
   ```
3. **Stale process cleanup**: `cleanupStaleProcesses()` (§7).
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
   ⚠️ Deletes by `id <= cutoff_id`, which is correct only if id order matches the `timestamp desc, id desc` order. Since id is autoincrement and timestamps are coarse (1-second), the cutoff is chosen by the sorted query but the delete is by id. This keeps the newest `maxLogsPerService` rows. Replicate the offset+id approach exactly.
5. `db.run('vacuum')` — runs `VACUUM` on the whole DB.
6. `db.upsert('process_last_cleanup', {}, { timestamp: now })`.
   - `upsert` semantics (from sqlite-wrapper `upsert.js`): first `UPDATE process_last_cleanup SET timestamp = ?` with **no WHERE clause** (updates all rows); if `changes === 0` (i.e. table empty) → `INSERT INTO process_last_cleanup (timestamp) VALUES (?)`. Net effect: keeps a single row updated in place; inserts the first row if empty. In Rust: `UPDATE ...; if rows_affected==0 { INSERT ... }`.

Eviction config (`src/configFile.ts`):
- `LOG_EVICTION_DEFAULTS = { maxLogsPerService: 1000, maxRetentionSeconds: 86400 }` (24h).
- Read from config file under `config.logEviction.{maxLogsPerService,maxRetentionSeconds}`. Validation: each, if present, must be an integer `>= 1`, else throws `Config file error: 'logEviction.<field>' must be a positive integer`.

---

## 7. Stale process cleanup (`staleProcessCleanup.ts`)

`cleanupStaleProcesses()`:
1. `findAllRunningProcesses()` (killed_at IS NULL). For each `proc`:
   - If `proc.log_collector_pid` truthy **and** `isProcessAlive(log_collector_pid)` → skip (collector is managing it).
   - Else if `isProcessAlive(proc.pid)` → skip (service still alive).
   - Else (both dead) → it's stale:
     - `saveProcessLog({command_name, project_dir, log_type: process_exited(6), content: 'Process cleaned up (stale entry after restart or crash)'})`.
     - `deleteProcessEntry({commandName, projectDir, pid})`.
2. `findAllKilledProcesses()` (killed_at IS NOT NULL). For each → `deleteProcessEntry(...)` unconditionally (collector died before deleting; clean them up).

`isProcessAlive(pid)` (`src/process-alive.ts:7`):
```js
try { process.kill(pid, 0); return true; }
catch (err) { if (err.code === 'EPERM') return true; return false; }
```
Semantics: signal 0 = existence check, no signal delivered.
- Process exists, we own it → no error → alive.
- `EPERM` → process exists but owned by another user → **treated as alive**.
- `ESRCH` (no such process) → dead.

**Rust equivalent (platform-specific, easy to get wrong):**
- Unix: `kill(pid, 0)` via `libc` / `nix::sys::signal::kill(Pid, None)`. Map `Ok` → alive, `EPERM` → alive, `ESRCH` → dead. Use `nix` crate or raw `libc::kill`.
- Windows: would need `OpenProcess`/`GetExitCodeProcess` — the TS code is Unix-centric (`process.kill` semantics). Candle targets macOS/Linux; document Windows as out of scope unless required.

`filterAliveProcesses(entries)` (`process-alive.ts:26`) is a related helper used by callers (not by cleanup.ts): keeps entries whose collector or pid is alive, deletes the rest from the DB. Same alive logic.

---

## 8. External npm dependencies → Rust crate mapping

| npm dep | role here | Rust replacement |
|---|---|---|
| `@facetlayer/sqlite-wrapper` (1.3.0) | thin wrapper over `node:sqlite`: `run/get/list/insert/update/upsert/count/exists`, schema loader + additive migrations, drift detection | `rusqlite` (bundled SQLite) — implement the same helper methods + an additive migration runner. `upsert` = update-then-insert-if-zero-changes (no `ON CONFLICT` needed since `process_last_cleanup` has no unique key). |
| `node:sqlite` (`DatabaseSync`) | underlying SQLite engine, synchronous | `rusqlite` (feature `bundled`) |
| `@facetlayer/parse-stdout-lines`, `@facetlayer/subprocess`, `@modelcontextprotocol/sdk`, `yargs` | not part of this subsystem (process spawning / MCP / CLI) | n/a for the database module |

No ORM; everything is hand-written SQL strings. The wrapper's `insert`/`update` builders validate identifiers with `/^[a-zA-Z_][a-zA-Z0-9_]*$/` and build `INSERT INTO t (cols) VALUES (?...)` / `UPDATE t SET c=? WHERE c=?`. Reproducible directly with `rusqlite` prepared statements.

---

## 9. Subtle / platform-specific gotchas

- **Timestamps are epoch seconds**, mixing two sources: SQLite `strftime('%s','now')` (defaults) and JS `Math.floor(Date.now()/1000)` (start_time, cutoffs). Keep seconds, not millis. Both must agree on UTC seconds.
- **WAL + busy_timeout(30s)** must be set per connection. Concurrent writers rely on this. VACUUM during cleanup briefly takes an exclusive lock.
- **Singleton DB instance**: `overrideDirectory` only honored on first `getDatabase` call. Tests rely on a fresh process/module per workspace.
- **`launch_id` vs `id` mismatch** in `ProcessEntry` (see §5) — real PK column is `id`; lookups/mutations are keyed on `(command_name, project_dir, pid)`.
- **`popStdinMessage` is not atomic** in TS — wrap in a transaction in Rust.
- **Per-service eviction deletes by `id <=`** the offset-selected cutoff; preserve the exact two-query approach to keep newest N.
- **`upsert` on `process_last_cleanup`** uses an unconditional UPDATE (no WHERE) — works only because the table is logically single-row. If two rows ever exist, both get the same timestamp; the `select timestamp from process_last_cleanup` (no LIMIT) returns the first row.
- **Stale cleanup writes a `process_exited` log line** with the exact string `'Process cleaned up (stale entry after restart or crash)'` — preserve verbatim for test parity.
- **State dir creation** is recursive; the parent `~/.local/state` may not exist.

---

## 10. Rust reimplementation notes

Suggested modules/functions and dependency ordering (bottom-up):

1. `dirs.rs` — `get_state_directory() -> PathBuf` (env precedence: `CANDLE_DATABASE_DIR` → `XDG_STATE_HOME`/candle → `~/.local/state/candle`). No deps.
2. `process_alive.rs` — `is_process_alive(pid) -> bool` via `nix`/`libc kill(pid,0)` mapping `EPERM→true`, `ESRCH→false`. Depends on `nix`/`libc`.
3. `db.rs` — connection bootstrap: ensure state dir, open `candle.db` with `rusqlite`, set `PRAGMA journal_mode=WAL` + `PRAGMA busy_timeout=30000`, run schema migration (create-if-not-exists for 4 tables + 4 indexes; additive `ALTER TABLE ADD COLUMN` for the nullable columns). Provide a shared connection (`Mutex<Connection>` or pool). Depends on 1.
4. `process_log_type.rs` — enum constants `1..=6`.
5. `process_table.rs` — `ProcessEntry` struct + the 9 functions in §5 with exact SQL. Depends on 3.
6. `stdin_messages_table.rs` — `create/pop(transactional)/clear`. Depends on 3.
7. `process_logs.rs` + `build_log_search_query.rs` — `save_process_log`, dynamic query builder, `get_process_logs_with_eviction_info`. Depends on 3,4.
8. `config_eviction.rs` — `ResolvedLogEvictionConfig` + defaults (1000 / 86400) + validation (integer ≥ 1). (Lives in the config subsystem; only the resolved struct is needed here.)
9. `stale_process_cleanup.rs` — `cleanup_stale_processes()`. Depends on 2,5,7.
10. `cleanup.rs` — `maybe_run_cleanup()` (600s gate) + `run_cleanup(cfg)` (time eviction → stale cleanup → per-service eviction → VACUUM → upsert last_cleanup). Depends on 3,7,8,9.

Build/test note: the existing test (`src/database/__tests__/cleanup.test.ts`) constructs a standalone DB with the identical schema and exercises `runCleanup` against `LOG_EVICTION_DEFAULTS`; mirror it in Rust to validate time-based + per-service eviction and the `process_last_cleanup` upsert.