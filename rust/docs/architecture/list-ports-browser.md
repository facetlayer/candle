# List, ports & open-browser

This subsystem covers five CLI commands. In the Rust implementation they live in `rust/src/commands/{list,list_ports,open_browser}.rs`, with process-tree walking in `rust/src/process_tree.rs`. It mirrors the original Node implementation:
- `list` / `ls` and `list-all` → `commands/list.rs` (original `src/list-command.ts`)
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
`ProcessEntry` (`src/database/processTable.ts:3-14`) maps 1:1 to these columns. `start_time` and `created_at`/`killed_at` are **unix seconds** (the code multiplies `start_time * 1000` to get ms). `killed_at` and `root` are nullable.

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
`isProcessAlive(pid)`: a signal-0 `kill(pid, 0)` probe (via `nix`/`libc` in Rust). **Subtle:** `EPERM` (process exists, other user) → **alive=true**; `ESRCH` → dead.

`filterAliveProcesses(entries)` (`process-alive.ts:26-41`): for each entry, alive if **either** `log_collector_pid` is truthy AND alive, **or** `pid` is alive. If neither, **delete the row from the DB** (side effect) and drop it. `log_collector_pid` is checked first and short-circuits.

## 2. `handleList` — `list` / `list-all` (`commands/list.rs`)

`src/list-command.ts:47-123`. Signature: `handleList(options?: { showAll?: boolean }) -> ListOutput`.

### Return type (tests/CLI serialize `output.processes`)
```ts
interface ListOutput {
  processes: {
    command: string;       // == serviceName
    workingDir: string;
    uptime: string;        // formatted, see §2.3
    pid: number;           // 0 when not running
    status: string;        // "RUNNING" | "not running"
    serviceName: string;
    configChanged?: boolean;
  }[];
  showAll?: boolean;       // never actually set by handleList
  message?: string;        // never set by handleList
}
```
CLI (`main-cli.ts:356-375`): with `--json`, prints `JSON.stringify(output.processes, null, 2)` (the **array**, not the wrapper). Otherwise calls `printListOutput`.

### 2.1 `list-all` branch (`showAll: true`, lines 49-64)
- No config file needed. `processEntries = filterAliveProcesses(findAllProcesses())`.
- Map each entry to: `serviceName = command = command_name`, `workingDir = project_dir`, `uptime = formatUptime(now_ms - start_time*1000)`, `pid = pid`, `status = 'RUNNING'`, `configChanged = false` (always; no config context).
- Only alive processes appear (dead ones filtered + deleted). All listed rows are `RUNNING`.

### 2.2 `list` branch (default, lines 65-122)
1. `findConfigFile(cwd)` → `{ config, projectDir }`. Throws `MissingSetupFileError` if no `.candle.json`/`.candle-setup.json` found walking up (see §6).
2. `configByName` = Map of `service.name → ServiceConfig`.
3. `processEntries = filterAliveProcesses(findRunningProcessesByProjectDir(projectDir))`.
4. `runningByName` = Map `command_name → entry`.
5. **First** iterate `config.services` in file order, marking each name `seen`:
   - If a running process matches the name: `status='RUNNING'`, real `pid`, `uptime` from `start_time`, `configChanged = hasConfigDrift(runningProcess, service)`, `workingDir = projectDir`.
   - Else: `status='not running'`, `pid=0`, `uptime='-'`, **no `configChanged` field emitted**.
6. **Then** iterate running entries again; for any whose `command_name` was not in config (transient/orphan), append with `status='RUNNING'`, real pid/uptime, `workingDir = entry.project_dir`, `configChanged = hasConfigDrift(entry, configByName.get(name))`.

Ordering matters: configured services first (config order), then unconfigured running processes (DB row order).

### `hasConfigDrift` (`list-command.ts:23-45`)
Returns `false` if no matching config service. Else `true` if `entry.shell !== service.shell`, OR `(entry.root || undefined) !== (service.root || undefined)` (empty string / null / undefined are normalized to `undefined` and treated equal). Otherwise `false`.

### 2.3 `formatUptime(ms)` (`list-command.ts:125-139`)
`totalSeconds = floor(ms/1000)`; days=`/86400`, hours=`%86400/3600`, minutes=`%3600/60`, secs=`%60`. Push `"{d}d"`, `"{h}h"`, `"{m}m"`, `"{s}s"` only for nonzero parts; **if all zero, emit `"0s"`**. Join with single space. E.g. `"1d 2h"`, `"3m 5s"`, `"0s"`.

