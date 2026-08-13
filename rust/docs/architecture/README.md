# Candle (Rust) — architecture reference

This directory documents how the Rust implementation of the `candle` CLI is built. The Rust code
under `rust/` is a drop-in reimplementation of the original Node/TypeScript CLI in `../../src`, which
remains the published npm package and the behavioral source of truth. The existing Vitest suite in
`../../test` runs against both implementations and is the conformance harness (see
[testing.md](testing.md)).

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
| [database.md](database.md) | SQLite schema, connection bootstrap, process/stdin tables, cleanup & eviction, stale-process cleanup | `db/{mod,process_table,stdin_messages,cleanup}`, `dirs`, `process_alive` |
| [config.md](config.md) | `.candle.json` discovery/parse/validate, state-dir resolution, `add-service`/`remove-service`/`set-config`/`setup-project` | `config/{model,paths,validate,file,commands}`, `dirs` |
| [logs.md](logs.md) | log storage model, query builder, log iterator, latest-execution filtering, `logs`/`clear-logs` | `logs/{log_type,process_logs,log_iterator,console_log}`, `log_filters/*`, `commands/{logs,clear_logs}` |
| [start-flow.md](start-flow.md) | `start`/`check-start`, the monitor handshake, transient vs configured services, success/failure detection | `start/{launch,start_one_service,start_command}`, `monitor/{launch_info,run}`, `process_alive`, `process_tree` |
| [kill-restart.md](kill-restart.md) | `kill`/`stop`, `kill-all`, `restart`; process-tree teardown | `kill/*`, `commands/restart`, `process_tree` |
| [watch-wait.md](watch-wait.md) | `watch` (live tailing, agent-mode guard) and `wait-for-log` | `commands/{watch,wait_for_log}`, `logs/log_iterator`, `log_filters/*` |
| [list-ports-browser.md](list-ports-browser.md) | `list`/`list-all`, `list-ports`/`list-ports-all` (lsof parsing), `open-browser` | `commands/{list,list_ports,open_browser}`, `process_tree` |
| [mcp.md](mcp.md) | the stdio JSON-RPC MCP server and its nine tools | `mcp/mod`, `output` |
| [cli.md](cli.md) | errors, debug logging, agent-mode detection, doc files (`list-docs`/`get-doc`), command-name validation, version handling | `errors`, `debug`, `run_context`, `doc_files`, `commands/mod`; CLI `parser`/`help` |
| [testing.md](testing.md) | the Vitest conformance harness, the `CANDLE_TEST_TARGET` switch, and CI | `../../test/*` |

## Cross-cutting conventions

**Output sink (`candle::output`).** Command handlers never call `println!`/`eprintln!`
directly; they emit through `output::out`/`output::err`. In the CLI this passes through to real
stdout/stderr. `output::capture(f)` buffers it into a `CapturedOutput` (with `stdout`/`stderr` vecs, a
`transcript()`, and `mcp_log_lines()` that prefixes stderr lines with `[stderr] `). This is what lets
the MCP server capture handler output instead of corrupting the JSON-RPC stream, without
monkeypatching a global console.

**Synchronous design.** The implementation is synchronous throughout, mirroring `rusqlite`'s sync
model and the original TypeScript's synchronous SQLite semantics. Line-buffered stdout/stderr readers
use threads + channels; there is no async runtime.

**Minimal, hand-rolled dependencies.** The shared crate depends only on `rusqlite` (bundled SQLite),
`serde`/`serde_json` (with `preserve_order` for byte-identical, key-order-preserving config
write-back), and `libc` (signals/`setsid`). The CLI argument parser, the grouped help renderer, and
the MCP JSON-RPC server are all hand-rolled rather than pulled from crates, because each must match
the original's exact output byte-for-byte (yargs-style `Unknown argument` errors, grouped help
section headers, MCP content shapes).

## Parity invariants

These are the contracts the Rust implementation maintains so the shared acceptance suite passes
against it. They are byte-level and must not drift.

- **SQLite schema is byte-identical** to the Node database (same DDL including
  `default (strftime('%s','now'))`, autoincrement, column order, and all four indexes — notably
  `idx_process_output_lookup (project_dir, command_name, timestamp desc, id desc)`). Migration is
  additive only. Several tests open `candle.db` with raw SQL, so this is a hard contract. Timestamps
  are **unix seconds** everywhere, never milliseconds. Full schema in [database.md](database.md).
- **Output strings are load-bearing.** Tests substring-match exact bytes, so brackets, backticks,
  quotes, and Unicode are reproduced verbatim — e.g. the start banner
  `` [Started process '<name>' (`<shell>`) in directory: '<dir>'] ``, `[Killed '<name>' process with
  PID: <pid>]`, `✓ Cleared N log entries` (U+2713), `-- older logs have been removed --`. With
  `FORCE_COLOR=0` set by the harness, no ANSI is emitted.
- **Agent-mode detection** keys on the truthiness of `CLAUDECODE` (the empty string is *not* agent
  mode). Agent mode disables `watch`. See `run_context` and [watch-wait.md](watch-wait.md).
- **Monitor handshake.** The launcher sends launch-info as a single-line JSON with no trailing
  newline and then closes stdin; the monitor reads to EOF. The monitor is detached into a new session
  (`setsid`) and is never waited on, so it outlives the CLI. Getting EOF/detach wrong hangs every
  start. See [start-flow.md](start-flow.md).
- **`logCollector` is retired.** The `.candle.json` key that chose between the Node and Rust
  collector sidecars no longer exists. It is not a valid `set-config` key; a leftover entry in a
  config file is preserved verbatim as an unknown key and otherwise ignored.
- **Version** comes from `env!("CARGO_PKG_VERSION")`.
- **MCP stdout purity.** Only newline-delimited JSON-RPC frames reach stdout; all handler output is
  captured. Tool list, ordering, content shapes, and error codes match the Node server exactly. See
  [mcp.md](mcp.md).

## Relationship to the Node implementation

The Rust modules mirror the Node subsystems in `../../src`, and the subsystem docs cross-reference the
original `src/...` files as the behavioral source of truth. The Node implementation is retained and
still published; the Rust port reaches parity through the shared Vitest suite rather than by sharing
any code. When the two disagree, the test suite — and the Node behavior it encodes — is authoritative.
