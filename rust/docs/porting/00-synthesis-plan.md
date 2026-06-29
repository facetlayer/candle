# Candle → Rust Reimplementation Plan

## 0. Context grounding

- Current Rust: a single package `rust/Cargo.toml` (`name = candle-log-collector`, one binary, `rust/src/main.rs`, 644 lines) that already implements the log-collector sidecar. Deps already present: `rusqlite` (bundled), `serde`, `serde_json`, `clap`, `libc`.
- Node source of truth: `@facetlayer/candle` `0.13.3`. CLI entry `src/main-cli.ts`; sidecar `dist/main-log-collector.js`.
- Acceptance suite: `test/` (Vitest), kept as a black-box conformance harness. Node impl is NOT deleted.
- Hard requirements: log-collector lives in a binary named **`log-collector`**; the main **`candle`** binary reimplements everything else and **always** launches the Rust `log-collector` (no feature flag, no `logCollector: node|rust` runtime switch in the Rust path).

---

## 1. Target Rust crate architecture

### 1.1 Workspace layout

Convert `rust/` into a Cargo workspace with one library crate and two binary crates. Keep everything under `rust/` (product-owner requirement).

```
rust/
  Cargo.toml                # [workspace] members = ["candle-core", "candle-cli", "log-collector"]
  Cargo.lock
  candle-core/              # lib crate: all shared logic (the "subsystems")
    Cargo.toml              # name = "candle-core"
    src/lib.rs
    src/...                 # modules below
  candle-cli/               # bin crate -> produces the `candle` binary
    Cargo.toml              # [[bin]] name = "candle"
    src/main.rs
  log-collector/            # bin crate -> produces the `log-collector` binary
    Cargo.toml              # [[bin]] name = "log-collector"
    src/main.rs             # ported from current rust/src/main.rs, but using candle-core
```

Notes:
- The product owner requires a binary/main *named* `log-collector`. Set `[[bin]] name = "log-collector"` (NOT the crate-default `candle-log-collector`). **This changes the produced binary filename**, so the launcher path in `candle-cli` and the test harness must reference `rust/target/release/log-collector`. (The TS spec assumed `candle-log-collector`; we deliberately rename to satisfy the requirement and wire the launcher to the new name.)
- Almost all behavior lives in `candle-core` so both binaries share the DB/log/config code and so unit tests live in the lib. The two `main.rs` files are thin.

### 1.2 `candle-core` modules → Node subsystem map