### 2.4 `printListOutput` (`list-command.ts:141-183`)
- If `output.message` → print it, return.
- If `processes.length === 0` → print exactly `No services configured.` and return.
- Headers: `NAME STATUS PID UPTIME COMMAND DIRECTORY`.
- Per row: status string gets ` [config changed]` appended when `configChanged` truthy. `pid` cell = `pid > 0 ? pid.toString() : '-'`.
- Column widths = max(header len, all cell lens). Cells padded to width, joined by **two spaces** (`'  '`). Separator row = `'-'.repeat(width)` per column joined by two spaces. Print header, separator, then rows.

## 3. `handleListPorts` — `list-ports` / `list-ports-all` (`commands/list_ports.rs`)

`src/list-ports-command.ts:23-78`. Signature: `handleListPorts(options?: { showAll?: boolean; commandNames?: string[] }) -> ListPortsOutput`.

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
The CLI never JSON-serializes this; only `printListPortsOutput` is used.

### Algorithm
1. `findConfigFile(cwd)` → `projectDir` (throws if none).
2. `processEntries = showAll ? findAllProcesses() : findProcessesByProjectDir(projectDir)`. **Note:** this uses the *non-running* query — includes `killed_at` rows. No `filterAliveProcesses` here; dead pids simply yield no lsof matches.
3. If `commandNames` non-empty, filter `processEntries` to those whose `command_name ∈ commandNames`.
4. For each entry, compute its full process tree `getProcessTree(entry.pid)`.
5. Collect **all** pids across all trees into `allPids`. If empty → return `{ ports: [] }`.
6. `getListeningPorts(allPids)` → raw `{pid, port, address, protocol}[]`.
7. Build `pidToService: Map<pid → {serviceName, rootPid}>` from the trees (later trees overwrite earlier on pid collision).
8. For each raw port, look up its pid in `pidToService`; skip if absent. Emit `PortInfo` with `isChildProcess = raw.pid !== service.rootPid`.

### 3.1 Process tree (`rust/src/process_tree.rs`, original `src/process-tree.ts`)
`getProcessTree(rootPid)`: BFS/DFS starting from `rootPid` (included), repeatedly calling `getChildPids(pid)`:
- **macOS (`darwin`)**: `pgrep -P <pid>` → child pids.
- **Linux**: `ps -o pid --no-headers --ppid <pid>`.
- **other platforms**: returns `[]` (no descendants).
Output parsing: trim, split on `\n`, drop empties, parse base-10 integers, drop non-numeric. On spawn error → `[]`. Result includes the root pid plus all transitive descendants.

### 3.2 Port detection — `getListeningPorts` (`list-ports-command.ts:87-118`)
Runs **one** command: `lsof -iTCP -sTCP:LISTEN -n -P` (stdin ignored, stderr ignored, stdout captured). Parses **all** listening sockets system-wide, then filters to pids in the requested set. On spawn error (e.g. lsof missing) → returns `[]`.

**`parseLsofOutput` (`list-ports-command.ts:128-192`)** — exact rules:
- Split stdout on `\n`. Skip any line not containing the substring `LISTEN`.
- Split line on `/\s+/`. Skip if `< 9` fields.
- pid = base-10 parse of `parts[1]`; skip if non-numeric.
- protocol = value of first field equal to `"TCP"` or `"UDP"`; if none found default `"TCP"`.
- name column = `parts[parts.length - 2]` (second-to-last, since last is `(LISTEN)`). Skip if missing or has no `:`.
- Split address/port at the **last** `:` (`lastIndexOf`). address = before, portStr = after, `port = parseInt(portStr,10)`; skip if non-numeric. This handles IPv6 like `[::1]:3000` (address `[::1]`) and `*:8080`.
- Normalize: address `"*"` → `"0.0.0.0"`.
- **Dedup**: keep first occurrence per `"${pid}:${port}"` key (lsof prints IPv4+IPv6 separate lines for the same listener).

Example lsof line that parses (from the doc comment, lines 123-126):
```
node    12345   user   45u  IPv4 0x1234    0t0  TCP 127.0.0.1:3000 (LISTEN)
```

