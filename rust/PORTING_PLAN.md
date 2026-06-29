# Candle → Rust Port Plan

Goal: reimplement the entire `candle` CLI (currently Node/TypeScript in `../src`) in Rust under
`rust/`, as a **drop-in replacement**. The existing TypeScript/Vitest suite in `../test` is kept as
the black-box acceptance suite and is repointed at the Rust binaries. The Node implementation is
**kept** (not deleted) during the transition.

## Hard requirements (from product owner)

1. The log-collector sidecar lives in a binary named **`log-collector`** (not the default/primary
   binary). The primary binary is **`candle`** and implements everything else.
2. There is **NO** `logCollector: node|rust` feature flag in the Rust world. The Rust `candle`
   binary **always** spawns the Rust `log-collector` sidecar.
3. The Vitest suite is kept and must run against the Rust binaries.
4. Keep the Node implementation (`../src`) for now.

## Target Rust layout

Cargo **workspace** with one shared library crate and two binary crates (keeps the two thin `main.rs`
files sharing all DB/config/log logic, and lets unit tests live in the lib):

```
rust/
  Cargo.toml                # [workspace] members = candle-core, candle-cli, log-collector
  candle-core/              # lib crate — all shared subsystems (dirs, errors, db, config, logs,
    src/lib.rs              #   process, kill, start, log_collector::monitor, commands, mcp, output)
  candle-cli/               # bin crate → produces the `candle` binary (clap dispatch)
    src/main.rs
  log-collector/            # bin crate → produces the `log-collector` binary (NOT candle-log-collector)
    src/main.rs             #   thin: read launch info, call candle_core::log_collector::monitor::run
```

`candle-core` modules mirror the Node subsystems: `dirs`, `errors`, `debug`, `run_context`,
`db/{mod,process_table,stdin_messages,cleanup,stale_process_cleanup}`,
`config/{model,paths,validate,file,commands}`,
`logs/{log_type,models,sql_builder,build_query,process_logs,log_iterator}`,
`log_filters/{latest_execution,execution_status}`, `console_log`,
`process_alive`, `process_tree`, `kill/*`, `subprocess`,
`log_collector/{monitor,launch}`, `start/*`, `commands/*`, `doc_files`, `mcp/*`,
and **`output`** (an output-sink abstraction: handlers write through it so the MCP server can
capture their output instead of touching real stdout — avoids console monkeypatching).

Crates: `rusqlite` (bundled), `serde`/`serde_json` (with `preserve_order` → byte-identical config
write-back via `indexmap`), `clap`, `nix` (kill/setsid) + `libc`, `regex` (doc frontmatter),
`indexmap`, `thiserror`/`anyhow`. MCP: hand-rolled stdio JSON-RPC (decide vs `rmcp` at M8).

**Binary rename:** the sidecar binary is `log-collector` (was `candle-log-collector`). The CLI
resolves it as a **sibling of the `candle` binary** via `std::env::current_exe()` (both land in
`rust/target/release/`); `CANDLE_LOG_COLLECTOR_PATH` overrides. The `.candle.json` `logCollector` key
is still parsed/validated (config tests assert its validation messages) but **ignored at launch** —
the Rust CLI always spawns the Rust sidecar.

**Sequencing note:** land the test-harness toggle (M1) immediately after the M0 scaffold so every
later milestone can run the real Vitest suite against the Rust binary as it lands. Foundation modules
are verified with `cargo test` (ported from the TS `__tests__`) until enough commands exist.

## Invariants that MUST match byte-for-byte (tests depend on these)

### SQLite schema (file: `<stateDir>/candle.db`, WAL, `busy_timeout=30000`)
```sql
create table processes(
  id integer primary key autoincrement,
  command_name text not null, project_dir text not null,
  pid integer not null, log_collector_pid integer,
  start_time integer not null,
  created_at integer not null default (strftime('%s','now')),
  killed_at integer, shell text, root text);
create table process_output(
  id integer primary key autoincrement,
  command_name text not null, project_dir text not null,
  content text, log_type integer not null,
  timestamp integer not null default (strftime('%s','now')));
create table process_last_cleanup(timestamp integer not null);
create table stdin_messages(
  id integer primary key autoincrement,
  command_name text not null, project_dir text not null,
  data text not null, encoding text not null default 'utf8',
  created_at integer not null default (strftime('%s','now')));
create index idx_process_output_command_name on process_output(command_name);
create index idx_process_output_project_dir on process_output(project_dir);
create index idx_process_output_lookup on process_output(project_dir, command_name, timestamp desc, id desc);
create index idx_stdin_messages_lookup on stdin_messages(project_dir, command_name, id);
```
`ProcessLogType`: stdout=1, stderr=2, process_start_initiated=3, process_start_failed=4,
process_started=5, process_exited=6. Timestamps are **unix seconds**.

### State dir resolution (`dirs`)
`CANDLE_DATABASE_DIR` (verbatim) → `XDG_STATE_HOME/candle` → `~/.local/state/candle`. DB file `candle.db`.

### Load-bearing output strings (verbatim, brackets/backticks included)
- start: `[Started process '<name>' (\`<shell>\`) in directory: '<dir>']`
- check-start skip: `[Service '<name>' is already running]`
- kill: `[Killed '<name>' process with PID: <pid>]`
- errors: `No service '<name>' configured for directory: <cwd>`;
  `No .candle.json file found in (or above) current directory: <cwd>`;
  `No services configured in .candle.json`;
  `Exactly one service name is required when using --shell`;
  `Unrecognized command '<cmd>'`; unknown flags must contain `Unknown argument` (yargs strict parity).