| Rust module (`candle-core/src/`) | Node source | Notes |
|---|---|---|
| `dirs.rs` | `src/dirs.ts` | `get_state_directory()` env precedence; `project_root_dir()`; version via `env!("CARGO_PKG_VERSION")` (replaces `findPackageJson`). |
| `errors.rs` | `src/errors.ts` | `CandleError` enum, `is_usage_error()`, `name()`, exact `Display` strings. |
| `debug.rs` | `src/debug.ts` | `debug_log()` gated on non-empty `CANDLE_ENABLE_LOGS`, append `cwd/candle.log`. |
| `run_context.rs` | `src/runContext.ts` | `is_run_by_agent()` via `OnceLock` reading `CLAUDECODE`. |
| `db/mod.rs` | `src/database/database.ts` + sqlite-wrapper | `get_database(override_dir)`, schema, WAL, busy_timeout, additive migration, `Db` wrapper (`run/list/get/upsert`). |
| `db/process_table.rs` | `src/database/processTable.ts` | `ProcessEntry`, 9 query fns keyed on `(command_name, project_dir, pid)`. |
| `db/stdin_messages.rs` | `src/database/stdinMessagesTable.ts` | `create/pop(transactional via DELETE…RETURNING)/clear`. |
| `db/cleanup.rs` | `src/database/cleanup.ts` | `maybe_run_cleanup` (600s gate), `run_cleanup`. |
| `db/stale_process_cleanup.rs` | `src/database/staleProcessCleanup.ts` | `cleanup_stale_processes`. |
| `logs/log_type.rs` | `src/logs/ProcessLogType.ts` | `#[repr(i64)]` enum 1–6 + `TryFrom`. |
| `logs/models.rs` | `processLogs.ts` types | `ProcessLog`, `NewProcessLog`, `LogSearchOptions`. |
| `logs/sql_builder.rs` | `src/logs/SqlBuilder.ts` | `String` + `Vec<Value>`. |
| `logs/build_query.rs` | `src/logs/buildLogSearchQuery.ts` | exact SQL + spacing; port its unit tests. |
| `logs/process_logs.rs` | `src/logs/processLogs.ts` | `save_process_log`, `get_process_logs`, `..._with_eviction_info`. |
| `logs/log_iterator.rs` | `src/logs/LogIterator.ts` | cursor `current_log_id: Option<i64>`. |
| `log_filters/latest_execution.rs` | `LatestExecutionLogFilter.ts` | stateful; `min_timestamp: Option<f64>`. |
| `log_filters/execution_status.rs` | `ExecutionStatusTracker.ts` | `apply`, `count_running_processes`. |
| `console_log.rs` | `src/logs.ts` | `console_log_row`, system/stdout/stderr formatting, `info_log`. |
| `process_alive.rs` | `src/process-alive.ts` | `is_process_alive` (EPERM→alive), `filter_alive_processes` (deletes dead). |
| `process_tree.rs` | `src/process-tree.ts` | `get_process_tree`, platform `get_child_pids`. |
| `kill/*.rs` | `src/kill/*` + `kill-command.ts`, `kill-all-command.ts`, `restart-command.ts` | tree kill, one-process kill, handlers. |
| `config/model.rs`, `paths.rs`, `validate.rs`, `file.rs` | `src/configFile.ts` | order-preserving serde; object-map `services`; loose matching. |
| `config/commands.rs` | `addServerConfig.ts`, `removeServerConfig.ts`, `set-config-command.ts`, `setup-project-command.ts` | 2-space JSON write-back, JS-`Number()` coercion. |
| `subprocess.rs` | `@facetlayer/subprocess` usage | line-buffered spawn, `wait_for_start/exit`, detached/setsid, stdin write. |
| `log_collector/launch.rs` | `launchWithLogCollector.ts` | spawn the `log-collector` binary detached, write JSON to stdin, close. **Rust path only.** |
| `log_collector/monitor.rs` | `startMonitoredService.ts`, `main-log-collector.ts` | service supervision lifecycle (reused by `log-collector` bin). |
| `start/start_one_service.rs`, `start_command.rs` | `startOneService.ts`, `start-command.ts` | check-start dedup, launch, 10s race, banner. |
| `commands/list.rs`, `list_ports.rs`, `open_browser.rs` | `list-command.ts`, `list-ports-command.ts`, `open-browser-command.ts` | table formatting, lsof parsing, browser opener. |
| `commands/logs.rs`, `clear_logs.rs`, `watch.rs`, `wait_for_log.rs` | corresponding `*-command.ts` | log views and tailing. |
| `commands/erase_database.rs` | `erase-database` handler | clear state. |
| `doc_files.rs` | `src/docFiles/DocFilesHelper.ts` | frontmatter parse, list-docs/get-doc, `indexmap`. |
| `assert_valid_command_name.rs` | `src/cli/assertValidCommandName.ts` | thin validator over `get_service_info_by_name`. |
| `mcp/*` | `src/mcp/*` | tools registry, capture sink, server. |
| `output.rs` | (new) | **output-sink abstraction** (see §1.4) so MCP can capture handler output instead of monkeypatching console. |

`candle-cli/src/main.rs` = clap parser + dispatch (maps `src/main-cli.ts`).
`log-collector/src/main.rs` = read launch info (stdin JSON or flags) + `candle_core::log_collector::monitor::run(...)`.

### 1.3 Recommended crates

