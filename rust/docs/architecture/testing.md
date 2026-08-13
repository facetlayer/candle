# Testing

> **Note (post-Node-removal):** The Node.js implementation has been deleted. The dual-target harness
> described below is now single-target — the suite always runs against the compiled Rust binary. The
> `test:node` script, the `CANDLE_TEST_TARGET=node` branch, the Node log collector, and the CI `tests`
> job no longer exist. The sections below are retained as a historical description of how the harness
> straddled both implementations during the port.

The acceptance test suite is a black-box conformance harness: a single Vitest suite that spawns the candle CLI as a subprocess and asserts on its behavior. It runs through one seam — `getCandleSpawn()` in [`test/TestWorkspace.ts`](../../../test/TestWorkspace.ts) — which spawns the compiled Rust binary. (Historically the seam also honored `CANDLE_TEST_TARGET=node` to run the Node implementation; that target has been removed.) Most tests are unchanged from the Node era.

## 1. Test runner mechanics

The suite is **Vitest 3** (`vitest.config.ts`), TypeScript, run from the repo root.

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
- `test`: **`cargo build --release --manifest-path rust/Cargo.toml && CANDLE_TEST_TARGET=rust vitest run`** — builds the Rust release binary, then runs the suite against the Rust target.
- `test:rust`: identical to `test`.
- `test:node`: **`pnpm build && CANDLE_TEST_TARGET=node vitest run`** — builds the Node `dist/` (needed for the Node log-collector, see §6), then runs the suite against the still-published Node implementation.
- `test:watch`: `vitest`.

### Target selection — the central seam
`CANDLE_TEST_TARGET` selects how the candle CLI under test is spawned. Unset defaults to `rust`.

| Target | What is spawned | Spawn form |
|---|---|---|
| `rust` (default) | `rust/target/release/candle` | direct exec (no `node`) |
| `node` | `src/main-cli.ts` | `node <path> <args>` |

`getCandleSpawn()` returns `{ cmd, baseArgs, mcpCommand }`:
- Rust: `cmd = <repo>/rust/target/release/candle`, `baseArgs = []`, `mcpCommand = "<bin> --mcp"`.
- Node: `cmd = "node"`, `baseArgs = ["<repo>/src/main-cli.ts"]`, `mcpCommand = "node <cli> --mcp"`. Node's native TypeScript type-stripping runs the `.ts` entrypoint directly; there is no compile step for the Node CLI itself, only for its log-collector.

This is the single seam that points the whole Vitest suite at either implementation; every spawn in `TestWorkspace` flows through it.

### What gets spawned, by target

| What | Rust target | Node target |
|---|---|---|
| The CLI under test | `rust/target/release/candle <args>` | `node src/main-cli.ts <args>` |
| MCP server under test | `<rust candle> --mcp` | `node src/main-cli.ts --mcp` |
| The per-service monitor (spawned by the CLI) | `rust/target/release/candle --monitor` (stdin = JSON) | `node dist/main-log-collector.js` (stdin = JSON) |

## 2. `TestWorkspace` helper (`test/TestWorkspace.ts`)

This is the primary harness used by nearly every test. Re-exported from `test/cli/utils.ts`.

Constructor: `new TestWorkspace(name: string)`
- `this.dbDir = path.join(__dirname, 'workspaces', name)` → `test/workspaces/<name>`. Used as **both** the cwd and the SQLite database directory.
- Creates `dbDir` if missing (`mkdirSync recursive`).

Each workspace ships a committed `.candle.json` (see §9). DB files (`candle.db`, `-shm`, `-wal`) live in the same dir and are git-ignored (`*.db*`), but the directory tree and `.candle.json` are force-committed despite `.gitignore` having `test/workspaces/`.

