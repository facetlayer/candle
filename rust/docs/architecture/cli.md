# CLI & miscellaneous subsystem

Covers errors, debug logging, run context, doc files, project-scope resolution, `find-orphans`, command-name validation, the library module surface, version handling, and the `wait-for-log` command.

The implementation lives in `rust/src/` — `errors.rs`, `debug.rs`, `run_context.rs`, `doc_files.rs`, `commands/mod.rs` (command-name validation), `commands/wait_for_log.rs`, and `lib.rs` (module surface) — with CLI dispatch in `main.rs` and help/parsing in `cli/{help,parser}.rs`.

## 1. Errors (`errors.rs`)

A single error enum, `CandleError`:

| Variant | Fields | `Display` message |
|---|---|---|
| `UsageError` | `String` | (caller-supplied) |
| `ConfigFileError` | `String` | (caller-supplied) |
| `MissingServiceWithName` | `command_name`, `cwd` | `No service '<command_name>' configured for directory: <cwd>` |
| `MissingSetupFile` | `cwd`, `explicit: bool` | Discovery: `No .candle.json file found in (or above) current directory: <cwd>` plus a second line suggesting `candle add-service <name> --shell <cmd>` or `candle setup-project`. `--project-dir` (`explicit`): `No .candle.json in <cwd> (--project-dir doesn't search parent directories)` |
| `ProcessStartFailed` | `command_name`, `recent_logs: String` | `Process '<command_name>' failed to start. Recent logs:\n<recent_logs>`, or just `Process '<command_name>' failed to start.` when `recent_logs` is empty |
| `Generic` | `String` | (caller-supplied) — timeouts, launch/IO failures; `rusqlite::Error` converts to `Generic("database error: <e>")` |

`recent_logs` is already the joined content string (blank/content-less rows dropped). `CandleError::unknown_service(name, project_dir)` builds the `MissingServiceWithName` error; every unknown-name check (CLI and MCP) goes through it so the text is identical everywhere.

There is no usage/non-usage distinction at print time. The top-level handler in `main.rs` (`fail_with` → `fatal`) prints every error the same way: `error_line(&err.to_string())` to stderr, then exit `1`. `error_line` prefixes the first line with `Error: ` (`ERROR_PREFIX`) unless it already starts with it, so a message can never read `Error: Error: ...`; following lines (hints, recent logs) are kept as-is. There is no stack/debug form.

## 2. Debug logging (`debug.rs`)

`fn debug_log(msg: &str)`:
- Gated on env var `CANDLE_ENABLE_LOGS` being set to a **non-empty** value. The value is not parsed as a boolean, so `CANDLE_ENABLE_LOGS=false` also enables it.
- Appends `msg + "\n"` to a file literally named `candle.log` in the **current working directory** at call time (NOT the install dir, NOT the database dir). cwd is resolved via `std::env::current_dir()` on each call (it can change).
- Opens with `OpenOptions::new().create(true).append(true)`, so the file is created if missing.
- IO errors are swallowed, so a read-only cwd never crashes the CLI.
- Callers are the monitor (`monitor/run.rs`), as `debug_log(&format!("[monitor] ..."))`. Each call is one line; callers do their own formatting.

## 3. Run context (`run_context.rs`)

- `is_run_by_agent()`: true iff **any** of the agent marker env vars in `AGENT_ENV_VARS` — `CLAUDECODE` (Claude Code), `GEMINI_CLI` (Gemini CLI), or `CURSOR_AGENT` (Cursor) — is present and non-empty (empty string → false). Codex's `CODEX_SANDBOX` is deliberately excluded: it marks an active sandbox, not the agent, and is unset under `--sandbox danger-full-access`.
- Evaluated **once**: `static CACHE: OnceLock<bool>`, initialized with `detect_agent(|n| std::env::var(n).ok())`. `detect_agent` takes the environment as a lookup so it can be tested without mutating the process env.

Effects of `is_run_by_agent()` in the CLI:
- Help text: when true, the `watch [name...]` line is **omitted** from grouped help (`cli/help.rs`).
- `cmd_watch` refuses to run (stderr `Error: 'watch' blocks and is not available in agent mode. Use 'candle logs' to view process output.`, exit 1).
- It feeds `is_interactive()` = `!is_run_by_agent() && stdout is a TTY`, which decides whether `start`/`restart` watch logs after launching (overridable with `--watch` / `--bg`).

## 4. Doc files (`doc_files.rs`)

Backs the `list-docs` / `get-doc` commands. The module has free functions, not a helper struct.

### Where docs come from
The docs are **embedded at compile time** with `include_dir!("$CARGO_MANIFEST_DIR/../docs")` plus `include_str!("../../README.md")`, so the binary is relocatable and never reads the filesystem for docs.
- `all_docs()`: the embedded top-level `*.md` files **sorted by filename**, then `README.md` last.
- Only files directly in `docs/` are served (`agents-intro.md`, `mcp-usage.md`, `project-setup.md`, `transient-processes.md`); developer docs under `docs/dev/` (e.g. `testing-strategy.md`) are excluded from both commands.

