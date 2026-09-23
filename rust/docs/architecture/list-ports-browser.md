# List, ports & open-browser

This subsystem covers the listing, port, and browser CLI commands. In the Rust implementation they live in `rust/src/commands/{list,list_ports,open_browser}.rs`, with process-tree walking in `rust/src/process_tree.rs`. It was ported from the original Node implementation, which has been removed; the `src/...` files and line references below are historical pointers only, and several sections still describe the Node code, with Rust differences called out:
- `list` / `ls`, `ps` / `status`, and `list-all` → `commands/list.rs` (original `src/list-command.ts`)
- `list-ports` and `list-ports-all` → `commands/list_ports.rs` (original `src/list-ports-command.ts`)
- `open-browser` → `commands/open_browser.rs` (original `src/open-browser-command.ts`)

It depends on: the SQLite `processes` table (`src/database/processTable.ts`), liveness checking (`src/process-alive.ts`), process-tree walking (`rust/src/process_tree.rs`, original `src/process-tree.ts`), config-file resolution (`src/configFile.ts`), the DB layer (`src/database/database.ts`), and state-dir resolution (`src/dirs.ts`).

## 1. Shared data model

### `processes` table (`src/database/database.ts:16-27`)
```sql
create table processes(
  id integer primary key autoincrement,
  command_name text not null,
  project_dir text not null,
  pid integer not null,
  log_collector_pid integer,
  start_time integer not null,             -- unix seconds
  created_at integer not null default (strftime('%s','now')),
  killed_at integer,                        -- NULL = still running
  shell text,
  root text
)
```
`ProcessEntry` (`rust/src/db/process_table.rs`; originally `src/database/processTable.ts:3-14`) maps 1:1 to these columns. `start_time` and `created_at`/`killed_at` are **unix seconds** (the code multiplies `start_time * 1000` to get ms). `killed_at` and `root` are nullable.

DB location (`src/dirs.ts:9-22`, `database.ts:66`): `<stateDir>/candle.db` where `stateDir` =
1. `$CANDLE_DATABASE_DIR` if set, else
2. `$XDG_STATE_HOME/candle` if set, else
3. `~/.local/state/candle`.

DB opened with `PRAGMA journal_mode=WAL` and `PRAGMA busy_timeout=30000` (`database.ts:80-81`). Created if missing (`mkdir -p`).

Relevant queries (`processTable.ts`):
- `findAllProcesses()` → `select * from processes` (no filter, includes killed).
- `findRunningProcessesByProjectDir(dir)` → `... where project_dir = ? and killed_at is null`.
- `findProcessesByProjectDir(dir)` → `... where project_dir = ?` (includes killed).
- `findProcessesByCommandNameAndProjectDir(name, dir)` → `... where command_name = ? and project_dir = ?`.
- `deleteProcessEntry({commandName, projectDir, pid})` → `delete ... where command_name=? and project_dir=? and pid=?`.

### Liveness (`src/process-alive.ts`)
`isProcessAlive(pid)`: a signal-0 `kill(pid, 0)` probe (`libc::kill` in Rust). **Subtle:** `EPERM` (process exists, other user) → **alive=true**; `ESRCH` → dead.

`filterAliveProcesses(entries)` (`process-alive.ts:26-41`): for each entry, alive if **either** `log_collector_pid` is truthy AND alive, **or** `pid` is alive. If neither, **delete the row from the DB** (side effect) and drop it. `log_collector_pid` is checked first and short-circuits.

## 2. `handleList` — `list` / `list-all` (`commands/list.rs`)

Rust: `handle_list(conn, cwd, show_all) -> Result<ListOutput, CandleError>` (originally `src/list-command.ts:47-123`, `handleList(options?: { showAll?: boolean })`).