- `list` columns: `NAME STATUS PID UPTIME COMMAND DIRECTORY`; statuses `RUNNING|STOPPED`; `[config changed]`.
- help section headers: `Process Management:`, `Port Detection:`, `Logs:`, `Configuration:`,
  `Documentation:`, `Troubleshooting & Maintenance:`, `Options:`.

### Behavioral subtleties (see scratchpad specs for full detail)
- `isRunByAgent = truthiness of CLAUDECODE` (empty string ⇒ false). Agent mode hides/disables `watch`.
- start handshake: launch info sent as a single-line JSON (no trailing newline) over the sidecar's
  stdin; sidecar reads to EOF. Sidecar is detached (new session) and outlives the CLI.
- start success = sidecar writes `process_started` after surviving a 500ms grace with exit code
  0/None; CLI watches the log table (100ms poll) up to 10s.
- check-start dedup = `killed_at IS NULL` AND PID-liveness probe (also deletes dead rows).
- kill = SIGTERM the process tree, children-first; two PIDs per row (`pid`=shell, `log_collector_pid`=sidecar).
- logs show only the **most recent execution** (filter on last process_start_initiated/started).
- config: `.candle.json` then deprecated `.candle-setup.json`, upward search to FS root; 2-space JSON
  with preserved key order; falsy `root`/`enableStdin` omitted; object-map `services` accepted.
- `set-config` numeric coercion mimics JS `Number()` (trims ws, accepts `1e3`/hex, rejects `3.5`).

## Test-harness integration (the seam)

All spawning funnels through `test/TestWorkspace.ts` (`runCli`, `createMcpApp`, `cleanup`) plus
`test/utils.ts:getCliPath()` (used by `test/simple.test.ts`). Plan:
- Add a target switch: env `CANDLE_TEST_TARGET` = `rust` (default) | `node`.
  - rust → spawn `rust/target/release/candle` directly (no `node` prefix); MCP → `<bin> --mcp`.
  - node → current behavior (`node src/main-cli.ts`). Keeps the Node impl testable.
- The Rust `candle` finds its sidecar as a sibling binary (`log-collector` next to `candle`, i.e.
  `std::env::current_exe()` dir); allow override via `CANDLE_LOG_COLLECTOR_PATH`.
- `test/with-stdin/stdin.test.ts` imports `createStdinMessage` from `src/` — replace with a raw SQL
  insert helper so it is implementation-agnostic.
- `package.json` `test` script: `cargo build --release --manifest-path rust/Cargo.toml && vitest run`
  (keep `pnpm build` only if node target is also being exercised).

## Milestones & task breakdown (TDD chunks, each independently committable)

Verification for every chunk: relevant subset of the Vitest suite passes with `CANDLE_TEST_TARGET=rust`
(after the harness switch lands in M1), plus Rust unit tests for pure logic.

- **M0 — Scaffold.** Restructure Cargo to package `candle` with lib + bins `candle` & `log-collector`.
  Move current `main.rs` logic into `bin/log_collector.rs` (still builds/works). `candle` bin stubs
  `--version`/`help`. Verify: `cargo build --release` produces both binaries.
- **M1 — Foundation + harness switch.** `dirs`, `errors`, `db` (schema/open/migration), and the
  `TestWorkspace` target switch + sidecar path resolution. After this, the suite can run against Rust
  (most tests fail; that's expected). Verify: `version.test.ts`, `erase-database.test.ts` pass.
- **M2 — Config + simple config commands.** `config` model/validate/find + `setup-project`,
  `add-service`, `remove-service`, `set-config`, `list-docs`, `get-doc`, full `--help`/`--version`,
  unknown-command/flag errors. Verify: help, version, errors, add/remove-service, setup-project,
  set/list/get-doc, list-docs tests.
- **M3 — Log collector parity.** Port `bin/log_collector.rs` onto the shared `db`/`config` libs
  (drop the standalone duplication); stdin JSON handshake, grace period, stdin polling, cleanup.
- **M4 — Start/kill/list core.** `process::{alive,tree,subprocess}`, `start::{launch,start_one_service}`,
  `kill`, `kill-all`, `list`/`list-all`. Verify: start, check-start, kill, kill-all, list, list-all,
  list-format, stale-cleanup, log-collector-cleanup, transient-processes, invalid-config.
- **M5 — Logs/restart/clear/eviction.** `logs` (`--count`/`--start-at`, latest-execution filter),
  `clear-logs`, `restart`, log eviction + stale cleanup gating. Verify: logs, clear-logs, restart,
  log-eviction.
- **M6 — Watch/wait-for-log/stdin.** `watch` (`--exit-after-ms`, agent gating), `wait-for-log`,
  stdin feeding end-to-end. Verify: watch, wait-for-log, with-stdin.
- **M7 — Ports/open-browser.** `list-ports`/`list-ports-all` (OS port detection), `open-browser`.
- **M8 — MCP server.** stdio JSON-RPC; tools ListServices, ListPorts, GetLogs, StartService,
  StartTransientService, KillService, RestartService, AddServerConfig. Verify: mcp.test.ts.
- **M9 — Docs + cleanup.** Update `docs-site` / README for any behavior notes; finalize test script;
  confirm full suite green against Rust.

Detailed per-subsystem specs (schema SQL, exact messages, function lists, ordering) were produced by
the mapping pass and are the reference for each milestone.