### Frontmatter parsing (`parse_frontmatter`)
`parse_frontmatter(&str) -> (name, description, content)`, hand-written (no `regex`):
- Normalizes `\r\n` to `\n`, then requires a leading `---\n` and a closing `\n---\n` (the first one wins).
- For each line of the block, splits on the first `:`; lines without one are skipped. Key and value are trimmed. Only `name` and `description` are read; other keys are ignored, and a later duplicate overwrites an earlier one.
- With frontmatter, `content` is the body **trimmed**. Without it, `(None, None, <full text unchanged>)`.

### `list_docs() -> Vec<DocInfo { name, description, filename }>`
For each doc in `all_docs()` order: `name` = frontmatter `name`, else the filename stem; `description` = frontmatter `description`, else `""` (the README, which has no frontmatter, gets `README_DESCRIPTION`: `Full reference for every command (the project README)`).

### `get_doc(name) -> Result<DocContent { filename, source_path, content }, DocLookupError::NotFound>`
Lookup is **exact** apart from letter case: the trimmed, lowercased name (optional `.md` suffix stripped) must equal a doc's filename stem or its frontmatter `name`. There is no substring or prefix matching, so `get-doc project` does not resolve to `project-setup`. An empty name is `NotFound`. `content` has the frontmatter stripped. `source_path` is `README.md` for the README (it lives at the repo root) and `docs/<filename>` otherwise.

### Printing (`main.rs`)
- `cmd_list_docs`: prints `Available docs (show one with 'candle get-doc <name>'):` and a blank line, then one line per doc: `  <name padded to the longest name>  <description>`, or just `  <name>` when the description is empty.
- `cmd_get_doc`: a missing/blank name is fatal (`Error: get-doc requires a <name>` + `Run "candle list-docs" to see available docs.`). On success it prints `content`, then `\n(File source: <source_path>)`. `NotFound` prints `Error: Doc file not found: <name>` and `Run with "list-docs" command to see available docs.` to stderr, exit 1. The error echoes the name as given, not the normalized form.

## 4b. Project scope (`project_scope.rs`)

`ProjectScope` is how a command decides which project directory it acts on. It has two variants:

- `Discover(cwd)` — the default. `resolve()` walks up from the CWD to the nearest config file.
- `Explicit(dir)` — `--project-dir <dir>`. The named directory *is* the project. `resolve()` returns
  it verbatim: no ancestor walk, no existence check.

The flag value is made absolute against the CWD and lexically normalized (`dirs::normalize_path`),
so `--project-dir .` produces the same string discovery would, and the result matches the
`project_dir` column on existing rows. Normalization is textual — the path is never canonicalized,
because it may no longer exist.

### The two checks, and why they differ

`resolve()` never fails for an explicit dir. That is deliberate: `candle kill --project-dir
/gone/project` is the intended way to clean up after a project that has been deleted, so the
DB-keyed commands (`kill`, `logs`, `clear-logs`, `wait-for-log`) call only `resolve()`.

`require_own_config()` is the second check, for commands that need service definitions (`start`,
`restart`, `list`, `ps`, `watch`, `list-ports`, `open-browser`). It requires an
explicit dir to contain a config file itself. Without it, config discovery would walk up and resolve
services from an *ancestor* project while the process rows stayed keyed to the directory the user
named — one command silently acting on two projects. For `Discover` it is always `Ok`, since walking
up is the point there.

`kill` additionally skips `assert_valid_command_names` under an explicit scope: there is no config
left to validate against, so an unknown name reports "No running processes found" rather than
failing.