### Return type (tests/CLI serialize `output.processes`)
```rust
struct ListOutput { processes: Vec<ListProcess> }
struct ListProcess {          // serialized in this field order, camelCase
  service_name: String,       // "serviceName"
  command: String,            // the service's shell string (NOT the service name)
  working_dir: String,        // "workingDir"
  uptime: String,             // formatted, see §2.3
  pid: Option<i64>,           // null when not running
  status: String,             // "RUNNING" | "not running" | "EXITED (<code>)" | "FAILED"
  config_changed: bool,       // "configChanged", always present; false when not running
  exit_code: Option<i64>,     // "exitCode": latest run's non-zero exit code, else null
}
```
CLI (`cmd_list` in `main.rs`): with `--json`, prints `list_output_to_json` = pretty JSON of `output.processes` (the **array**, not the wrapper). Otherwise it renders with one of the §2.4 renderers.

### 2.1 `list-all` branch (`showAll: true`, lines 49-64)
- No config file needed. `entries = filter_alive_processes(find_all_processes())`.
- Map each entry to: `serviceName = command_name`, `command = entry.shell` (or `''`), `workingDir = resolve_launch_dir(project_dir, entry.root)` (the same directory `list` reports), `uptime = formatUptime(now_ms - start_time*1000)`, `pid = pid`, `status = 'RUNNING'`, `configChanged = false` (always; no config context).
- Only alive processes appear (dead ones filtered + deleted). All listed rows are `RUNNING`.

### 2.2 `list` branch (default, lines 65-122)
1. `find_config_file(cwd)` → `{ config, project_dir }`. `MissingSetupFile` if no `.candle.json`/`.candle-setup.json` found walking up (see §6). (`cwd` is the scope's base dir; `cmd_list` first runs `require_own_config` for an explicit `--project-dir`.)
2. `configByName` = Map of `service.name → ServiceConfig`.
3. `processEntries = filterAliveProcesses(findRunningProcessesByProjectDir(projectDir))`.
4. `runningByName` = Map `command_name → entry`.
5. **First** iterate `config.services` in file order, marking each name `seen`:
   - If a running process matches the name: `status='RUNNING'`, real `pid`, `uptime` from `start_time`, `configChanged = has_config_drift(entry, service)`, `command` = the row's `shell` (falling back to the config's), `workingDir = resolve_launch_dir(project_dir, entry.root or service.root)`.
   - Else: `pid=null`, `uptime='-'`, `command = service.shell`, `workingDir = resolve_launch_dir(project_dir, service.root)`, `configChanged=false`. `status` is `EXITED (<code>)` (with `exitCode = code`) when the service's newest lifecycle log row (`process_start_initiated` / `process_start_failed` / `process_started` / `process_exited`) is an exit or start failure whose message ends in a non-zero `exited with code N`; `FAILED` (`exitCode = null`) when it is a `process_start_failed` row with no exit code (spawn failure, missing root, signal during the grace period) other than `STOPPED_WHILE_STARTING_MESSAGE` (a deliberate kill during startup); otherwise `not running` (`exitCode = null`). A signal exit ("Process was stopped") is not a crash. `start`'s up-front missing-root check also writes `process_start_initiated` + `process_start_failed` rows (only when no instance is running), so that case shows `FAILED` too. `list-all` lists only running processes, so it never shows `EXITED` or `FAILED`. Name filtering (`filter_by_service_names`) errors with `CandleError::unknown_service` (`No service '<name>' configured for directory: <dir>`); for `list-all`, which has no project, `No running service named '<name>'`.
6. **Then** iterate running entries again; for any whose `command_name` was not in config (transient/orphan), append with `status='RUNNING'`, real pid/uptime, `workingDir = resolve_launch_dir(project_dir, entry.root)` (so `--root` shows), `configChanged = has_config_drift(entry, find_service_by_name(...))`.

`resolve_launch_dir` (`rust/src/dirs.rs`) is the same helper the start banner uses: absolute root wins, relative root is joined, and the result is lexically normalized. (The Node `list` reported the bare project dir.)

Ordering matters: configured services first (config order), then unconfigured running processes (DB row order).

### `hasConfigDrift` (`list-command.ts:23-45`)
Returns `false` if no matching config service. Else `true` if `entry.shell !== service.shell`, OR `(entry.root || undefined) !== (service.root || undefined)` (empty string / null / undefined are normalized to `undefined` and treated equal). Otherwise `false`.