- **CLI parsing:** `clap` 4 (derive). Must emulate yargs strictness and the literal substring `Unknown argument` in error output (custom error formatting; see Risks §5).
- **SQLite:** `rusqlite` 0.31 `features=["bundled"]` (already in use). Wrap in `Mutex<Connection>` behind a `OnceLock`/`OnceCell` to mirror the singleton-per-process semantics. Set WAL + `busy_timeout=30000` on open.
- **Serialization:** `serde` + `serde_json` with **`features=["preserve_order"]`** (pulls in `indexmap`) for byte-identical, key-order-preserving config write-back.
- **Ordered map for doc files:** `indexmap`.
- **Process signals/liveness:** `nix` (`signal::kill`, `unistd::setsid`) — cleaner than raw `libc`; keep `libc` if preferred. Decision: adopt `nix` for kill/setsid, retain `libc` only if a specific call is missing.
- **Frontmatter regex:** `regex`.
- **Home dir:** `dirs` crate (or read `$HOME`/`$XDG_STATE_HOME` directly to exactly match Node `os.homedir()`).
- **Time:** std `SystemTime` for unix seconds; `chrono` only for `info_log` ISO-8601 timestamp.
- **Concurrency/timers:** prefer a **synchronous** design (std threads + `std::thread::sleep`) to mirror `rusqlite`'s sync model and the TS synchronous SQLite semantics. Use threads + channels for line-buffered stdout/stderr readers (the existing `main.rs` already does `mpsc`/`thread`). Avoid forcing `tokio` except where MCP needs async.
- **MCP:** see §1.5.

### 1.4 Output-sink abstraction (cross-cutting, build early)

The MCP server captures handler console output. Rather than a global console, define in `output.rs`:

```rust
pub trait Output { fn out(&mut self, line: &str); fn err(&mut self, line: &str); }
pub struct StdOutput;            // out->println! stdout, err->eprintln! stderr
pub struct CaptureOutput { pub logs: Vec<String> } // err prefixes "[stderr] "
```

Every command handler takes `&mut dyn Output` instead of calling `println!`/`eprintln!` directly. CLI passes `StdOutput`; MCP passes `CaptureOutput` then `take_logs()`. This replaces the monkeypatch design and is required for MCP parity. **This is a project-wide convention; establish it in Milestone A so handlers are written against it from the start.**

### 1.5 MCP approach

Two options:
- **(A) Use `rmcp`** (official Rust MCP SDK). Pros: maintained, handles stdio JSON-RPC and `initialize`/`tools/list`/`tools/call`. Cons: must map our exact tool schemas, instructions string, error codes (`MethodNotFound=-32601`, `InvalidRequest=-32600`), and content-array ordering; `rmcp` is async (tokio).
- **(B) Hand-roll a minimal stdio JSON-RPC server** in `serde_json`. Pros: total control over byte-level output, content ordering, `isError` shaping, stdin-close→exit(0); stays synchronous. Cons: more code; must implement `initialize` handshake.

**Recommendation: (B) hand-rolled**, because the acceptance tests (`test/mcp.test.ts`, `expect-mcp`) assert specific tool list, JSON result shapes, `isError`, and content ordering, and because the rest of candle-core is synchronous. A ~300-line `mcp/server.rs` reading line-delimited JSON-RPC from stdin and writing responses to stdout gives exact parity with `src/mcp/mcp-main.ts`. Reserve `rmcp` as a fallback if the handshake proves fiddly.

---

## 2. Shared foundation (build FIRST — blocks everything)

These have no candle-internal dependencies and gate all command work. Build and unit-test them before any command handler.