### `runCli(args, options): Promise<SubprocessResult>`
```ts
const cwd = this.dbDir;
const env = {
    ...process.env,
    CANDLE_DATABASE_DIR: cwd,   // ← DB isolation
    FORCE_COLOR: '0',           // ← disable ANSI color in output
    CLAUDECODE: '',             // ← force isRunByAgent=false (see §8)
    ...(options.env || {}),
};
const { cmd, baseArgs } = getCandleSpawn();
const result = await runShellCommand(cmd, [...baseArgs, ...args], {
    cwd: options.cwd ?? cwd, env,
});
if (result.failed() && !options.ignoreExitCode) throw result.asError();
return result;
```
`CliOptions`: `{ cwd?, env?, ignoreExitCode? }`. Default behavior **throws on non-zero exit**; tests that expect failure pass `ignoreExitCode: true` and assert `result.failed() === true`.

The returned `SubprocessResult` (from `@facetlayer/subprocess`) exposes: `exitCode: number|null`, `failed(): boolean`, `asError(): Error`, `stdoutAsString(): string`, `stderrAsString(): string`. stdout/stderr are stored internally as arrays of lines and joined.

### `createMcpApp(options?): MCPStdinSubprocess`
```ts
return mcpShell(getCandleSpawn().mcpCommand, {
    allowDebugLogging,
    cwd: this.dbDir,
    env: { ...process.env, CANDLE_DATABASE_DIR: this.dbDir, CLAUDECODE: '' },
});
```
Note: the MCP env does **not** set `FORCE_COLOR`. `mcpShell` comes from `expect-mcp` and drives a stdio MCP client.

### `ensureSubdir(name)` — mkdir a subdir inside the workspace (for `--root` tests).
### `cleanup()` — runs `kill-all` (via `getCandleSpawn()`) with `CANDLE_DATABASE_DIR=dbDir`, swallows errors. **Does NOT delete the DB** — only kills processes. Called in `afterAll`.

## 3. Other test utilities

[`test/utils.ts`](../../../test/utils.ts):
- `getTestTempDirectory(name)` → `test/temp/<name>` (unused by current tests).
- `getSampleServersDirectory()` → `test/sampleServers`.
- `getCliPath()` → `src/main-cli.ts` (the Node entrypoint), used by `test/simple.test.ts`, which does a raw `spawn('node', [cliPath, '--help'])`. This one test is hardwired to the Node CLI rather than going through `getCandleSpawn()`.