### 2.3 `formatUptime(ms)` (`list-command.ts:125-139`)
`totalSeconds = floor(ms/1000)`; days=`/86400`, hours=`%86400/3600`, minutes=`%3600/60`, secs=`%60`. Push `"{d}d"`, `"{h}h"`, `"{m}m"`, `"{s}s"` only for nonzero parts; **if all zero, emit `"0s"`**. Join with single space. E.g. `"1d 2h"`, `"3m 5s"`, `"0s"`.

### 2.4 Renderers (`commands/list.rs`)
Three renderers share one `ListOutput`. All three print exactly `No services configured.` when
`processes` is empty.

**`format_list_detail` — the `candle list` / `ls` multiline view.** One entry per service:
a single header line `<name>  <status>` (with ` [config changed]` appended on drift), followed on
the same line, only when RUNNING, by `  pid <pid>` and `  uptime <uptime>`. Two indented detail
lines follow:
`  command:   <shell>` and `  directory: <workingDir>`, both printed in full and never truncated.
Entries are separated by a blank line.

**`format_ps_output` — the `candle ps` / `status` table.** Columns `NAME STATUS PID UPTIME` only;
`COMMAND` and `DIRECTORY` are dropped so the table stays narrow.

**`format_list_output` — the full table, now used only by `list-all`.** Columns
`NAME STATUS PID UPTIME COMMAND DIRECTORY`.

Shared by both tables: status gets ` [config changed]` appended when `configChanged` is truthy;
`pid` cell = `pid > 0 ? pid.toString() : '-'`. Column widths = max(header len, all cell lens).
Cells padded to width, joined by **two spaces** (`'  '`). Separator row = `'-'.repeat(width)` per
column joined by two spaces. Print header, separator, then rows.

Both `list` and `ps` accept zero or more positional service names (`filter_by_service_names`),
which filter the listing — and the `--json` array — to just those services; an unmatched name is a
usage error (`No service found with name: <name>`) that exits non-zero. `list-all` takes no names
filter in practice but goes through the same code.

## 3. `handleListPorts` — `list-ports` / `list-ports-all` (`commands/list_ports.rs`)

Rust: `handle_list_ports(conn, cwd, show_all, command_names) -> Result<ListPortsOutput, CandleError>` (originally `src/list-ports-command.ts:23-78`).

### Return type
```ts
interface PortInfo {
  serviceName: string;
  pid: number;
  port: number;
  address: string;       // normalized; "*" → "0.0.0.0"
  protocol: string;      // "TCP" (UDP possible but filter is LISTEN→TCP only in practice)
  isChildProcess: boolean; // pid !== root service pid
}
interface ListPortsOutput { ports: PortInfo[]; }
```
With `--json` the CLI prints `list_ports_output_to_json` (the `{ ports: [...] }` wrapper, same as MCP `ListPorts`); otherwise `format_list_ports_output`.

### Algorithm
1. `showAll`: `processEntries = find_all_processes()`, no config needed (so `list-ports-all` works outside a project).
2. Otherwise `find_config_file(cwd)` → `project_dir` (errors if none), `processEntries = find_processes_by_project_dir(project_dir)`, and each requested name must be configured (exact match) or have a row in `processEntries`, else `MissingServiceWithName`. `open-browser <name>` inherits this check. **Note:** this uses the *non-running* query — includes `killed_at` rows. No `filterAliveProcesses` here; dead pids simply own no listening sockets.
3. If `commandNames` non-empty, filter `processEntries` to those whose `command_name ∈ commandNames`.
4. For each entry, compute its full process tree `getProcessTree(entry.pid)`.
5. Collect **all** pids across all trees into `allPids`. If empty → return `{ ports: [] }`.
6. `getListeningPorts(allPids)` → raw `{pid, port, address, protocol}[]`.
7. Build `pidToService: Map<pid → {serviceName, rootPid}>` from the trees (later trees overwrite earlier on pid collision).
8. For each raw port, look up its pid in `pidToService`; skip if absent. Emit `PortInfo` with `isChildProcess = raw.pid !== service.rootPid`.

