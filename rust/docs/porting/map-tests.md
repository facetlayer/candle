# Candle Test Subsystem — Technical Specification for Rust Reimplementation

## 1. Overview & test runner mechanics

The test suite is **Vitest 3** (`vitest.config.ts`), TypeScript, run from the repo root.

`vitest.config.ts`:
```ts
test: {
    globals: true,
    environment: 'node',
    testTimeout: 30000,   // 30s — needed for subprocess-heavy tests
    hookTimeout: 10000,
    setupFiles: ['test/setup.ts'],
}
```
`test/setup.ts` is one line: `import 'expect-mcp/vitest-setup';` (registers custom matchers `toBeSuccessful`, `toHaveTool(s)`, `toMatchTextContent`).

`package.json` scripts:
- `test`: **`pnpm build && vitest run`** — note the **build step is mandatory** (see §6: the log-collector binary that the CLI spawns is `dist/main-log-collector.js`, not a `src/` file).
- `test:watch`: `vitest`.

The repo runs on **Node v24** (`node --version` → v24.14.0). The CLI is invoked as `node src/main-cli.ts` — i.e. Node's native TypeScript type-stripping runs the `.ts` entrypoint directly. There is **no compile step for the CLI itself**; only the log-collector is consumed from `dist/`.

### How the candle binary is spawned (the central seam)
Two distinct things are spawned, from two different paths:

| What | Path used | Spawn form |
|---|---|---|
| The CLI under test | `<repo>/src/main-cli.ts` | `node <path> <args>` |
| MCP server under test | same | `node <path> --mcp` |
| The log-collector (spawned by the CLI) | `<repo>/dist/main-log-collector.js` | `<node> <path>` (stdin = JSON) |
| Rust log-collector (opt-in) | `<repo>/rust/target/release/candle-log-collector` | direct exec, stdin = JSON |

## 2. `TestWorkspace` helper (`test/TestWorkspace.ts`)

This is the primary harness used by nearly every test. Re-exported from `test/cli/utils.ts`.

Constructor: `new TestWorkspace(name: string)`
- `this.dbDir = path.join(__dirname, 'workspaces', name)` → `test/workspaces/<name>`. Used as **both** the cwd and the SQLite database directory.
- `this.cliPath = path.join(__dirname, '..', 'src', 'main-cli.ts')` ← **SEAM #1**: hardcoded Node entrypoint.
- Creates `dbDir` if missing (`mkdirSync recursive`).

Each workspace ships a committed `.candle.json` (see §9). DB files (`candle.db`, `-shm`, `-wal`) live in the same dir and are git-ignored (`*.db*`), but the directory tree and `.candle.json` are force-committed despite `.gitignore` having `test/workspaces/`.

### `runCli(args, options): Promise<SubprocessResult>`  (lines 60–82)
```ts
const cwd = this.dbDir;
const env = {
    ...process.env,
    CANDLE_DATABASE_DIR: cwd,   // ← DB isolation
    FORCE_COLOR: '0',           // ← disable ANSI color in output
    CLAUDECODE: '',             // ← force isRunByAgent=false (see §8)
    ...(options.env || {}),
};
const result = await runShellCommand('node', [this.cliPath, ...args], {
    cwd: options.cwd ?? cwd, env,
});
if (result.failed() && !options.ignoreExitCode) throw result.asError();
return result;
```
`CliOptions`: `{ cwd?, env?, ignoreExitCode? }`. Default behavior **throws on non-zero exit**; tests that expect failure pass `ignoreExitCode: true` and assert `result.failed() === true`.

The returned `SubprocessResult` (from `@facetlayer/subprocess`) exposes: `exitCode: number|null`, `failed(): boolean`, `asError(): Error`, `stdoutAsString(): string`, `stderrAsString(): string`. stdout/stderr are stored internally as arrays of lines and joined.

### `createMcpApp(options?): MCPStdinSubprocess`  (lines 88–100)
```ts
return mcpShell(`node ${this.cliPath} --mcp`, {
    allowDebugLogging,
    cwd: this.dbDir,
    env: { ...process.env, CANDLE_DATABASE_DIR: this.dbDir, CLAUDECODE: '' },
});
```
Note: MCP env does **not** set `FORCE_COLOR`. `mcpShell` comes from `expect-mcp` and drives a stdio MCP client.