`test/cli/utils.ts`:
- Re-exports `TestWorkspace`, `CommandResult`.
- `normalizeOutput(output)` — snapshot normalizer (only used by `help.test.ts`). Normalizes: CRLF→LF, trailing whitespace, uptime `\d+m \d+s|\d+s` → `<uptime>`, `PID: \d+`→`PID: <pid>`, `pid \d+`→`pid <pid>`, abs candle paths (`/Users/.../candle/`, `/home/.../candle/`, `C:\...\candle\`) → `<project>/`, `/tmp/...` → `<tmpdir>`, `CANDLE_DATABASE_DIR=...` → `=<dbdir>`.

`bin/test-candle.ts` — dev helper (not used by Vitest, but documented in `CLAUDE.md`). Parses `--database-dir <path>` → sets `CANDLE_DATABASE_DIR`, `--enable-logs` → `CANDLE_ENABLE_LOGS=true`, passes the rest through to candle. Prints captured stdout/stderr line-arrays and exits with the child's exit code.

## 4. Database isolation & direct DB access by tests

Isolation is purely via `CANDLE_DATABASE_DIR` → the DB file is `<dbDir>/candle.db`. The state-directory resolution order is: `CANDLE_DATABASE_DIR` → `XDG_STATE_HOME/candle` → `~/.local/state/candle`.

Several tests open the DB **directly** with Node's built-in `node:sqlite` `DatabaseSync` and run raw SQL. These hard-code the schema and rely on the Rust DB layer producing a byte-compatible schema:

- `check-start.test.ts` and `list.test.ts` insert a stale row:
  ```sql
  insert into processes (command_name, project_dir, pid, log_collector_pid, start_time, shell)
  values ('echo', '<dbDir>', 2147483000, 2147483001, strftime('%s','now'), 'node ...');
  ```
- `monitor-cleanup.test.ts` reads:
  ```sql
  select log_collector_pid from processes where command_name = ? and killed_at is null
  ```
- `with-stdin/stdin.test.ts` imports `createStdinMessage` **directly from `src/database/stdinMessagesTable.ts`** (not via the CLI) and writes rows to `stdin_messages`. This test bypasses the binary entirely and pokes the DB through Node library code, so it asserts on schema/behavior shared by both implementations.

### Exact DB schema
WAL mode + `busy_timeout=30000`. Migration behavior is additive (`safe-upgrades`). Tables:

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

### `ProcessLogType` enum — `process_output.log_type` integer values
`stdout=1, stderr=2, process_start_initiated=3, process_start_failed=4, process_started=5, process_exited=6`.

### Stale-process cleanup semantics (tested by stale-cleanup/list/check-start)
A `processes` row with `killed_at IS NULL` is treated as **stale** (and deleted) iff **both** `log_collector_pid` and `pid` are dead (signal-0 liveness check fails). Rows with `killed_at NOT NULL` are always deleted. Cleanup runs lazily: `maybe_run_cleanup()` is a no-op unless more than 10 min (`CLEANUP_INTERVAL_SECONDS = 600`) have passed since the `process_last_cleanup.timestamp`; it is invoked at CLI startup before command dispatch, and every 60s inside the log-collector. Tests rely on stale detection happening on the next CLI invocation regardless of the 10-min gate via the alive-process filter in the start/check-start path (it deletes dead rows inline).

### Log eviction
Defaults `LOG_EVICTION_DEFAULTS = { maxLogsPerService: 1000, maxRetentionSeconds: 86400 }`. Config override via `.candle.json` `logEviction`. The `cli-log-eviction` workspace sets `maxLogsPerService: 10`. `run_cleanup`: delete `process_output` older than `now - maxRetentionSeconds`; delete stale processes; per `(project_dir, command_name)` keep the newest `maxLogsPerService` rows (ordered `timestamp desc, id desc`); `vacuum`; upsert `process_last_cleanup`.

## 5. Sample servers (`test/sampleServers/*.js`) — log markers tests depend on

These are ESM/CJS Node scripts launched via `node <file>`. They are test fixtures, not part of candle, so Node remains available to run them regardless of the target. Key stdout markers asserted by tests:

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

## 6. Monitor process resolution

The candle CLI spawns a separate monitor process to supervise each service. It is the **same binary**, re-invoked as `candle --monitor` via `std::env::current_exe()`, so there is nothing extra to build or locate — `cargo build --release` produces everything the suite needs. (`CANDLE_MONITOR_PATH` overrides the path if a test needs to point at a different build.)

Launch protocol: spawn detached (`setsid`) with stdin piped and stdout/stderr null, then write the launch-info JSON to the monitor's stdin and close it. `MonitorLaunchInfo = { commandName, projectDir, shell, root?, enableStdin?, databasePath }`. `databasePath = <stateDirectory>/candle.db`.

Monitor lifecycle: reads launch info from stdin JSON (or flags `--command-name/--project-dir/--shell/--root/--enable-stdin/--database-path`), launches the monitored shell command (`shell:true`, cwd = `projectDir` joined with `root`), inserts a `processes` row with `log_collector_pid` = the monitor's own pid, waits for start, applies a **500ms grace period** (`DEFAULT_GRACE_PERIOD_WAIT_MS`) — if the child exited non-zero in that window it writes a `process_start_failed` log + deletes the row + exits 1. On success it writes `process_started`; on exit it writes `process_exited` (content `Process exited with code N`) and deletes the row. stdin feature: polls `stdin_messages` every 500ms (`STDIN_POLL_INTERVAL_MS`), pops oldest, writes `.data` to child stdin; clears stale messages on start.

The `processes` row is created **by the monitor** (not the CLI), so `start` only succeeds after the monitor writes a `process_started` log (a 10s wait race in `start_one_service`). The DB write ordering matters for the `list`/`monitor-cleanup` tests.

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
| `monitor-cleanup.test.ts` | `cli-monitor-cleanup` | monitor PID (from DB `log_collector_pid`) alive after start; **dead within 5s after `kill`** |
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

## 8. Subtle / platform-specific behaviors

1. **`CLAUDECODE=''` is set by the harness** → `isRunByAgent = !!process.env.CLAUDECODE` becomes `false` (empty string is falsy). When agent mode is on, the `watch` command is hidden from help and disabled. Both implementations replicate this: empty string ⇒ not-agent; the help text and `watch` availability must match (`watch.test.ts` and `help.test.ts` depend on watch being present).
2. **`FORCE_COLOR=0`** disables ANSI; output must contain no escape codes when this is set (tests do raw substring matching on `RUNNING`, `Started`, etc.).
3. Exact output strings are load-bearing — preserved verbatim across implementations:
   - Start: `[Started process '<name>' (\`<shell>\`) in directory: '<dir>']` (must contain `Started` and `'<name>'`).
   - check-start skip: `[Service '<name>' is already running]` (contains `already running`).
   - Errors: `No service '<name>' configured for directory: <cwd>`; `No .candle.json file found in (or above) current directory: <cwd>`; `Process '<name>' failed to start. Recent logs: ...`; `No services configured in .candle.json`; `Exactly one service name is required when using --shell`; `Unrecognized command '<cmd>'`.
   - Unknown flags yield yargs-style `Unknown argument` (strict mode). The Node CLI uses yargs `.strictOptions()`; the Rust CLI (clap) is configured to reject unknown flags and emit a message containing `Unknown argument`.
4. **Timeouts are in seconds on the CLI** (`--timeout 30`) but converted to ms internally (`timeout*1000`). Negative/zero handled as immediate failure.
5. Stale detection uses signal-0 liveness; fake PID `2147483000` is chosen to be unused. Both `pid` and `log_collector_pid` deadness form the staleness condition.
6. Multiple-launch log filtering: `logs` shows only the **most recent execution** (filtered via the latest-execution filter keyed off the last `process_start_initiated`/`process_started`). Markers from earlier runs must not appear.
7. `list-all` must work from any cwd (no config required); `list`/`start`/`kill`/`logs` require/resolve a config via upward search.
8. Times stored as unix seconds; uptime formatting matches `\d+m \d+s` or `\d+s`.
9. Config file lookup order: `.candle.json` then `.candle-setup.json` (deprecated), searching upward to filesystem root. Empty config file ⇒ `{services:[]}`.

## 9. Test fixtures (committed `.candle.json` per workspace)

All shells reference `node ../../sampleServers/<x>.js` (relative to the workspace dir). Notable: `cli-log-eviction` adds `"logEviction": {"maxLogsPerService": 10}`; `with-stdin` has `{name:"stdin-echo", shell:"node ../../sampleServers/stdinEchoServer.js", enableStdin:true}`; `invalid-config` has `{name:"invalid-startup-command", shell:"node nonexistent-script.js"}`; `cli-start-empty` has `{services:[]}`. The `web` service everywhere maps to `node ../../sampleServers/testProcess.js` (not a real web server). `getting-started`/`transient-processes` docs come from `<repo>/docs` + `README.md` via the doc-files helper.

## 10. Dependencies

Test-side tooling:

| Dependency | Used for |
|---|---|
| `@facetlayer/subprocess` (`runShellCommand`, `SubprocessResult`) | spawning the CLI under test; line-buffered stdout/stderr capture; exit handling |
| `node:sqlite` `DatabaseSync` | test-side raw SQL against the shared schema (§4) |
| `expect-mcp` (`mcpShell`, `MCPStdinSubprocess`, vitest matchers) | test-side MCP client driving the candle MCP server over stdio |
| `vitest` | the black-box conformance harness (points at either target via `getCandleSpawn()`) |

The Rust binary is exercised by this same Vitest suite; `cargo test` covers the Rust unit tests in addition. Keeping the Vitest suite as a single black-box harness means nearly all ~25 test files run unchanged against both targets; the only test that imports `src/` directly is `with-stdin` (`createStdinMessage`).

## 11. CI

`.github/workflows/ci.yml` runs two jobs:

- **`tests`** — Node target: `pnpm test:node` (`pnpm build` then Vitest with `CANDLE_TEST_TARGET=node`), validating the still-published Node implementation.
- **`rust`** — Rust target: installs the Rust toolchain (with clippy), `cargo build --release`, `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, then the acceptance suite at the Rust target (`CANDLE_TEST_TARGET=rust pnpm exec vitest run`).
