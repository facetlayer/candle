# Candle (Rust) — architecture reference

This directory documents how the `candle` CLI under `rust/` is built. The Vitest suite in
`../../test` runs against the compiled binary and is the conformance harness (see
[testing.md](testing.md)); when a doc and the suite disagree, the suite is authoritative.

These are internal docs aimed at developers working on the Rust code. Each subsystem doc describes
what the code does, the exact strings/SQL/algorithms it must produce, and the subtleties that are
easy to get wrong. Many facts here are **load-bearing**: the acceptance suite substring-matches exact
output and opens the database with raw SQL, so the strings, schema, and byte-level behavior below are
contracts, not suggestions.

## Crate layout

`rust/` is a single crate producing a single binary, `candle`. The library target exists so unit and
integration tests can drive internals directly; everything ships in the one executable:

```
rust/
  Cargo.toml                # package "candle" — one [lib] + one [[bin]]
  src/lib.rs                # module surface for tests
  src/main.rs               # entry point: picks monitor mode / MCP mode / normal CLI
  src/cli/                  # help text, hand-rolled parser, `--monitor` argument handling
  src/monitor/              # the per-service supervision loop
  src/{commands,config,db,kill,logs,log_filters,mcp,start,...}/
  tests/                    # integration tests
```

**Monitor mode.** There is no sidecar binary. Every service Candle starts is supervised by a second
`candle` process launched as `candle --monitor` — the same executable, re-invoked via
`std::env::current_exe()` (`CANDLE_MONITOR_PATH` overrides, for tests). That means installation is
one file, and the CLI and its monitors can never fall out of version sync.

## Subsystem docs

| Doc | Covers | Primary modules |
|---|---|---|
| [database.md](database.md) | SQLite schema, connection bootstrap, process/stdin tables, cleanup & eviction, stale-process cleanup, `erase-database` | `db/{mod,process_table,stdin_messages,cleanup}`, `dirs`, `process_alive`, `commands/erase_database` |
| [config.md](config.md) | `.candle.json` discovery/parse/validate, state-dir resolution, `add-service`/`remove-service`/`set-config`/`setup-project` | `config/{model,paths,validate,file,commands}`, `dirs` |
| [logs.md](logs.md) | log storage model, query builder, `logs --count` tail query, log iterator, runs (`run_id`) and latest-run filtering, `logs`/`clear-logs` | `logs/{log_type,process_logs,log_iterator,console_log}`, `log_filters/*`, `commands/{logs,clear_logs}` |
| [start-flow.md](start-flow.md) | `start`/`check-start`, the per-service start lock, the monitor handshake, transient vs configured services, success/failure detection | `start/{launch,start_one_service,start_command,service_lock}`, `monitor/{launch_info,run}`, `cli/monitor_mode`, `process_alive`, `process_tree` |
| [kill-restart.md](kill-restart.md) | `kill`/`stop`, `kill-all`, `restart`; process-tree teardown with SIGKILL escalation | `kill/*`, `commands/restart`, `process_tree` |
| [watch-wait.md](watch-wait.md) | `watch` (live tailing, agent-mode guard) and `wait-for-log` | `commands/{watch,wait_for_log}`, `logs/log_iterator`, `log_filters/*` |
| [list-ports-browser.md](list-ports-browser.md) | `list`/`list-all`, `list-ports`/`list-ports-all` (per-platform port detection), `open-browser` | `commands/{list,list_ports,open_browser}`, `listening_ports`, `process_tree` |
| [mcp.md](mcp.md) | the stdio JSON-RPC MCP server and its nine tools | `mcp/mod`, `output` |
| [cli.md](cli.md) | errors, debug logging, agent-mode detection, doc files (`list-docs`/`get-doc`), `--project-dir` scope resolution, `find-orphans`, command-name validation, version handling | `errors`, `debug`, `run_context`, `doc_files`, `project_scope`, `commands/{mod,find_orphans}`; CLI `parser`/`help` |
| [testing.md](testing.md) | the Vitest conformance harness, its `getCandleSpawn()` seam, fixtures, and CI | `../../test/*` |