`list-all`, `list-ports-all`, `kill-all`, `find-orphans`, and `erase-database` do not accept the flag at all — the
parser rejects it as `Unknown argument` — because they are already system-wide. (`erase-database`'s only flag is `--force`; see [database.md](database.md) §10.)

## 4c. find-orphans (`commands/find_orphans.rs`)

A system-wide diagnostic in the same family as `kill-all`: it reports every *live* tracked
process whose project no longer accounts for it. `classify(project_dir, service_name, transient)` returns the
first applicable `OrphanReason`:

| Reason | Condition |
|---|---|
| `MissingProjectDir` | `project_dir` is not a directory |
| `MissingConfigFile` | no name in `CONFIG_FILENAMES` exists **in that directory** (ancestors deliberately don't count — an ancestor's config describes a different project) |
| `ServiceNotInConfig` | the config parses but has no service by that name, and the row is not `transient` |

A transient process (row `transient = 1`, started with `--shell`) is never in the config, so it is
only orphaned by the first two reasons.

A config file that exists but fails to parse yields `None` (not an orphan): it almost certainly still
lists the service, and reporting it would invite killing a healthy process over a JSON typo.

Only rows that pass `filter_alive_processes` are considered — a dead row is stale bookkeeping for the
reaper, not an orphan. Output is the human report from `format_find_orphans` or, with `--json`, the
serialized `FindOrphansOutput` (reasons serialize camelCase: `missingProjectDir`,
`missingConfigFile`, `serviceNotInConfig`).

## 5. Command-name validation (`commands/mod.rs`)

There are **no syntactic name rules**. "Valid" = the name is known to the project.

`assert_valid_command_names(conn, cwd, names) -> Result<(), CandleError>`: empty `names` → `Ok`. Otherwise `project_dir = find_project_dir(cwd)?` (so a missing config propagates `MissingSetupFile`), and for each name: if `find_processes_by_command_name_and_project_dir` returns **any** row (running or killed, so transient names pass), continue; else `get_service_config_by_name(name, Some(cwd))?` (exact match, then loose matching; see [config.md](config.md) §9), which fails with `MissingServiceWithName`. Fail-fast: the first error wins. Callers: `kill` (skipped under an explicit `--project-dir`), `restart`, `watch`, and the MCP kill tool.

`assert_known_service_names(conn, config_dir, project_dir, names, check_config)` is the variant for commands that read stored logs: a name is known if it has stored logs (`has_logs_for_command`) or a process row in `project_dir`, or — when `check_config` is set — resolves via `get_service_config_by_name`. Otherwise it fails with `CandleError::unknown_service`. `assert_known_service_names_in_scope(conn, scope, project_dir, names)` sets `check_config` from `scope.require_own_config().is_ok()`, so a `--project-dir` whose config is gone is still checked against stored rows. Used by `logs`, `wait-for-log`, `clear-logs`, and the MCP `GetLogs` tool.

## 6. Library surface (`lib.rs`)

`lib.rs` is **not** a curated public API: it declares every module `pub` (`cli`, `commands`, `config`, `db`, `debug`, `dirs`, `doc_files`, `errors`, `kill`, `listening_ports`, `log_filters`, `logs`, `mcp`, `monitor`, `output`, `process_alive`, `process_tree`, `project_scope`, `run_context`, `start`) so the integration tests in `rust/tests/` can reach internals, and re-exports nothing at the crate root. The `candle` binary (`main.rs`) is the only executable.

## 7. Version handling

The version is compiled in at build time via `env!("CARGO_PKG_VERSION")` (`help::version()` in `rust/src/cli/help.rs`, from `rust/Cargo.toml`); nothing is read at runtime. In `main()`, `-v`/`--version` anywhere in argv is handled right after the `--monitor` check and before help and command dispatch; output is the bare version string + newline on stdout, exit 0. The grouped help header uses the same `version()`.

## 8. wait-for-log command (`commands/wait_for_log.rs`)

`handle_wait_for_log(conn, project_dir, command_names, message, timeout_ms) -> WaitForLogResult { success: bool }` polls logs until a target substring appears, the run ends, or the timeout is hit. Synchronous: `std::thread::sleep` between polls, no async runtime.

Constants: `POLL_INTERVAL = 200` (ms), `LOG_COUNT_SEARCH_LIMIT = 1000`, `RECENT_LOG_LINES = 20`, `LIVENESS_CHECK_EVERY = 5` (polls).

Outline (full flow in [watch-wait.md](watch-wait.md) §8):
1. Seed a `LatestRunFilter` with each service's latest run, create `LogIterator::with_limit(.., Some(1000))`, and filter the first batch. If any row's content contains `message` (plain substring, `None` content never matches) → stdout `Found message "<message>" in existing logs.`, success. A run that already finished still counts.
2. Decide from the latest run's lifecycle rows (`latest_run_ids`, `get_process_logs` with `latest_launch_only`) and liveness (`find_running_processes_by_project_dir` + `filter_alive_processes`): never launched and nothing running → fail at once without logs; latest run already ended (`process_exited` / `process_start_failed`) and nothing running → fail at once with recent logs. The failure line is `<Service '<name>' is not running | Services '<a, b>' are not running | No service in this project is running> and message "<message>" was not found.` on stderr.
3. Poll loop: once the start has been reported, every `LIVENESS_CHECK_EVERY` polls re-check that something is still running (else the same not-running failure). Past the timeout → stderr `Timed out after <timeout_ms>ms and message "<message>" not found.` plus recent logs. Each new filtered row: containing `message` → stdout `Found message "<message>" in logs.`, success; a `process_exited` / `process_start_failed` row → stderr `Process exited before finding message "<message>"` plus recent logs.

`print_recent_logs` prints `get_log_tail`'s newest 20 printable rows of the latest run (header `Last 20 lines of the latest run of <subject>:` when truncated, else `Logs from the latest run of <subject>:`), each via `console_log_row` in pretty format, then `Run 'candle logs [<name>]' to see more.`

The `LogIterator` and the filter are stateful across polls: `get_next_logs()` returns only rows newer than its cursor. The exact output strings (with embedded quotes around `message`) are asserted by tests.

`cmd_wait_for_log` in `main.rs`: `--message` is required (else `Error: --message <text> is required`, exit 1); `--timeout` is in **seconds** (default 30, fractional allowed, must be finite and non-negative) and converted to ms; names are validated with `assert_known_service_names_in_scope` (unknown → `No service '<name>' configured for directory: <dir>`, exit 1); a `success: false` result exits 1.