### 3.3 `printListPortsOutput` (`list-ports-command.ts:194-225`)
- Empty → print exactly `No open ports found for running services.` and return.
- Headers `SERVICE PID PORT ADDRESS PROTOCOL`. The PROTOCOL cell gets suffix ` (child)` when `isChildProcess`. Same column-pad/two-space-join table formatting as §2.4.

## 4. `handleOpenBrowser` — `open-browser` (`commands/open_browser.rs`)

`src/open-browser-command.ts:41-75`. Signature: `handleOpenBrowser({ projectDir, serviceName? }) -> OpenBrowserOutput`.

### Return type
```ts
interface OpenBrowserOutput { serviceName: string; port: number; url: string; }
```

### Algorithm
1. **Resolve service name** (`resolveServiceName`, lines 18-39):
   - If `serviceName` provided → use it.
   - Else `findProcessesByProjectDir(projectDir)` (includes killed rows). If `length === 0` → throw `UsageError('No service name provided and no running processes found in this project.')`. If `length > 1` → throw `UsageError('No service name provided and multiple processes are running: <names joined ", ">. Please specify which service to open.')`. Else use the single `command_name`.
   - **Subtle:** "running" here actually means any row in `processes` for the dir (killed included), since the query is `findProcessesByProjectDir`, not the running-only variant.
2. `handleListPorts({ commandNames: [serviceName] })` → ports for just that service.
3. If no ports:
   - `findProcessesByCommandNameAndProjectDir(serviceName, projectDir)`; `isRunning = some(p.killed_at === null)`.
   - If running → throw `UsageError("No open ports found for service '<name>'.")`.
   - Else → throw `UsageError("No open ports found for service '<name>'. Start the service with: candle start")`.
4. Pick port: sort ports ascending by `port`, take the lowest (`sortedPorts[0]`). `url = "http://localhost:" + port` (always `localhost`, ignores the bind address).
5. `openUrl(url)` then return `{ serviceName, port, url }`.

### `openUrl` (lines 77-111) — per-platform browser launch
| platform | command | args |
|---|---|---|
| `darwin` | `open` | `[url]` |
| `win32` | `cmd` | `['/c','start','',url]` (note empty title arg) |
| else (linux/other) | `xdg-open` | `[url]` |

Spawned with `stdio:'ignore', detached:true`. On `'spawn'` event → `child.unref()` and resolve. On `'error'` → reject `Error("Failed to open browser: <msg>")`. The child is fully detached so candle can exit without killing the browser.

### `printOpenBrowserOutput` (lines 113-115)
Prints exactly: `Opened <url> in browser`.

## 5. CLI wiring (`src/main-cli.ts`)

Command definitions:
- `['list','ls']` and `list-all`: both accept `--json` boolean (`:139-140`). Dispatch `:356-375`: `handleList({})` vs `handleList({showAll:true})`; `--json` → print `JSON.stringify(output.processes, null, 2)`.
- `list-ports [names...]` (`:180`) → `handleListPorts({ commandNames })` (`:378`). `list-ports-all` (`:181`) → `handleListPorts({ showAll:true })` (`:384`).
- `open-browser [name]` (`:182`) → `projectDir = findProjectDir(); serviceName = commandNames[0]; handleOpenBrowser({projectDir, serviceName})` (`:389-394`).

**`list-ports` positional-name quirk (preserved):** the positional for `list-ports` is declared as `names`, but arg extraction (`:266-270`) reads `argv.name` (singular) to build `commandNames`. So as wired, `list-ports foo bar` does **not** populate `commandNames` and instead lists ports for all project processes. This quirk is reproduced in the Rust implementation. `commandNames` is reliably populated for commands declared with `[name...]`/`[name]`, and the internal `handleListPorts` filter itself works correctly when given names (open-browser relies on it).

All commands use strict option parsing.

## 6. Config resolution (`src/configFile.ts`) — needed by all three

`findConfigFile(cwd)` (`:68-98`): walk from the resolved `cwd` upward; at each dir test `.candle.json` then `.candle-setup.json` (priority order, `CONFIG_FILENAMES`). First existing → parse via `readConfigFile` and return `{ config, projectDir, configFilename }`. If a file exists but parse fails → throw `Error("Invalid <filename> at <path>: <msg>")`. If the filesystem root is reached with nothing → throw `MissingSetupFileError(startingDir)` (message: `No .candle.json file found in (or above) current directory: <cwd>`).