1. **`dirs.rs`** — `get_state_directory()`: `CANDLE_DATABASE_DIR` (verbatim) → `XDG_STATE_HOME/candle` → `~/.local/state/candle`. `project_root_dir()` (used to find the sibling `log-collector` binary). Version = `env!("CARGO_PKG_VERSION")` — **must be set to `0.13.3`** in each crate's `Cargo.toml` (Risk §5).
2. **`errors.rs`** — `CandleError` enum: `Usage`, `ConfigFile`, `MissingServiceWithName{cwd,command_name}`, `MissingSetupFile{cwd}`, `ProcessStartFailed{command_name,recent_logs}`. Exact `Display` templates; `is_usage_error()` (all true except `ConfigFile`); `name()` returns the literal `.name` strings incl. `"NeedRunCommandError"`, `"MissingSetupFile"`.
3. **`logs/log_type.rs`** — `ProcessLogType` `#[repr(i64)]` (1–6), `TryFrom<i64>`, `is_lifecycle_event`, `is_running_event`.
4. **`db/mod.rs`** — connection bootstrap: ensure state dir, open `candle.db`, `PRAGMA journal_mode=WAL`, `PRAGMA busy_timeout=30000`, run schema (`CREATE TABLE/INDEX IF NOT EXISTS` for the 4 tables + 4 indexes, exact DDL incl. `default (strftime('%s','now'))`), additive `ALTER TABLE ADD COLUMN` for `log_collector_pid/shell/root/killed_at`. Provide `Db { conn: Mutex<Connection> }` with `run -> usize`, `list`, `get`, `upsert(table,key_empty,values)` (UPDATE-then-INSERT-if-0). Singleton via `OnceCell` honoring `override_dir` on first call only.
5. **`config/*`** — full config subsystem: model (serde `preserve_order`, array+object-map `services`, `skip_serializing_if` for `root`/`enableStdin`), lexical path validation (`is_valid_root_path` with `..`-prefix string test, no `canonicalize`), `validate_config` (exact messages), `find_config_file` (upward walk, filename priority, parse-error wrapping), `read_config_file` (empty-file→`{services:[]}`), `get_log_eviction_config` (defaults 1000/86400), loose matcher (`find_loose_command_name`/`get_service_config_by_name`), `resolve_command_names_or_all`.
6. **`output.rs`** — the `Output` trait + `StdOutput`/`CaptureOutput` (§1.4).
7. **`process_alive.rs`** — `is_process_alive` (kill 0; EPERM→alive, ESRCH→dead) + `filter_alive_processes` (deletes dead rows).
8. **`db/process_table.rs`**, **`db/stdin_messages.rs`** — the row CRUD (depend on 3,4).

Everything in §3 milestones B+ depends on these eight items.

---

## 3. Ordered task breakdown (TDD chunks, swe-work style)

