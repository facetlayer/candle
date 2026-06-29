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

---

# 🔧 HANDOFF — current status & how to continue

**Branch:** `rust-port` (off `main`). Each milestone is its own commit(s); nothing is merged to `main`.

## What works today

- **M0–M8 complete and committed (plus `erase-database`).** The Rust `candle` binary can:
  `--version`, grouped/per-command `help`, `setup-project`, `add-service`, `remove-service`,
  `set-config`, `list-docs`, `get-doc`, `start`/`run`/`check-start`, `kill`/`stop`, `kill-all`,
  `list`/`ls`, `list-all`, `logs`, `clear-logs`, `wait-for-log`, `watch`, `restart`,
  `erase-database`, and `mcp`/`--mcp` (stdio JSON-RPC server). The `log-collector` sidecar is fully
  ported onto `candle-core` and launched (detached, stdin-JSON handshake) by `start`. Verified
  end-to-end. **Only `list-ports`/`open-browser` (M7) remain stubbed — and they have no acceptance
  tests.**
- **Test harness toggle is in place.** `CANDLE_TEST_TARGET=rust` runs the *existing* Vitest suite
  against the Rust binary; unset/`node` keeps the Node path. The Rust binary finds its sidecar as a
  sibling of `current_exe()` (override `CANDLE_LOG_COLLECTOR_PATH`).
- **Quality gates green:** `cargo test` (134 unit + 4 + 1 integration), and
  `cargo clippy --workspace --all-targets -- -D warnings` are clean.

## Acceptance-suite status against Rust (snapshot)

`CANDLE_TEST_TARGET=rust npx vitest run` → **322 passed / 0 failed (322 total); all 33 files green.**
The Rust port is at full acceptance-suite parity with the Node implementation. The only Node commands
not yet ported are `list-ports`/`open-browser` (M7), which have **no acceptance tests** in this suite
(the MCP `ListPorts`/`OpenBrowser` tools are registered but return a "not yet implemented" error if
invoked — they are never called by any test).

## Build & test commands

```
cd rust && cargo build --release                                   # builds candle + log-collector
cd rust && cargo test                                              # rust unit/integration tests
cd rust && cargo clippy --workspace --all-targets -- -D warnings   # lint gate (must be clean)
CANDLE_TEST_TARGET=rust npx vitest run [files...]                  # acceptance suite vs Rust
npx vitest run [files...]                                          # acceptance suite vs Node (baseline)
pnpm test:rust                                                     # cargo build --release + suite vs Rust
```

## Architecture as built (candle-core modules)

`dirs`, `errors` (`CandleError`), `debug`, `run_context`, `output` (capturable sink — see below),
`db/{mod (schema+WAL+migration, get_database/open_database_at), process_table, stdin_messages,
cleanup}`, `config/{model,paths,validate,file,commands}`, `logs/{log_type, process_logs,
log_iterator, console_log}`, `process_alive`, `process_tree`, `kill`, `commands/{mod
(assert_valid_command_names), list}`, `log_collector/{mod, monitor}`, `start/{launch,
start_one_service, start_command}`, `doc_files`. CLI: `candle-cli/src/{main (hand-rolled dispatch),
parser, help}`.

**Output sink (`candle_core::output`):** handlers emit via `output::out`/`output::err`. In the CLI
this passes through to real stdout/stderr; `output::capture(f)` buffers it. This exists so the **MCP
server (M8)** can capture handler output. New command handlers SHOULD emit through this sink. `list`
additionally returns structured data + a JSON serializer (MCP and `--json` both use the JSON form).

## Remaining work — do in this order

> **DONE (steps 1–3 below): M5/M6 log-viewing cluster + `restart` + `log-eviction` parity.**
> Committed on `rust-port`: `log_filters/` (`LatestExecutionLogFilter` + `ExecutionStatusTracker`)
> with `LogIterator` per-call/default limit support; `commands::{logs, clear_logs, wait_for_log,
> watch, restart}`, all wired into `candle-cli`; the `test/with-stdin/stdin.test.ts` seam now does a
> raw `node:sqlite` insert; and `erase-database` was ported opportunistically. `log-eviction`,
> `with-stdin`, `watch`, `wait-for-log`, `logs`, `clear-logs`, `restart`, `erase-database` test files
> are all green. **The only remaining suite failures are M8 (MCP) — start at step 5.** Steps 1–3 are
> retained below for historical reference.