## Cross-cutting conventions

**Output sink (`candle::output`).** Command handlers never call `println!`/`eprintln!`
directly; they emit through `output::out`/`output::err`. In the CLI this passes through to real
stdout/stderr. `output::capture(f)` buffers it into a `CapturedOutput` (with `stdout`/`stderr` vecs, a
`transcript()`, and `mcp_log_lines()` that prefixes stderr lines with `[stderr] `). This is what lets
the MCP server capture handler output instead of corrupting the JSON-RPC stream.

**Synchronous design.** The implementation is synchronous throughout, matching `rusqlite`'s sync
model. Line-buffered stdout/stderr readers
use threads + channels; there is no async runtime.

**Minimal, hand-rolled dependencies.** The crate depends only on `rusqlite` (bundled SQLite),
`serde`/`serde_json` (with `preserve_order` for byte-identical, key-order-preserving config
write-back), `libc` (signals, `setsid`, `flock`), and `include_dir` (embeds `docs/` for
`list-docs`/`get-doc`). The CLI argument parser, the grouped help renderer, and
the MCP JSON-RPC server are all hand-rolled rather than pulled from crates, because each must produce
exact output byte-for-byte (`Unknown argument` errors, grouped help
section headers, MCP content shapes).

## Invariants

These are the contracts the acceptance suite depends on. They are byte-level and must not drift.

- **SQLite schema is fixed.** Four tables (`processes`, `process_output`, `process_last_cleanup`,
  `stdin_messages`) with `default (strftime('%s','now'))` timestamps, autoincrement ids, a fixed column
  order, six indexes — notably `idx_process_output_lookup (project_dir, command_name, timestamp desc, id desc)` —
  a trailing nullable `run_id` column on `processes` and `process_output`, and the `process_output_assign_run`
  trigger. Migration creates missing tables and rebuilds a table that lacks a column (backfilling `run_id`).
  Several tests open `candle.db` with raw SQL, so this is a hard contract. Timestamps
  are **unix seconds** everywhere, never milliseconds. Full schema in [database.md](database.md).
- **Output strings are load-bearing.** Tests substring-match exact bytes, so brackets, backticks,
  quotes, and Unicode are reproduced verbatim — e.g. the start banner
  `[Started process '<name>'] $ <shell>` followed by `[With root directory: <dir>]`, `[Killed '<name>' process with
  PID: <pid>]`, `✓ Cleared N log entries` (U+2713), `-- showing the last N lines; use --count to see more --`. With
  `FORCE_COLOR=0` set by the harness, no ANSI is emitted.
- **Agent-mode detection** keys on the truthiness of any agent marker var — `CLAUDECODE`, `GEMINI_CLI`, `CURSOR_AGENT` (the empty string is *not* agent
  mode). Agent mode disables `watch`. See `run_context` and [watch-wait.md](watch-wait.md).
- **Monitor handshake.** The launcher sends launch-info as a single-line JSON with no trailing
  newline and then closes stdin; the monitor reads to EOF. The monitor is detached into a new session
  (`setsid`) and is never waited on, so it outlives the CLI. Getting EOF/detach wrong hangs every
  start. See [start-flow.md](start-flow.md).
- **`logCollector` is retired.** This obsolete `.candle.json` key, which once selected a log-collector
  sidecar, no longer exists. It is not a valid `set-config` key; a leftover entry in a
  config file is preserved verbatim as an unknown key and otherwise ignored.
- **Version** comes from `env!("CARGO_PKG_VERSION")`.
- **MCP stdout purity.** Only newline-delimited JSON-RPC frames reach stdout; all handler output is
  captured. Tool list, ordering, content shapes, and error codes are fixed. See [mcp.md](mcp.md).