Each task lists: title · files · deps (task #s) · verification. Tasks are sized to be independently committable. Most verification runs the *existing* Vitest suite, which only works once the harness is repointed (Milestone H, task 26) — so foundation tasks rely on **Rust unit tests** (`cargo test`) ported from the corresponding TS `__tests__`, and the full Vitest suite becomes green progressively. **Recommendation: land the harness toggle (task 26) early, behind an env var (`CANDLE_TEST_BINARY`), so each command milestone can run the real suite as it completes.**

### Milestone A — Workspace + foundation
1. **Workspace split.** Files: `rust/Cargo.toml` (workspace), new `candle-core/`, `candle-cli/`, `log-collector/` crates; move current `rust/src/main.rs` into `log-collector/src/main.rs` unchanged; set `[[bin]] name="log-collector"`; set every crate `version="0.13.3"`. Deps: none. Verify: `cargo build --release` produces `target/release/candle` (stub) and `target/release/log-collector`.
2. **dirs + errors + log_type + run_context + debug.** Files: `candle-core/src/{dirs,errors,run_context,debug,logs/log_type}.rs`, `lib.rs`. Deps: 1. Verify: `cargo test` unit tests for state-dir precedence, error `Display`/`name`, `CLAUDECODE` truthiness, log-type round-trip.
3. **db bootstrap + Db wrapper.** Files: `candle-core/src/db/mod.rs`. Deps: 2. Verify: `cargo test` opens a temp DB, asserts schema via `PRAGMA table_info`/`sqlite_master` matches the exact DDL incl. indexes; WAL pragma returns `wal`; upsert single-row behavior.
4. **process_table + stdin_messages.** Files: `candle-core/src/db/{process_table,stdin_messages}.rs`. Deps: 3. Verify: `cargo test` CRUD keyed on `(command_name,project_dir,pid)`; `pop` transactional FIFO.
5. **output sink.** Files: `candle-core/src/output.rs`. Deps: 1. Verify: unit test that `CaptureOutput.err` prefixes `[stderr] `.
6. **config subsystem.** Files: `candle-core/src/config/*`. Deps: 2. Verify: `cargo test` ported from `configFile` tests — object-map services, `..`-prefix path rule, empty file, validation messages verbatim, 2-space order-preserving serialize.

### Milestone B — Logs core
7. **sql_builder + build_query.** Files: `candle-core/src/logs/{sql_builder,build_query,models}.rs`. Deps: 3. Verify: port `buildLogSearchQuery.test.ts` verbatim (exact SQL strings/spacing, IN-clause `?, ?`, throw on neither).
8. **process_logs + eviction info.** Files: `candle-core/src/logs/process_logs.rs`. Deps: 4,7. Verify: `cargo test` insert+read chronological reverse; eviction count-subquery when `rows>=limit`.
9. **log_iterator.** Files: `candle-core/src/logs/log_iterator.rs`. Deps: 8. Verify: `cargo test` cursor advance, `reset_to_latest`, `afterLogId>` strictness, `Some(0)` vs `None`.
10. **filters.** Files: `candle-core/src/log_filters/{latest_execution,execution_status}.rs`. Deps: 3,7. Verify: `cargo test` — `min_timestamp` float math, launch-boundary on type 3, statefulness across batches, `count_running_processes` counts 3&5.
11. **console_log + info_log.** Files: `candle-core/src/console_log.rs`. Deps: 3,5. Verify: `cargo test` exact pretty/json strings, hidden types 3&5, `[stderr] ` and bracket system formatting; `info_log` env gate.

### Milestone C — Process control primitives
12. **process_tree.** Files: `candle-core/src/process_tree.rs`. Deps: 2. Verify: `cargo test` parse `pgrep`/`ps` output, NaN drop; integration spawning a child tree (mac/linux).
13. **kill tree + one + handlers.** Files: `candle-core/src/kill/*`. Deps: 4,7,12, process_alive(task in A? — process_alive is task 7-foundation; schedule as 13a). Verify: Vitest `kill.test.ts`, `kill-all.test.ts` once harness repointed; meanwhile `cargo test` for `KillResult` mapping (ESRCH→not_found, reverse order, 300s stale branch).
    - **13a process_alive.** Files: `candle-core/src/process_alive.rs`. Deps: 4. (Foundation; can move into Milestone A.)
14. **stale cleanup + cleanup/eviction.** Files: `candle-core/src/db/{stale_process_cleanup,cleanup}.rs`. Deps: 8,13a, config(6). Verify: port `cleanup.test.ts` to `cargo test` (time eviction, per-service offset+id delete, `process_last_cleanup` upsert, exact stale log string).

### Milestone D — Subprocess + sidecar
15. **subprocess abstraction.** Files: `candle-core/src/subprocess.rs`. Deps: 1. Verify: `cargo test` line splitting incl. trailing partial line on EOF; detached spawn via `setsid`; `wait_for_start/exit`. (Reuse logic already in current `main.rs`.)
16. **log_collector monitor (lib).** Files: `candle-core/src/log_collector/monitor.rs`. Deps: 4,8,15. Verify: `cargo test` lifecycle: create row → 500ms grace → process_started/start_failed branches (incl. the asymmetry: waitForStart-reject does NOT delete row, grace-fail DOES), process_exited+delete, stdin polling 500ms, 60s cleanup interval, exact content strings.
17. **port log-collector binary onto core.** Files: `log-collector/src/main.rs` (rewrite to call `candle_core::log_collector::monitor::run`), keep stdin-JSON + flag parsing. Deps: 16. Verify: pipe a `LogCollectorLaunchInfo` JSON to the built binary against a temp DB; assert rows. (Standalone integration test in `cargo test` or a shell test.)
18. **launch (Rust collector only).** Files: `candle-core/src/log_collector/launch.rs`. Deps: 1,15. Verify: `cargo test` — resolves sibling `log-collector` binary path relative to the `candle` exe (use `std::env::current_exe()` → same dir), spawns detached, writes JSON line (no trailing newline), closes stdin. **No node branch; always Rust.**

### Milestone E — Start flow
19. **start_one_service + start_command.** Files: `candle-core/src/start/*`. Deps: 6,9,11,13,13a,16,18. Verify: Vitest `start.test.ts`, `check-start.test.ts` (after harness). Assert exact banner `[Started process '<name>' (\`<shell>\`) in directory: '<dir>']`, `[Service '<name>' is already running]`, 10s timeout message; check-start dual-liveness dedup; transient `--shell`/`--root` validation.
20. **restart.** Files: `candle-core/src/kill/restart.rs` (or `commands/restart.rs`). Deps: 13,19,6. Verify: Vitest `restart.test.ts` (snapshot-before-kill, config reload of edited shell, ≤1 Killed message).

### Milestone F — Read/view commands
21. **list / list-all.** Files: `candle-core/src/commands/list.rs`. Deps: 4,6,13a. Verify: Vitest `list.test.ts`, `list-all.test.ts`, `list-format.test.ts` (columns `NAME STATUS PID UPTIME COMMAND DIRECTORY`, `[config changed]`, `formatUptime` `0s`, two-space pad).
22. **list-ports / open-browser.** Files: `candle-core/src/commands/{list_ports,open_browser}.rs`. Deps: 12,6. Verify: `cargo test` for `parse_lsof_output` (dedup, `*`→`0.0.0.0`, IPv6 lastIndexOf split); Vitest open-browser path; preserve the `list-ports` positional-name quirk (decide per tests).
23. **logs / clear-logs.** Files: `candle-core/src/commands/{logs,clear_logs}.rs`. Deps: 8,10,11. Verify: Vitest `logs.test.ts`, `clear-logs.test.ts` (exact `✓ Cleared N log entries`, `No logs found...`, `-- older logs have been removed --`, orphan delete + vacuum).
24. **watch / wait-for-log.** Files: `candle-core/src/commands/{watch,wait_for_log}.rs`. Deps: 9,10,11,19. Verify: Vitest `watch.test.ts`, `wait-for-log.test.ts` (200ms poll, `--exit-after-ms` hidden, agent-mode guard exact stderr+exit1, `--message`/`--timeout` seconds→ms, exact success/failure strings, exit codes).

### Milestone G — Config-mutation, docs, misc commands
25. **add/remove-service, set-config, setup-project, erase-database.** Files: `candle-core/src/config/commands.rs`, `candle-core/src/commands/erase_database.rs`. Deps: 6. Verify: Vitest `add-service`/`remove-service`/`setup-project`/`erase-database` tests (exact stdout, 2-space JSON, falsy-field omission, JS-`Number()` coercion in set-config).
26. **doc_files + list-docs/get-doc.** Files: `candle-core/src/doc_files.rs`. Deps: 2. Verify: Vitest `list-docs.test.ts`, `get-doc.test.ts` (frontmatter parse, fuzzy match, `formatGetDocCommand` → `candle get-doc <file>`, exact list format). Ensure `docs/` + `README.md` resolve relative to repo (for tests, relative to the candle binary's project root — see Risk §5 on doc path resolution).

### Milestone H — CLI wiring + harness + MCP
27. **(LAND EARLY) Test-harness repoint behind env var.** Files: `test/TestWorkspace.ts`, `test/utils.ts`, `bin/test-candle.ts`, `package.json`, `test/with-stdin/stdin.test.ts`. Deps: 1 (so a binary exists). Verify: with `CANDLE_TEST_BINARY` unset → Node (unchanged); set → Rust. See §4. This task is sequenced *first within H conceptually but implemented right after task 1* so each milestone can run the suite.
28. **clap CLI dispatch (`candle` bin).** Files: `candle-cli/src/main.rs`. Deps: all command tasks (13,19–25). Verify: Vitest `help.test.ts`, `errors.test.ts`, `version.test.ts`, `simple.test.ts` (grouped help section headers, `Unknown argument` on bad flags, `Unrecognized command`, `--version`=`0.13.3`, `-v` short-circuit, command aliases `ls`/`run`/`stop`).
29. **MCP server.** Files: `candle-core/src/mcp/*`, dispatch `candle mcp`/`--mcp` in `candle-cli`. Deps: 28 + handlers + output sink (5). Verify: Vitest `mcp.test.ts`, `invalid-config.test.ts` MCP cases (9-tool list & order, JSON result shapes, `isError`, logs-before-result content ordering, stdin-close→exit0, stdout purity). Decide on the AddServerConfig duplicate-log quirk (default: replicate).
30. **Full suite green + docs.** Files: `docs-site/`, `CLAUDE.md`, `package.json` test script. Deps: 28,29. Verify: full `vitest run` against Rust binaries green; update public docs for any intentional behavior changes (e.g. removal of `logCollector` runtime switch).

---

## 4. Test-harness integration plan

Goal: the existing Vitest suite spawns the Rust `candle` + `log-collector` binaries, while preserving the ability to still test the Node impl.

### 4.1 Binary selection via env var (single switch)
Introduce `CANDLE_TEST_BINARY` (or `CANDLE_TEST_TARGET=node|rust`). In `test/TestWorkspace.ts`:

- Add a resolver:
  - `rust`: `cliPath = <repo>/rust/target/release/candle`, spawn form = `runShellCommand(cliPath, [...args])` (no `node` prefix).
  - default/`node` (current): `cliPath = <repo>/src/main-cli.ts`, spawn `runShellCommand('node', [cliPath, ...args])`.
- Centralize the `(command, baseArgs)` pair in one helper used by `runCli` (line ~72) and `cleanup` (line ~127) and `createMcpApp` (line ~91). For MCP: rust → `mcpShell(\`${cliPath} --mcp\`)`; node → unchanged.
- `test/utils.ts:getCliPath()` (used by `test/simple.test.ts`) gets the same resolver; `simple.test.ts` must branch its `spawn('node',[cliPath])` vs `spawn(cliPath)`. Simplest: have `getCliPath()` return `{cmd, args}` and update the one caller.
- `bin/test-candle.ts`: add the same resolver (dev convenience).

Env passed to children is unchanged and already correct for Rust: `CANDLE_DATABASE_DIR=dbDir`, `FORCE_COLOR=0`, `CLAUDECODE=''`. Rust must honor all three (DB isolation, no ANSI, agent-off).

### 4.2 Collector path seam
- The Node CLI spawns `dist/main-log-collector.js`; the Rust CLI must spawn `rust/target/release/log-collector`. In Rust this is handled internally by `log_collector/launch.rs` resolving the sibling binary via `std::env::current_exe()` → same directory as `candle`. Since both binaries are produced in `rust/target/release/`, no env var is needed. (Keep an override `CANDLE_LOG_COLLECTOR_PATH` env for flexibility/tests if desired.)
- Remove the `logCollector: node|rust` decision in the Rust path: it **always** launches the Rust collector (product requirement). The `.candle.json` `logCollector` key is still parsed/validated (config tests depend on validation messages) but ignored at launch time in the Rust CLI.

### 4.3 Direct-DB / library-import seams
- `test/with-stdin/stdin.test.ts` imports `createStdinMessage` from `src/database/stdinMessagesTable.ts` (SEAM #4). For Rust runs this won't drive the Rust DB unless it writes to the same `candle.db`. Fix: replace that import with a **raw `node:sqlite` INSERT** into `stdin_messages` against `<dbDir>/candle.db` (the test already knows `dbDir`). This is binary-agnostic (works for both Node and Rust) since both use byte-identical schema. Implement as a tiny `insertStdinMessage(dbDir, row)` test helper.
- `check-start.test.ts`, `list.test.ts`, `log-collector-cleanup.test.ts` already use raw `node:sqlite` SQL against `candle.db` — these are binary-agnostic provided the Rust schema is byte-identical (DDL, defaults, `strftime('%s','now')`, column order). No change needed; they become a parity check.

### 4.4 Build step
- Change `package.json` `test` script to build both targets conditionally. Simplest: keep `pnpm build && vitest run` for node; add `test:rust`: `(cd rust && cargo build --release) && CANDLE_TEST_TARGET=rust vitest run`. Document both. The Node build remains required while the Node impl exists (and for default test runs).
- CI: add a `test:rust` job that runs `cargo build --release` then the suite with the env var set.

### 4.5 Version parity
- `test/cli/version.test.ts` reads `package.json` version and asserts the CLI prints it. Set all Rust crate `version = "0.13.3"` and emit `env!("CARGO_PKG_VERSION")`. Add a CHANGELOG/release note that the Rust crate version must track `package.json`. (Optionally a build script reads the root `package.json` to keep them in sync.)

---

## 5. Risks / subtle parity issues

**Output-string parity (highest risk).** Dozens of tests substring-match exact bytes. Reproduce verbatim including brackets/backticks/quotes/Unicode: start banner `[Started process '<name>' (\`<shell>\`) in directory: '<dir>']`, `[Service '<name>' is already running]`, `[Killed '<name>' process with PID: <pid>]`, `[Cleaning up stale process entry for ...]`, `✓ Cleared N log entries` (U+2713), `-- older logs have been removed --`, `\nLogs cleared successfully!`, all error templates from `errors.rs`. Build a parity test that diffs Rust vs Node output for each command early.

**`Unknown argument` and yargs strictness.** clap's default error text differs from yargs. `errors.test.ts` expects strings containing `Unknown argument` and `Unrecognized command`. Customize clap error rendering (override `error`/`unknown_argument` messaging, or post-process clap errors) to contain those literal substrings. Also map missing-required-arg and unknown-subcommand to non-zero exit with the expected wording.

**Help text.** `help.test.ts` snapshots grouped help via `normalizeOutput`. clap won't produce the exact grouped layout (`Process Management:`, `Port Detection:`, `Logs:`, `Configuration:`, `Documentation:`, `Troubleshooting & Maintenance:`, `Options:`). Likely need a hand-written help renderer (or clap `help_template` + heading groups) to match. Treat as its own task; high effort.

**Exit codes.** `wait-for-log` →1 on failure/0 success; `watch` agent-mode →1; usage errors →1; success →0; unknown command non-zero. clap exits 2 on parse error by default — verify tests only check `!= 0` vs an exact code (most use `failed()`); if any assert exit 1, override clap's exit code.

**SQL/schema byte parity.** Tests open `candle.db` with raw SQL. Keep DDL identical incl. `default (strftime('%s','now'))`, autoincrement, column order, the 4 indexes (esp. `idx_process_output_lookup (project_dir, command_name, timestamp desc, id desc)`). Additive migration only (no rebuild/drop). Verify via `sqlite_master` snapshot test. Timestamps are **unix seconds** everywhere; never milliseconds. The `recentWindowMs/1000` float cutoff must stay `f64` compared with `>=`.

**Daemonization / detach.** The sidecar must outlive the CLI: spawn with `setsid` (new session), do NOT wait/join, let `candle` exit after the 10s watch race. Do not `unref`-equivalent kill it. stdin handshake JSON has **no trailing newline** and is only delivered on stdin close — write JSON then close stdin; collector reads to EOF. Getting EOF wrong hangs every start.

**Process-tree / ports detection (platform).** `pgrep -P` (macOS) vs `ps --ppid` (Linux); `lsof -iTCP -sTCP:LISTEN -n -P` parsing is positional/brittle (≥9 fields, name=second-to-last, split at last `:`, dedup by `pid:port`, `*`→`0.0.0.0`, default proto TCP). Windows returns no children (out of scope; candle targets mac/linux). Liveness: `kill(pid,0)` mapping EPERM→alive, ESRCH→dead — both `pid` and `log_collector_pid` deadness define staleness.

**check-start / stale dedup ordering.** Must filter `killed_at IS NULL` AND probe liveness AND delete dead rows inline (`filter_alive_processes`), done before config resolution. Tests inject fake PID `2147483000`; Rust must treat it as dead.

**MCP protocol purity & ordering.** stdout carries only JSON-RPC frames — route all handler output through `CaptureOutput`, never real stdout. Content array: logs item first, then result (pretty 2-space JSON) or `Error: <message>` with `isError:true`. `KillService`/`AddServerConfig` return no result (logs-only). Unknown tool → `McpError MethodNotFound (-32601)`. stdin-close → `exit(0)` (wire explicitly). Preserve (or consciously fix) the AddServerConfig duplicate success log; default to replicating for parity then revisit.

**Config nuances.** Object-map `services`; unknown keys preserved on round-trip (order-preserving serde); `..`-prefix is a *string* prefix test on lexically-normalized path (not `canonicalize`); empty file → `{services:[]}`; `set-config` numeric coercion mimics JS `Number()` (accepts `1e3`, `0x10`, trims whitespace, rejects `3.5`/`3abc`/empty). Write-back is 2-space pretty, no trailing newline, insertion-order keys.

**`launchDir` divergence.** Success banner special-cases absolute `root` (`Path.isAbsolute? root : join`), but `startMonitoredService` cwd uses `join(projectDir, root)` unconditionally. Decide: preserve divergence (safer for test parity) or unify. Default: preserve.

**Binary naming.** Product requires the collector binary be named `log-collector`; the TS launcher and tests referenced `candle-log-collector`. We standardize on `log-collector` and repoint all references (launcher resolves sibling binary; harness env optional). Document this rename.

**`CARGO_PKG_VERSION` drift.** Crate versions must equal `package.json` `0.13.3` or `version.test.ts` fails. Add a sync check (build script or CI assertion).