### `ensureSubdir(name)` (106–111) — mkdir a subdir inside the workspace (for `--root` tests).
### `cleanup()` (119–134) — runs `node <cliPath> kill-all` with `CANDLE_DATABASE_DIR=dbDir`, swallows errors. **Does NOT delete the DB** — only kills processes. Called in `afterAll`.

## 3. Other test utilities

`test/utils.ts`:
- `getTestTempDirectory(name)` → `test/temp/<name>` (unused by current tests).
- `getSampleServersDirectory()` → `test/sampleServers`.
- `getCliPath()` → `test/../src/main-cli.ts` ← **SEAM #2**, used by `test/simple.test.ts` which does raw `spawn('node', [cliPath, '--help'])`.

`test/cli/utils.ts`:
- Re-exports `TestWorkspace`, `CommandResult`.
- `normalizeOutput(output)` — snapshot normalizer (only used by `help.test.ts`). Normalizes: CRLF→LF, trailing whitespace, uptime `\d+m \d+s|\d+s` → `<uptime>`, `PID: \d+`→`PID: <pid>`, `pid \d+`→`pid <pid>`, abs candle paths (`/Users/.../candle/`, `/home/.../candle/`, `C:\...\candle\`) → `<project>/`, `/tmp/...` → `<tmpdir>`, `CANDLE_DATABASE_DIR=...` → `=<dbdir>`.

`bin/test-candle.ts` ← **SEAM #3**: dev helper (not used by Vitest, but documented in CLAUDE.md). Parses `--database-dir <path>` → sets `CANDLE_DATABASE_DIR`, `--enable-logs` → `CANDLE_ENABLE_LOGS=true`, passes the rest to `node <repo>/src/main-cli.ts`. Prints captured stdout/stderr line-arrays and exits with the child's exit code.

## 4. Database isolation & direct DB access by tests

Isolation is purely via `CANDLE_DATABASE_DIR` → the DB file is `<dbDir>/candle.db`. `getStateDirectory()` (`src/dirs.ts`) resolution order: `CANDLE_DATABASE_DIR` → `XDG_STATE_HOME/candle` → `~/.local/state/candle`.

Several tests open the DB **directly** with Node's built-in `node:sqlite` `DatabaseSync` and run raw SQL — these hard-code the schema and must remain byte-compatible in the Rust impl:

- `check-start.test.ts` and `list.test.ts` insert a stale row:
  ```sql
  insert into processes (command_name, project_dir, pid, log_collector_pid, start_time, shell)
  values ('echo', '<dbDir>', 2147483000, 2147483001, strftime('%s','now'), 'node ...');
  ```
- `log-collector-cleanup.test.ts` reads:
  ```sql
  select log_collector_pid from processes where command_name = ? and killed_at is null
  ```
- `with-stdin/stdin.test.ts` imports `createStdinMessage` **directly from `src/database/stdinMessagesTable.ts`** (not via the CLI) and writes rows to `stdin_messages`. ← **SEAM #4**: this test bypasses the binary entirely and pokes the DB through library code. A Rust port must either expose an equivalent test helper or rewrite this test to insert via SQL/CLI.

### Exact DB schema (`src/database/database.ts`)
WAL mode + `busy_timeout=30000`. Migration behavior `safe-upgrades` (additive). Tables:

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
  root text
);
create table process_output(
  id integer primary key autoincrement,
  command_name text not null,
  project_dir text not null,
  content text,
  log_type integer not null,
  timestamp integer not null default (strftime('%s','now'))
);
create table process_last_cleanup( timestamp integer not null );
create table stdin_messages(
  id integer primary key autoincrement,
  command_name text not null,
  project_dir text not null,
  data text not null,
  encoding text not null default 'utf8',
  created_at integer not null default (strftime('%s','now'))
);
create index idx_process_output_command_name on process_output(command_name);
create index idx_process_output_project_dir on process_output(project_dir);
create index idx_process_output_lookup on process_output(project_dir, command_name, timestamp desc, id desc);
create index idx_stdin_messages_lookup on stdin_messages(project_dir, command_name, id);
```
`RunningStatus = { running:1, stopped:0 }`. Times are **unix seconds** (`Math.floor(Date.now()/1000)` / `strftime('%s','now')`).

### `ProcessLogType` enum (`src/logs/ProcessLogType.ts`) — `process_output.log_type` integer values
`stdout=1, stderr=2, process_start_initiated=3, process_start_failed=4, process_started=5, process_exited=6`.

### Stale-process cleanup semantics (tested by stale-cleanup/list/check-start)
A `processes` row with `killed_at IS NULL` is treated as **stale** (and deleted) iff **both** `log_collector_pid` and `pid` are dead (`process.kill(pid,0)` throws). Rows with `killed_at NOT NULL` are always deleted. Cleanup runs lazily: `maybeRunCleanup()` is a no-op unless >10 min (`CLEANUP_INTERVAL_SECONDS = 600`) since the `process_last_cleanup.timestamp`; it is invoked at CLI startup (`main-cli.ts`) before command dispatch, and every 60s inside the log-collector. Tests rely on stale detection happening on the next CLI invocation regardless of the 10-min gate via `filterAliveProcesses` in the start/check-start path (it deletes dead rows inline).

### Log eviction (`cleanup.ts`, `configFile.ts`)
Defaults `LOG_EVICTION_DEFAULTS = { maxLogsPerService: 1000, maxRetentionSeconds: 86400 }`. Config override via `.candle.json` `logEviction`. `cli-log-eviction` workspace sets `maxLogsPerService: 10`. `runCleanup`: delete `process_output` older than `now - maxRetentionSeconds`; delete stale processes; per `(project_dir,command_name)` keep newest `maxLogsPerService` rows (ordered `timestamp desc, id desc`); `vacuum`; upsert `process_last_cleanup`.

## 5. Sample servers (`test/sampleServers/*.js`) — log markers tests depend on

These are ESM/CJS Node scripts launched via `node <file>`. The Rust port keeps Node available for these (they are test fixtures, not part of candle). Key stdout markers asserted by tests:

| File | Behavior | Marker string(s) tested |
|---|---|---|
| `echoServer.js` | logs `Echo server started` + stderr line; every 1s logs `[stdout] Echo N: ...`; reads stdin | `Echo server started`, `Echo \d+:` |
| `testProcess.js` | logs `Test server started successfully`; every 2s a message; `process.stdin.resume()` keeps alive | `Test server started` |
| `delayedLogger.js` | `Server initializing...`, then `Server started` @2s, `Server ready to accept connections` @4s | `Server ready` |
| `delayedExitServer.js <code> <ms>` | `Delayed exit server running...` then exits with `<code>` after `<ms>` | `Delayed exit server running` |
| `markerServer.js <marker> <ms>` | logs `MARKER=<marker>` then exits after `<ms>` | `MARKER=...` |
| `periodicLogger.js` | `Starting periodic logger...`; `Log message N ...` every 500ms | `Starting periodic logger`, `Log message` |
| `quickExitServer.js` | `Quick exit server starting...`/`done.`; exit 0 | `Quick exit server` |
| `stdinEchoServer.js` | `Stdin echo server started`; echoes stdin as `[RECEIVED] <data>` | `Stdin echo server started`, `[RECEIVED] ...` |
| `errorServer.js` | logs to stderr, exit 1 | (used in `cli-logs` config) |
| `simpleServer.js`, `listeningServer.js` | HTTP servers (port detection / `web` service) | `Test server listening on port` |

Sample-server **config fixture** `test/sampleServers/.candle-setup.json` defines `web, api(root:test), echo, echo2, echo-test, test-format, delayed-logger` all relative to that dir (`node simpleServer.js` etc.). Shell commands in workspace configs use `node ../../sampleServers/<file>.js` (relative to `<dbDir>`, i.e. `test/workspaces/<name>/`).

## 6. Log-collector path resolution — CRITICAL SEAM #5

`src/log-collector/launchWithLogCollector.ts`:
- `launchNodeCollector`: `command = process.argv[0]` (the node binary), `args = [Path.join(ProjectRootDir, 'dist', 'main-log-collector.js')]`. `ProjectRootDir = dirname(dirs.ts)/..` = repo root. **So the spawned collector is the BUILT `dist/main-log-collector.js`** — this is why `pnpm test` runs `pnpm build` first. If `dist/` is stale or missing, every start/logs test fails.
- `getRustCollectorPath()`: returns `<repo>/rust/target/release/candle-log-collector` if it exists, else null.
- Selection: `options.logCollector === 'rust'` → rust path (throws if not built); otherwise node. The value comes from `.candle.json` `logCollector: 'node'|'rust'` resolved in `startOneService.ts` (lines 130–134). No test workspace currently sets `rust`, so all tests exercise the node collector by default.

Launch protocol: spawn detached with `stdio: ['pipe','pipe','pipe']`, `await waitForStart()`, then write `JSON.stringify(launchInfo)` to the collector's stdin and `end()` it. `LogCollectorLaunchInfo = { commandName, projectDir, shell, root?, enableStdin?, databasePath }`. `databasePath = Path.join(getStateDirectory(), 'candle.db')`.

Collector lifecycle (`main-log-collector.ts`): reads launch info from stdin JSON (or CLI flags `--commandName/--projectDir/--shell/--root/--enableStdin/--databasePath`), launches the monitored shell command (`shell:true`, cwd = `projectDir` joined with `root`), inserts a `processes` row with `log_collector_pid = process.pid`, waits for start, applies a **500ms grace period** (`DEFAULT_GRACE_PERIOD_WAIT_MS`) — if the child exited non-zero in that window it writes a `process_start_failed` log + deletes the row + exits 1. On success writes `process_started`; on exit writes `process_exited` (content `Process exited with code N`) and deletes the row. stdin feature: polls `stdin_messages` every 500ms (`STDIN_POLL_INTERVAL_MS`), pops oldest, writes `.data` to child stdin; clears stale messages on start.

→ **Rust impl: the candle binary must spawn the Rust log-collector binary directly** (not `node dist/...`). The default collector path and selection logic is the seam to flip.

## 7. Test files & coverage

CLI tests (`test/cli/`), each owns a named workspace:

| File | Workspace(s) | Covers |
|---|---|---|
| `add-service.test.ts` | `cli-add-service` | `add-service <name> --shell [--root]`; creates/updates `.candle.json`/`.candle-setup.json`; output contains `'name'` + `added`; errors on missing name/shell; exits <2s; success stderr empty |
| `check-start.test.ts` | `cli-check-start`, `cli-check-start-stale` | `check-start [names...]`: starts if not running (`Started`); skips if running (`already running`, not `Started`); **dead-PID stale row → starts anyway**; multiple names |
| `clear-logs.test.ts` | `cli-clear-logs` | `clear-logs [name]`: `Logs cleared successfully`; unknown → `No logs found to clear`; works on transient/running; no stderr |
| `erase-database.test.ts` | `cli-erase-database` | `erase-database`: clears state; idempotent; <2s; recognized command |
| `errors.test.ts` | `cli-errors` | unknown command → exit≠0 + `unrecognized`; unknown flags (strict) → `Unknown argument`; missing required args; timeouts; missing config → stderr contains `.candle`; unknown service → stderr contains the name / `No service`; `--root` without `--shell`; `--root` escaping → stderr contains `root`; errors→stderr, help→stdout |
| `get-doc.test.ts` | `cli-get-doc` | `get-doc <name>`: `getting-started` contains `Getting Started`; unknown doc fails; `transient-processes` contains `Transient` |
| `help.test.ts` | `cli-help` | `--help`/`help`/no-args: section headers `Process Management:`, `Port Detection:`, `Logs:`, `Configuration:`, `Documentation:`, `Troubleshooting & Maintenance:`, `Options:`; lists all commands; per-command `--help`; `help nonexistent` → `Unknown help topic`; uses `normalizeOutput` snapshot |
| `invalid-config.test.ts` | `invalid-config` | start of a service whose shell `node nonexistent-script.js` fails: stderr `Process 'invalid-startup-command' failed to start`, `Cannot find module`, `nonexistent-script.js`, `MODULE_NOT_FOUND`; logs capture same; MCP `StartService` returns `isError` + `failed to start`; MCP `GetLogs` shows error |
| `kill-all.test.ts` | `cli-kill-all` | `kill-all`: kills config+transient; no-op when none; <5s; clears list; no stderr |
| `kill.test.ts` | `cli-kill` | `kill [names...]`: `Killed`+name; non-running known service → `No running processes` (exit 0); unknown → exit 1 + stderr `No service`+name; multiple names; mixed running/non-running |
| `list-all.test.ts` | `cli-list-all` | `list-all` works without local config (from `$HOME`); shows project dir; headers `NAME`/`STATUS` |
| `list-docs.test.ts` | `cli-list-docs` | `list-docs`: non-empty stdout, <2s, no stderr |
| `list.test.ts` | `cli-list`, `cli-list-fresh`, `cli-list-stale` | `list`/`ls`: headers `NAME STATUS UPTIME`; `RUNNING`; uptime regex `\d+s|\d+m`; `[config changed]` for transient shadowing config; stale dead-PID row NOT shown as RUNNING; killed not RUNNING |
| `log-collector-cleanup.test.ts` | `cli-log-collector-cleanup` | log-collector PID (from DB) alive after start; **dead within 5s after `kill`** |
| `log-eviction.test.ts` | `cli-log-eviction` | accepts `logEviction` config; eviction indicator (`older logs have been removed` absence when small); cleanup respects `maxLogsPerService:10` |
| `logs.test.ts` | `cli-logs` | `logs [name]`: shows content; transient; historical after kill; unknown → `No logs found`; no-name shows running; only most-recent launch (marker filtering); `--count N`; `--start-at <id>` (high id → `No logs found`); `--bogus-flag` → `Unknown argument`; `Started` + `'name'` start message; `--shell` with multiple names → stderr `Exactly one service name is required when using --shell` |
| `remove-service.test.ts` | `cli-remove-service` | `remove-service <name>`: removes from config, output `removed`; not-found → exit1 + name in stderr; missing name; no config → fail; preserves `logEviction` |
| `restart.test.ts` | `cli-restart`, `cli-restart-reload` | `restart [name]`: `Started`; starts if not running; transient reuses DB shell; restart-all (no name); no running → exit1 + `No running processes`; unknown → `No service`; ≤1 `Killed` message; **reloads edited shell from config on restart** (marker-v1→v2) |
| `setup-project.test.ts` | `cli-setup-project` | `setup-project`: creates `.candle.json` = `{services:[]}`; output `Created`+`.candle.json`; existing → `already exists`, not overwritten; parent-dir config → `already exists`; <2s |
| `stale-cleanup.test.ts` | `stale-cleanup` | externally SIGKILL a started service's PID → next invocation removes stale entry (exactly 1 RUNNING after restart); live process not removed; transient stale removed after `kill-all` |
| `start.test.ts` | `cli-start`, `cli-start-empty` | `start [names...]`/config services; multiple; all (no name); unknown → fail; `--shell` transient; `--shell --root`; `--root` without `--shell` fail; `--root` escape fail (`root`); transient shadows config; restart-on-rerun; no config (`/tmp`) → stderr `.candle.json`; empty config → stderr `No services configured` |
| `version.test.ts` | `cli-version` | `--version`/`-v`: matches `\d+.\d+.\d+`, single line, no stderr, equals `package.json` version |
| `wait-for-log.test.ts` | `cli-wait-for-log` | `wait-for-log <name> --message <m> [--timeout <s>]`: waits; returns fast if present; requires `--message`; partial+case-sensitive match; default timeout 30; custom timeout; timeout failure (exit1, output matches `/timeout|not found|failed/`) |
| `watch.test.ts` | `cli-watch` | `watch <name> --exit-after-ms <ms>` (hidden flag): header `Watching process 'echo'`; prints existing logs `Echo server started`; streams live `Echo \d+:` |

Root-level tests:
- `test/simple.test.ts` — raw `spawn('node',[getCliPath(),'--help'])`; asserts `Process Management:`, `run`, `kill`, exit 0.
- `test/mcp.test.ts` (`mcp` workspace) — MCP tools list `[ListServices, ListPorts, GetLogs, StartService, StartTransientService, KillService, RestartService, AddServerConfig]`; `StartService{name}` → `Started` + shell echoed; `ListServices` returns JSON `{processes:[{serviceName,status:'RUNNING',...}]}`; `StartTransientService{name,shell[,root]}`; missing `name`/`shell` → `isError`; `RestartService` matches `/Started|Restarted/`; `GetLogs{name,limit}`; `showAll` param.
- `test/transient-processes.test.ts` (`transient-processes`) — transient start/kill/restart/logs; `--root`; validation; name-collision (transient shadows config, kills prior same-name); config-drift `[config changed]`; DB stores shell.
- `test/with-stdin/stdin.test.ts` (`with-stdin`) — config `enableStdin:true` and `--enable-stdin`; sends via `createStdinMessage` directly into DB; expects `[RECEIVED] ...` in logs; no-enableStdin → not received.
- `test/list-format/list-format.test.ts` (`list-format`) — exact column order `NAME, STATUS, PID, UPTIME, COMMAND, DIRECTORY` (header index ordering asserted); old headers `LAUNCH_ID`/`WRAPPER_PID` absent; status `RUNNING|STOPPED`.

## 8. Subtle / platform-specific behaviors (easy to get wrong in Rust)

1. **`CLAUDECODE=''` is set by the harness** → `isRunByAgent = !!process.env.CLAUDECODE` becomes `false` (empty string is falsy). When agent mode is on, the `watch` command is hidden from help and disabled. Rust must replicate: empty string ⇒ not-agent; the help text and `watch` availability must match. (`watch.test.ts` and `help.test.ts` depend on watch being present.)
2. **`FORCE_COLOR=0`** disables ANSI; Rust output must contain no escape codes when this is set (tests do raw substring matching on `RUNNING`, `Started`, etc.).
3. Exact output strings are load-bearing — copy verbatim:
   - Start: `[Started process '<name>' (\`<shell>\`) in directory: '<dir>']` (must contain `Started` and `'<name>'`).
   - check-start skip: `[Service '<name>' is already running]` (contains `already running`).
   - Errors: `No service '<name>' configured for directory: <cwd>`; `No .candle.json file found in (or above) current directory: <cwd>`; `Process '<name>' failed to start. Recent logs: ...`; `No services configured in .candle.json`; `Exactly one service name is required when using --shell`; `Unrecognized command '<cmd>'`.
   - Unknown flags must yield yargs-style `Unknown argument` (strict mode). This is **yargs `.strictOptions()`** behavior — Rust arg parser (e.g. clap) must reject unknown flags and emit a message containing `Unknown argument`.
4. **Timeouts are in seconds on the CLI** (`--timeout 30`) but converted to ms internally (`timeout*1000`). Negative/zero handled as immediate failure.
5. Stale detection uses signal-0 liveness (`process.kill(pid,0)`); fake PID `2147483000` is chosen to be unused. Rust must use `kill(pid,0)`/equivalent and treat both `pid` and `log_collector_pid` deadness as the staleness condition.
6. Multiple-launch log filtering: `logs` shows only the **most recent execution** (filtered via `LatestExecutionLogFilter` keyed off the last `process_start_initiated`/`process_started`). Markers from earlier runs must not appear.
7. `list-all` must work from any cwd (no config required); `list`/`start`/`kill`/`logs` require/resolve a config via upward search.
8. Times stored as unix seconds; uptime formatting matches `\d+m \d+s` or `\d+s`.
9. The `processes` row is created **by the log collector** (not the CLI), so `start` only succeeds after the collector writes a `process_started` log (10s wait race in `startOneService`). The DB write ordering matters for `list`/`log-collector-cleanup` tests.
10. Config file lookup order: `.candle.json` then `.candle-setup.json` (deprecated), searching upward to filesystem root. Empty config file ⇒ `{services:[]}`.

## 9. Test fixtures (committed `.candle.json` per workspace)

All shells reference `node ../../sampleServers/<x>.js` (relative to the workspace dir). Notable: `cli-log-eviction` adds `"logEviction": {"maxLogsPerService": 10}`; `with-stdin` has `{name:"stdin-echo", shell:"node ../../sampleServers/stdinEchoServer.js", enableStdin:true}`; `invalid-config` has `{name:"invalid-startup-command", shell:"node nonexistent-script.js"}`; `cli-start-empty` has `{services:[]}`. The `web` service everywhere maps to `node ../../sampleServers/testProcess.js` (not a real web server). `getting-started`/`transient-processes` docs come from `<repo>/docs` + `README.md` via `DocFilesHelper`.

## 10. External npm dependencies & Rust crate equivalents

| npm dep | Used for | Rust equivalent |
|---|---|---|
| `@facetlayer/subprocess` (`runShellCommand`, `startShellCommand`, `Subprocess`, `SubprocessResult`) | spawning CLI/collector/services; line-buffered stdout/stderr capture; `detached`; `waitForStart/waitForExit` | `std::process` / `tokio::process`; line splitting via `BufReader`; `nix::unistd::setsid` for detach |
| `@facetlayer/sqlite-wrapper` (`DatabaseLoader`, `SqliteDatabase`, `.insert/.upsert/.list/.get/.run`) + WAL/busy_timeout + `migrationBehavior:'safe-upgrades'` | all DB access | `rusqlite` (+ `r2d2_sqlite` if pooling); WAL pragma + busy_timeout; hand-rolled additive migrations |
| `node:sqlite` `DatabaseSync` | **test-side** raw SQL | (test harness only) `rusqlite` in tests |
| `yargs` + `yargs/helpers` (`.command`, `.strictOptions`, `.demandOption`, aliases) | CLI parsing, strict unknown-flag rejection, `Unknown argument` message | `clap` (must emulate yargs error text containing `Unknown argument`) |
| `@modelcontextprotocol/sdk` `1.12.1` | MCP server (`--mcp`) | `rmcp` (official Rust MCP SDK) or hand-rolled stdio JSON-RPC |
| `@facetlayer/parse-stdout-lines` | line parsing of child output | manual line splitter |
| `expect-mcp` (`mcpShell`, `MCPStdinSubprocess`, vitest matchers) | **test-side** MCP client | keep tests in TS, or port to a Rust MCP test client |
| `vitest`, `tsx`, `prettier`, `typescript` | test/build tooling | `cargo test` / keep Vitest as black-box harness pointing at the Rust binary |

**Recommendation:** keep the Vitest suite as a black-box conformance harness and only change `TestWorkspace.cliPath` + spawn form (drop `node`) to point at the compiled Rust binary; this preserves all ~25 test files unchanged except the two seams that import `src/` directly (`with-stdin` → `createStdinMessage`, and any reliance on `src/main-cli.ts`).

## 11. Every seam referencing the Node implementation (must point at Rust)

1. `test/TestWorkspace.ts:43` — `cliPath = src/main-cli.ts`; spawned as `runShellCommand('node', [cliPath, ...])` (runCli, line 72; cleanup, line 127). → Rust: `cliPath = <rust candle binary>`, spawn directly without `node`.
2. `test/TestWorkspace.ts:91` — `mcpShell(\`node ${cliPath} --mcp\`)`. → `mcpShell(\`<rust-binary> --mcp\`)`.
3. `test/utils.ts:17` `getCliPath()` → `src/main-cli.ts`, consumed by `test/simple.test.ts` via `spawn('node', [cliPath, '--help'])`. → point at Rust binary, spawn directly.
4. `bin/test-candle.ts:58,70` — dev wrapper spawns `node src/main-cli.ts`. → spawn Rust binary.
5. `src/log-collector/launchWithLogCollector.ts:39-41` — node collector path `dist/main-log-collector.js` spawned via `process.argv[0]`. → Rust candle must spawn the Rust collector binary (default), removing the `pnpm build` precondition for the node collector.
6. `src/start/startOneService.ts:130-147` & `configFile.ts` — `logCollector: 'node'|'rust'` selection. → default to rust; keep/repurpose the config switch.
7. `test/with-stdin/stdin.test.ts:3` — `import { createStdinMessage } from '../../src/database/stdinMessagesTable'` writes directly to the DB. → replace with raw SQL insert or a Rust test helper.
8. `test/cli/version.test.ts` reads `../../package.json`. → Rust version must equal the published package version (source of truth currently `package.json`, e.g. `0.13.3`).
9. Direct DB schema assumptions in `check-start`/`list`/`log-collector-cleanup` tests (table `processes`, columns, `strftime('%s','now')`, `killed_at is null`). → Rust DB layer must produce byte-identical schema.
10. `pnpm test` = `pnpm build && vitest run` — the build exists solely to produce `dist/main-log-collector.js`; for Rust, the test script must instead `cargo build --release` (produce `rust/target/release/candle-log-collector` and the candle binary) before `vitest run`.

## 12. Rust reimplementation notes (modules/functions + ordering)

Build in this dependency order:

1. **`dirs`** — `get_state_directory()` (env `CANDLE_DATABASE_DIR` → `XDG_STATE_HOME/candle` → `~/.local/state/candle`); `project_root_dir()`. No deps.
2. **`db`** — `get_database(override_dir)`: create dir, open `candle.db`, apply exact schema (§4), set `journal_mode=WAL`, `busy_timeout=30000`, additive migrations. Define `RunningStatus`, `ProcessLogType` enums. Depends on dirs.
3. **`db::process_table`** — `ProcessEntry` (note TS field alias: interface calls it `launch_id` but the column is `id`; DB column is `id`), `create/update_killed_at/delete/find_*` (queries verbatim from `processTable.ts`).
4. **`db::process_output` / `logs`** — `save_process_log`, log iteration, `LatestExecutionLogFilter` (most-recent-launch filtering), `get_process_logs(limit default 100, start_at_id)`.
5. **`db::stdin_messages`** — `create/pop/clear_stdin_message`.
6. **`config`** — `find_config_file` (upward search, `.candle.json` then `.candle-setup.json`), `read/validate_config`, `find_project_dir`, `get_service_config_by_name` (+ loose matching), `get_log_eviction_config` (defaults 1000 / 86400), `get_service_info_by_name`. Error types: `UsageError`, `MissingServiceWithNameError`, `MissingSetupFileError`, `ConfigFileError`, `ProcessStartFailedError` with exact messages.
7. **`process_alive`** — `is_process_alive(pid)` via `kill(pid,0)`; `filter_alive_processes` (deletes dead rows). 
8. **`db::cleanup` + `stale_process_cleanup`** — `maybe_run_cleanup` (600s gate), `run_cleanup`, `cleanup_stale_processes`.
9. **log-collector binary** (`candle-log-collector`) — read launch info from stdin JSON or flags; spawn monitored shell (`shell:true`, cwd=projectDir/root); 500ms grace period; write process_started/start_failed/exited logs; stdin polling (500ms); 60s cleanup interval. This is the unit the main binary spawns.
10. **`start::start_one_service` + `log_collector::launch`** — kill existing, save `process_start_initiated`, select node/rust collector (default rust), launch detached + write launch JSON to stdin, wait up to 10s for `process_started`/`process_start_failed`. Print the exact start banner.
11. **command handlers** — `start/check-start`, `kill/kill-all`, `restart` (reloads shell from config), `list/list-all` (columns `NAME STATUS PID UPTIME COMMAND DIRECTORY`, `[config changed]`), `logs` (`--count`/`--start-at`), `watch` (`--exit-after-ms`, gated by agent mode), `wait-for-log` (`--message`/`--timeout` seconds), `clear-logs`, `erase-database`, `setup-project`, `add-service`/`remove-service`, `set-config`, `list-docs`/`get-doc`, `list-ports`/`open-browser`.
12. **`main-cli`** — clap parser matching yargs commands/aliases/strictness (emit `Unknown argument`), grouped `--help` text (exact section headers), `--version` (= package version), `mcp`/`--mcp`, `isRunByAgent` from `CLAUDECODE` truthiness, `maybe_run_cleanup()` at startup.
13. **`mcp`** — tools `ListServices, ListPorts, GetLogs, StartService, StartTransientService, KillService, RestartService, AddServerConfig`; JSON `{processes:[{serviceName,status,...}]}`; errors as `isError` text content.

Harness changes to land first (so the existing Vitest suite can validate the Rust binary): repoint seams §11.1–4 to the Rust binary path and drop the `node` prefix; make the spawned collector the Rust collector (§11.5–6); replace the `createStdinMessage` import in `with-stdin` (§11.7) with raw SQL; change `pnpm test` to `cargo build --release && vitest run`.