1. **Log-viewing cluster (M5/M6) — ✅ DONE.** Scaffold already committed:
   `logs::console_log` (row/system-message formatting) and `process_logs::
   get_process_logs_with_eviction_info` exist and are tested but **unused**. TODO:
   - `log_filters/` — port `LatestExecutionLogFilter` + `ExecutionStatusTracker` (only-most-recent
     execution; `logs` must hide earlier runs' markers — `logs.test.ts` depends on it).
   - `commands::logs` (`--count` default 100, `--start-at`, latest-execution filter, `No logs found`,
     eviction indicator) — port `logs-command.ts`/`processLogs.ts`.
   - `commands::clear_logs` — exact strings `✓ Cleared N log entries` (U+2713) / `Logs cleared
     successfully!` / `No logs found to clear`; orphan cleanup + vacuum. Port `clear-logs-command.ts`.
   - `commands::wait_for_log` — poll for a log whose content CONTAINS `message` (partial,
     case-SENSITIVE); `--timeout` SECONDS→ms (default 30; ≤0 → immediate fail); exit 1 on
     failure/0 on success; failure text matches `/timeout|not found|failed/i`. Port
     `wait-for-log-command.ts`. **HIGHEST PRIORITY — unblocks the most tests.**
   - `commands::watch` — `is_run_by_agent()` → exact stderr `Error: 'watch' is not available in agent
     mode. Use 'candle logs' to view process output.` + exit 1; else header `Watching process
     '<name>'`, print existing logs, stream live (~200ms poll), stop after hidden `--exit-after-ms`.
   - Wire all four into `candle-cli` dispatch (resolve project dir; do NOT validate command names —
     transient allowed; `wait-for-log` exits 1 on failure; `watch` checks agent mode first).
   - **Test seam fix:** `test/with-stdin/stdin.test.ts` imports `createStdinMessage` from
     `src/database/stdinMessagesTable.ts` — replace with a raw `node:sqlite` INSERT into
     `stdin_messages` at `<dbDir>/candle.db` so it drives the Rust DB too (schema is byte-identical).
   - Spec refs: `rust/docs/porting/map-logs.md`, `map-watch-wait.md`; Node `src/logs*.ts`,
     `src/log-filters/*`, `src/*-command.ts`.
   - Verify: `logs`, `clear-logs`, `wait-for-log`, `watch`, `with-stdin`, `log-eviction` tests, plus
     the many `start`/`kill`/`list`/`transient`/`stale-cleanup`/`log-collector-cleanup` tests that
     unblock once `wait-for-log` exists.

2. **`restart` (M5) — ✅ DONE.** Small: port `restart-command.ts` — snapshot `(shell,root)` per name BEFORE
   killing, `handle_kill_command`, then `start_one_service` per name; if the name is defined in
   config pass `shell=None`/`root=None` so it RELOADS from config (picks up edited shell); else use
   the captured shell/root. Errors: `No running processes found in this project to restart`,
   `Failed to restart: <msg>` (stderr), unknown → `No service` (via assert_valid_command_names).
   Spec: `rust/docs/porting/map-kill-restart.md §7`. Verify `restart.test.ts`.

3. **`log-eviction` parity (M5) — ✅ DONE.** `db::cleanup::run_cleanup` already implemented eviction;
   `log-eviction.test.ts` passes (5/5) now that `wait-for-log`/`logs` exist.

4. **Ports / open-browser (M7).** `list-ports`/`list-ports-all` (parse `lsof -iTCP -sTCP:LISTEN -n
   -P`; see `map-list-ports-browser.md` for the brittle positional parsing — dedup by `pid:port`,
   `*`→`0.0.0.0`, IPv6 split on last `:`), `open-browser`. NOTE: there are **no acceptance tests**
   for these in the current Vitest suite, so this step does not move the suite count; port for
   feature parity. Still dispatch-stubbed (`not_implemented`).

5. **MCP server (M8) — ✅ DONE.** Hand-rolled stdio JSON-RPC in `candle_core::mcp` (newline-delimited
   frames; `initialize`/`tools/list`/`tools/call`/`ping`; `notifications/initialized` ignored; EOF →
   `exit(0)`). All 9 tools registered (`ListServices, ListPorts, GetLogs, StartService,
   StartTransientService, KillService, RestartService, AddServerConfig, OpenBrowser`); `ListPorts`/
   `OpenBrowser` return a not-implemented error (untested). Handler output routed through
   `candle_core::output::capture` + the new `CapturedOutput::mcp_log_lines()` (stderr lines prefixed
   `[stderr] `); content array = logs item then result (pretty 2-space JSON) or `Error: <msg>` with
   `isError:true`; unknown tool → JSON-RPC `-32601`. `mcp.test.ts` (10/10) and the
   `invalid-config.test.ts` MCP cases pass. `--mcp`/`mcp` wired in `candle-cli` (`run_mcp` →
   `serve_mcp`).

6. **Finalize (M9) — remaining.** Full suite is green (322/322). Still TODO for a true finish:
   update `docs-site`/README/CLAUDE.md for intentional changes (the `logCollector` runtime switch is
   parsed/validated but ignored — the Rust CLI always uses the Rust sidecar); decide whether to flip
   the default `CANDLE_TEST_TARGET` to `rust` and make `package.json` `test` build Rust first; update
   CI. Optionally port the M7 `list-ports`/`open-browser` commands for full feature parity (no tests
   gate them).

## Gotchas learned (save yourself time)

- **Output strings are load-bearing** — tests do raw substring matching. Copy brackets/backticks/
  quotes/emoji verbatim (e.g. start banner `[Started process '<name>' (\`<shell>\`) in directory:
  '<dir>']`, `✓ Cleared N log entries`). `FORCE_COLOR=0` is set by the harness → emit NO ANSI.
- **`CLAUDECODE=''` (empty) ⇒ NOT agent mode** (JS truthiness). The harness sets it empty, so `watch`
  is ENABLED in tests. `run_context::is_run_by_agent()` already handles this.
- **Schema is byte-parity with Node** and several tests open `candle.db` with raw SQL (fake PID
  `2147483000` = "dead"). Don't change the DDL. Timestamps are unix **seconds** everywhere.
- **Sidecar handshake needs EOF:** the launch-info JSON has NO trailing newline; the sidecar reads
  stdin to EOF, so the launcher must close stdin after writing. Sidecar is detached via `setsid` in
  `pre_exec` and never waited on, so it outlives the CLI.
- **`list` COMMAND column shows the command *name*, not the shell** (Node sets `command:
  command_name`). That is correct parity, not a bug.
- **`wait-for-log` is a hidden dependency of most start/kill/list tests** — they `start` then
  `wait-for-log` for a marker before asserting. Port it early.
- **Don't run two subagents that both edit `candle-core/src/lib.rs` concurrently** — they race on the
  `pub mod` lines. Sequence candle-core work; CLI/test-harness edits can run in parallel with
  candle-core work.
- One subagent **stalled on a stream watchdog** mid-cluster; its only salvaged output is the
  committed `console_log` scaffold. The cluster commands themselves still need doing (step 1).

---

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