### 3.1 Process tree (`rust/src/process_tree.rs`, original `src/process-tree.ts`)
`get_process_tree(root_pid)`: worklist traversal starting from `root_pid` (included), repeatedly calling `get_child_pids(pid)`:
- **macOS (`darwin`)**: `pgrep -P <pid>` → child pids.
- **Linux**: `ps -o pid --no-headers --ppid <pid>`.
- **Windows**: `powershell -NoProfile -NonInteractive -Command "Get-CimInstance Win32_Process -Filter \"ParentProcessId=<pid>\" | Select-Object -ExpandProperty ProcessId"` (`wmic` is deprecated/absent on recent Windows).
- **other platforms**: returns `[]` (no descendants).
Output parsing: iterate `lines()` (handles `\r\n`), trim, parse base-10 integers, drop non-numeric. On spawn error → `[]`. Result includes the root pid plus all transitive descendants.

### 3.2 Port detection — `listening_sockets_for_pids` (`rust/src/listening_ports.rs`)
Given the full pid set, returns `Result<Vec<ListeningSocket>, PortLookupError>`. Empty pid set → `Ok([])` without touching the system. Results are filtered to the requested pids and **deduped** by `(pid, port)`, keeping the first address seen (a dual-stack listener shows once). Per platform:

- **Linux**: read `/proc/net/tcp` then `/proc/net/tcp6` (optional), keep rows with state `0A` (LISTEN), decode the hex `local_address` (IPv4: one little-endian u32; IPv6: four little-endian u32 words, rendered bracketed like `[::1]`), and take the `inode` column. Then map inode → pid by `read_link` on `/proc/<pid>/fd/*` for the requested pids only (`socket:[N]`). Needs no external tools and no root for the user's own processes. If `/proc/net/tcp` is unreadable, fall back to lsof; if that also fails → `PortLookupError::ProcUnavailable` naming both causes.
- **macOS / other Unix**: `lsof -iTCP -sTCP:LISTEN -n -P` (stdin/stderr ignored). Parse: skip lines without `LISTEN`; split on whitespace, need ≥9 fields; pid = field 1; protocol = first `TCP`/`UDP` token else `TCP`; name = second-to-last field split at its **last** `:` (handles `[::1]:3000`); `*` → `0.0.0.0`.
- **Windows**: `netstat -ano -p TCP`. Rows with exactly 5 fields, `TCP`, state starting `LISTEN` (or `ABH`, German) → pid = field 5, address:port = field 2 (same last-colon split).

**Errors** (`PortLookupError`, `Display` prefixed `Could not detect listening ports: `): `ToolNotFound {tool, hint}` when the spawn fails with `NotFound` (hint says how to install), `ToolFailed {tool, detail}` for any other spawn error, `ProcUnavailable {detail, fallback}` on Linux. A non-zero exit from the tool is *not* an error (lsof exits 1 when nothing matches). `handle_list_ports` maps the error to `CandleError::Generic`, so the CLI prints `Error: Could not detect listening ports: ...` and exits 1, and `open-browser` / MCP `ListPorts` surface the same message. This replaces the Node behaviour of silently returning `[]` (which showed a misleading "No open ports found").

All three parsers are pure functions unit-tested on every host; only the tool/`/proc` access is `cfg`-gated. `finds_own_listening_socket` binds a port in-process and checks the platform path end to end.

### 3.3 `printListPortsOutput` (`list-ports-command.ts:194-225`)
- Empty → print exactly `No open ports found for running services.` and return.
- Headers `SERVICE PID PORT ADDRESS PROTOCOL`. The PROTOCOL cell gets suffix ` (child)` when `isChildProcess`. Same column-pad/two-space-join table formatting as §2.4.

## 4. `handleOpenBrowser` — `open-browser` (`commands/open_browser.rs`)

Rust: `handle_open_browser(conn, cwd, project_dir, service_name: Option<&str>) -> Result<OpenBrowserOutput, CandleError>` (originally `src/open-browser-command.ts:41-75`).

### Return type
```ts
interface OpenBrowserOutput { serviceName: string; port: number; url: string; }
```