`findProjectDir(cwd)` (`:37-44`) returns just `projectDir`.

`readConfigFile`: read UTF-8, trim; empty file → `{ services: [] }`; else `JSON.parse`, default `services=[]`, then `validateConfig`. `ServiceConfig = { name, shell, root?, enableStdin? }`. Only `name` and `shell` matter for this subsystem (drift detection compares `shell` and `root`).

Errors: `UsageError`/`MissingSetupFileError`/`MissingServiceWithNameError` all carry `isUsageError = true` (`src/errors.ts`), used by the top-level CLI to print a clean message instead of a stack trace.

## 7. Subtleties / correctness notes

- **Time units:** `start_time` is unix **seconds**; uptime computed as `now_ms - start_time*1000`. Not treated as ms.
- **`formatUptime` zero case:** emits `"0s"` when all components are zero (other "not running" rows use the literal `"-"`, set separately).
- **`pid=0` sentinel** for not-running rows; printed as `"-"`.
- **liveness EPERM → alive**; only ESRCH (no-such-process) is dead. And `filterAliveProcesses` **deletes** dead rows as a side effect (mutating the DB during a read command).
- **`log_collector_pid` checked before `pid`** in liveness, and only if truthy/nonzero.
- **list-ports uses `findProcessesByProjectDir` (includes killed)** and does NOT prune via `filterAliveProcesses`; correctness comes from lsof simply not matching dead pids. open-browser's `resolveServiceName` likewise counts killed rows as "processes."
- **lsof parsing** is positional and brittle: relies on the `LISTEN` substring, `>=9` whitespace fields, name = second-to-last token, split at last `:`. The dedup-by-`pid:port` and `*`→`0.0.0.0` normalization are reproduced. Default protocol `"TCP"` if no TCP/UDP token found.
- **Single global lsof call** then in-memory filter — not per-pid; preserved for performance and to match output.
- **Process tree is platform-specific** (`pgrep -P` on macOS, `ps --ppid` on Linux, empty elsewhere). `isChildProcess` depends on it.
- **open-browser always builds `http://localhost:<port>`**, ignoring bind address; picks the numerically lowest port.
- **Table formatting:** two-space column separator, padded cells, dashed separator line; exact empty-state strings (`No services configured.`, `No open ports found for running services.`, `Opened <url> in browser`) are user-facing.
- **`--json` prints the inner array**, not `{processes: [...]}`.

## 8. Implementation dependencies

| Node API | Used for | Rust equivalent |
|---|---|---|
| `@facetlayer/sqlite-wrapper` (`DatabaseLoader`, `SqliteDatabase`) | SQLite access, WAL, migrations | `rusqlite` |
| `node:child_process` `spawn` | run `lsof`, `pgrep`/`ps`, browser opener | `std::process::Command` |
| `process.kill(pid,0)` | liveness | `nix::sys::signal::kill(Pid, None)` / `libc::kill` |
| `os.platform()` / `process.platform` | platform branch | `std::env::consts::OS` / `cfg!(target_os=...)` |
| `path`, `fs` | config walk-up, state dir | `std::path`, `std::fs`, `dirs`/`home` for `~` |
| `yargs` | CLI parsing | `clap` |
| Browser open | `open`/`xdg-open`/`cmd start` | same per-platform commands (including the win32 empty-title arg) |

Cross-module dependency edges: open-browser → list-ports (calls `handleListPorts` with a single command name); list/list-ports → config_file + process_table; everything → db.

## 9. Source files

Rust modules: `rust/src/commands/list.rs`, `rust/src/commands/list_ports.rs`, `rust/src/commands/open_browser.rs`, `rust/src/process_tree.rs`.

Historical Node source of truth: `src/list-command.ts`, `src/list-ports-command.ts`, `src/open-browser-command.ts`, `src/database/processTable.ts`, `src/database/database.ts`, `src/process-alive.ts`, `src/process-tree.ts`, `src/configFile.ts`, `src/dirs.ts`, `src/errors.ts`, `src/main-cli.ts`. Tests: `test/cli/list.test.ts`, `test/cli/list-all.test.ts`.