### Algorithm
1. **Resolve service name** (`resolveServiceName`, lines 18-39):
   - If a non-empty `service_name` is provided → use it.
   - Else `findProcessesByProjectDir(projectDir)` (includes killed rows). If `length === 0` → throw `UsageError('No service name provided and no running processes found in this project.')`. If `length > 1` → throw `UsageError('No service name provided and multiple processes are running: <names joined ", ">. Please specify which service to open.')`. Else use the single `command_name`.
   - **Subtle:** "running" here actually means any row in `processes` for the dir (killed included), since the query is `findProcessesByProjectDir`, not the running-only variant.
2. `handle_list_ports(conn, cwd, false, [service_name])` → ports for just that service.
3. If no ports:
   - `findProcessesByCommandNameAndProjectDir(serviceName, projectDir)`; `isRunning = some(p.killed_at === null)`.
   - If running → throw `UsageError("No open ports found for service '<name>'.")`.
   - Else → throw `UsageError("No open ports found for service '<name>'. Start the service with: candle start")`.
4. Pick port: sort ports ascending by `port`, take the lowest (`sortedPorts[0]`). `url = "http://localhost:" + port` (always `localhost`, ignores the bind address).
5. `open_url(url)` then return `{ service_name, port, url }`.

### `open_url` — per-platform browser launch (originally `openUrl`, lines 77-111)
| platform | command | args |
|---|---|---|
| `darwin` | `open` | `[url]` |
| `win32` | `cmd` | `['/c','start','',url]` (note empty title arg) |
| else (linux/other) | `xdg-open` | `[url]` |

The platform is chosen with `cfg!(target_os)`. Spawned with stdin/stdout/stderr null and never waited on; a spawn error → `Generic("Failed to open browser: <e>")`. (The Node original also set `detached: true` and `unref()`; the Rust code does not `setsid`, but the opener exits on its own.)

### `format_open_browser_output` (originally `printOpenBrowserOutput`, lines 113-115)
Prints exactly: `Opened <url> in browser`.

## 5. CLI wiring (`rust/src/main.rs`; originally `src/main-cli.ts`)

Command definitions:
- `list` / `ls`, `ps` / `status`, and `list-all`: all accept `--json`; `list` and `ps` also accept `--project-dir`. `cmd_list(args, show_all, view)` runs `require_own_config`, `handle_list`, `filter_by_service_names`, then prints JSON or the `Detail` / `PsTable` / `FullTable` renderer.
- `list-ports` (`--project-dir` accepted) → `cmd_list_ports(args, false)`; `list-ports-all` → `cmd_list_ports(args, true)`. Both call `handle_list_ports(conn, base_dir, show_all, &[])`.
- `open-browser [name]` (`--project-dir` accepted) → `configured_project_dir_or_exit`, `service_name = positionals[0]`, `handle_open_browser(conn, base_dir, project_dir, service_name)`.

**`list-ports` positional-name quirk (preserved):** in the Node original the positional for `list-ports` was declared as `names`, but arg extraction (`:266-270`) read `argv.name` (singular) to build `commandNames`. So `list-ports foo bar` did **not** populate `commandNames` and instead listed ports for all project processes. The Rust `cmd_list_ports` reproduces this by always passing an empty name list. `commandNames` is reliably populated for commands declared with `[name...]`/`[name]`, and the internal `handleListPorts` filter itself works correctly when given names (open-browser relies on it).

All commands use strict option parsing (`Unknown argument` on unrecognized flags).

## 6. Config resolution (`rust/src/config/file.rs`; originally `src/configFile.ts`) — needed by all three

`find_config_file(cwd)` (`:68-98`): walk from the resolved `cwd` upward; at each dir test `.candle.json` then `.candle-setup.json` (priority order, `CONFIG_FILENAMES`). First existing → parse via `readConfigFile` and return `{ config, projectDir, configFilename }`. If a file exists but parse fails → `ConfigFileError("Invalid <filename> at <path>: <msg>")`. If the filesystem root is reached with nothing → `MissingSetupFile { cwd: starting_dir }` (message: `No .candle.json file found in (or above) current directory: <cwd>`).

`findProjectDir(cwd)` (`:37-44`) returns just `projectDir`.

`readConfigFile`: read UTF-8, trim; empty file → `{ services: [] }`; else `JSON.parse`, default `services=[]`, then `validateConfig`. `ServiceConfig = { name, shell, root?, enableStdin? }`. Only `name` and `shell` matter for this subsystem (drift detection compares `shell` and `root`).

Errors: `UsageError`/`MissingSetupFile`/`MissingServiceWithName` are usage errors (`CandleError::is_usage_error`, see [cli.md](cli.md)); the CLI prints every error's message to stderr and exits 1.

## 7. Subtleties / correctness notes

- **Time units:** `start_time` is unix **seconds**; uptime computed as `now_ms - start_time*1000`. Not treated as ms.
- **`formatUptime` zero case:** emits `"0s"` when all components are zero (other "not running" rows use the literal `"-"`, set separately).
- **`pid=None`** (JSON `null`) for not-running rows; printed as `"-"`.
- **liveness EPERM → alive**; only ESRCH (no-such-process) is dead. And `filterAliveProcesses` **deletes** dead rows as a side effect (mutating the DB during a read command).
- **`log_collector_pid` checked before `pid`** in liveness, and only if truthy/nonzero.
- **list-ports uses `findProcessesByProjectDir` (includes killed)** and does NOT prune via `filterAliveProcesses`; correctness comes from dead pids owning no listening sockets. open-browser's `resolveServiceName` likewise counts killed rows as "processes."
- **Port detection is per-platform** (`listening_ports.rs`): `/proc` on Linux, `lsof` on macOS, `netstat` on Windows — one system-wide read then in-memory filter to the pid set, never per-pid. A missing tool is a hard error with an install hint, not an empty result.
- **Process tree is platform-specific** (`pgrep -P` on macOS, `ps --ppid` on Linux, empty elsewhere). `isChildProcess` depends on it.
- **open-browser always builds `http://localhost:<port>`**, ignoring bind address; picks the numerically lowest port.
- **Table formatting:** two-space column separator, padded cells, dashed separator line; exact empty-state strings (`No services configured.`, `No open ports found for running services.`, `Opened <url> in browser`) are user-facing.
- **`--json` prints the inner array**, not `{processes: [...]}`.

## 8. Implementation dependencies

| Node API | Used for | Rust equivalent |
|---|---|---|
| `@facetlayer/sqlite-wrapper` (`DatabaseLoader`, `SqliteDatabase`) | SQLite access, WAL, migrations | `rusqlite` |
| `node:child_process` `spawn` | run `lsof`/`netstat`, `pgrep`/`ps`/PowerShell, browser opener | `std::process::Command` |
| `process.kill(pid,0)` | liveness | `libc::kill(pid, 0)` |
| `os.platform()` / `process.platform` | platform branch | `#[cfg(target_os = ...)]` |
| `path`, `fs` | config walk-up, state dir | `std::path`, `std::fs`, `$HOME` for `~` |
| `yargs` | CLI parsing | hand-rolled parser (`rust/src/cli/parser.rs`) |
| Browser open | `open`/`xdg-open`/`cmd start` | same per-platform commands (including the win32 empty-title arg) |

Cross-module dependency edges: open-browser → list-ports (calls `handleListPorts` with a single command name); list/list-ports → config_file + process_table; everything → db.

## 9. Source files

Rust modules: `rust/src/commands/list.rs`, `rust/src/commands/list_ports.rs`, `rust/src/commands/open_browser.rs`, `rust/src/process_tree.rs`.

Historical Node sources (removed from the repo): `src/list-command.ts`, `src/list-ports-command.ts`, `src/open-browser-command.ts`, `src/database/processTable.ts`, `src/database/database.ts`, `src/process-alive.ts`, `src/process-tree.ts`, `src/configFile.ts`, `src/dirs.ts`, `src/errors.ts`, `src/main-cli.ts`. Tests: `test/cli/list.test.ts`, `test/cli/list-all.test.ts`.
