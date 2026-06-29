# cli-misc Subsystem — Rust Reimplementation Spec

Source files covered: `src/errors.ts`, `src/debug.ts`, `src/runContext.ts`, `src/docFiles/DocFilesHelper.ts`, `src/index.ts`, `src/cli/assertValidCommandName.ts`, `src/wait-for-log-command.ts`, plus supporting `src/findPackageJson.ts` and version handling in `src/main-cli.ts`.

---

## 1. Errors (`src/errors.ts`)

The codebase uses a **structural** error-classification convention, not `instanceof`. An error is "usage" if and only if it carries a truthy `isUsageError` property. The top-level handler tests it dynamically:

```ts
// src/main-cli.ts:539-545
export function printError(error: Error) {
  if ((error as any).isUsageError) {
    console.error(error.message);      // usage error: print message only
  } else {
    console.error(error);              // other: print full error (stack/object)
  }
}
```

### Error classes and their exact fields

All extend JS `Error`. The `.name` value is set explicitly (and notably does NOT always equal the class name — see two cases below). `super(message)` sets `.message`.

| Class | `isUsageError` | `.name` | Extra fields | Message template |
|---|---|---|---|---|
| `UsageError` | `true` | `"UsageError"` | — | (caller-supplied) |
| `ConfigFileError` | **absent** (not a usage error) | `"ConfigFileError"` | — | (caller-supplied) |
| `MissingServiceWithNameError` | `true` | `"NeedRunCommandError"` ⚠️ | `cwd: string`, `commandName: string` | `` No service '${commandName}' configured for directory: ${cwd} `` |
| `MissingSetupFileError` | `true` | `"MissingSetupFile"` ⚠️ | `cwd: string` | `` No .candle.json file found in (or above) current directory: ${cwd} `` |
| `ProcessStartFailedError` | `true` | `"ProcessStartFailedError"` | — | `` Process '${commandName}' failed to start. Recent logs: ${recentLogs.map(l => l.content).join('\n')} `` |

⚠️ **Easy to get wrong:** `MissingServiceWithNameError.name === "NeedRunCommandError"` and `MissingSetupFileError.name === "MissingSetupFile"` — the `name` strings do not match the class identifiers. Preserve these literal strings if any test or log depends on `error.name`.

`ProcessStartFailedError` constructor takes a single options object `{ commandName: string, recentLogs: ProcessLog[] }` (named/destructured, not positional). `recentLogs` joins each entry's `.content` with newlines.

`ConfigFileError` is the only one **without** `isUsageError`, so it prints with full detail.

### Rust mapping
Use one error enum (e.g. `CandleError`) with a method `is_usage_error(&self) -> bool`. Each variant carries its data (`cwd`, `command_name`, `recent_logs`). Implement `Display` to produce the exact message templates above. Keep a separate `name()` accessor returning the literal `.name` strings if needed for parity. The top-level handler should: if `is_usage_error()` print only `Display`/message to stderr; otherwise print a fuller debug representation. Exit code on any uncaught error is `1` (`process.exit(1)` in the `.catch`).

---

## 2. Debug logging (`src/debug.ts`)

```ts
export function debugLog(message: string) {
    if (process.env.CANDLE_ENABLE_LOGS) {
        fs.appendFileSync(path.join(process.cwd(), 'candle.log'), message + '\n');
    }
}
```

Behavior:
- Gated entirely on env var `CANDLE_ENABLE_LOGS` being **truthy/present** (any non-empty value enables; the JS check `if (process.env.CANDLE_ENABLE_LOGS)` is true for any non-empty string, including `"false"`). In Rust: enabled when the var is set AND its value is a non-empty string. Match JS semantics — do not parse as boolean.
- Appends `message + "\n"` to a file literally named `candle.log` in **`process.cwd()`** (the current working directory at call time, NOT the package dir, NOT the database dir).
- Synchronous append (`appendFileSync`); creates the file if missing. No flush/close management needed.
- Used widely as `debugLog('[component] ...' + JSON.stringify(...))` in log-collector and startup code. Each call is one line. Callers do their own string formatting.

### Rust mapping
`fn debug_log(msg: &str)` → if `std::env::var("CANDLE_ENABLE_LOGS").map(|v| !v.is_empty()).unwrap_or(false)`, open `cwd/candle.log` with `OpenOptions::new().create(true).append(true)` and write `msg` + `"\n"`. Resolve cwd via `std::env::current_dir()` on each call (it can change). Ignore/swallow IO errors to match the fire-and-forget nature (JS would throw, but in practice these are never wrapped — actually JS *would* propagate an exception; safest is to log-and-ignore or `expect`, but real-world cwd is always writable. Prefer silently ignoring to avoid crashing the CLI on a read-only cwd).

---

## 3. Run context (`src/runContext.ts`)

```ts
export const isRunByAgent = !!process.env.CLAUDECODE;
```

- A single module-level constant: true iff env var `CLAUDECODE` is present and non-empty (`!!` coerces non-empty string → true; empty string → false).
- Evaluated **once at module load** (process start). In Rust, compute once (e.g. `LazyLock<bool>` / `OnceLock`) reading `CLAUDECODE`.

Effects of `isRunByAgent` (from `src/main-cli.ts`):
- Help text: when true, the `watch [name...]` line is **omitted** from grouped help (`const watchLines = isRunByAgent ? '' : "...watch..."`, line 42).
- Line 435: alters behavior of some command (agent-aware branch — outside this subsystem's core but note the flag drives CLI presentation/behavior differences). Keep the flag globally accessible.

### Rust mapping
`pub fn is_run_by_agent() -> bool { static V: OnceLock<bool> = ...; *V.get_or_init(|| std::env::var("CLAUDECODE").map(|v| !v.is_empty()).unwrap_or(false)) }`.

---

## 4. DocFilesHelper (`src/docFiles/DocFilesHelper.ts`)

In-repo reimplementation of the `list-docs` / `get-doc` subset of `@facetlayer/docs-tool`. No external dependency.

### Where docs come from
Constructed in `src/main-cli.ts:36-39`:
```ts
const docFiles = new DocFilesHelper({
  dirs: [join(__packageRoot, 'docs')],     // __packageRoot = dirname(main-cli) + '/..'
  files: [join(__packageRoot, 'README.md')],
});
```
- `dirs`: scanned for `*.md` files (non-recursive, top-level only via `readdirSync`).
- `files`: explicit individual files added by basename.
- `__packageRoot` resolves relative to the running script's directory. In source mode it's `src/..` = repo root; in bundled mode it's `dist/..`. The current docs dir contents: `agents-intro.md`, `getting-started.md`, `rust-log-collector.md`, `testing-strategy.md`, `transient-processes.md`, plus `README.md`.

### Internal model
`fileMap: Map<basename, fullPath>`. Built in constructor:
1. For each dir: `readdirSync(dir)`, skip entries not ending in `.md`, set `fileMap[file] = join(dir, file)` (key = bare filename incl. `.md`).
2. For each file in `files`: `fileMap[basename(filePath)] = filePath`.

Note: `files` are added after `dirs`, so a `files` entry with the same basename overrides a dir entry. Order of map iteration = insertion order (dirs first, then files) — `listDocs` preserves this order.

### Frontmatter parsing (`parseFrontmatter`)
Regex: `^---\r?\n([\s\S]*?)\r?\n---\r?\n([\s\S]*)$`.
- If no match: `{ frontmatter: {}, content: <full text unchanged> }`.
- If match: split block on `\n`; for each line find first `:`; if none, skip line; else `key = trim(before colon)`, `value = trim(after colon)`. Only simple `key: value`; later duplicate keys overwrite earlier.
- `content` is the body **trimmed** (`.trim()`).
- Recognized keys used downstream: `name`, `description` (any others stored but unused).

⚠️ Subtle: frontmatter must be at the very start (`^---`). Handles both `\n` and `\r\n` line endings. The non-greedy `[\s\S]*?` matches the smallest first block.

### `listDocs(): DocInfo[]`
For each `[baseFilename, fullPath]` in fileMap insertion order:
- `readFileSync(fullPath, 'utf-8')`; if `ENOENT`, **silently skip** (file deleted after construction); any other read error is rethrown.
- Parse frontmatter; push `{ name: frontmatter.name || basename(baseFilename, '.md'), description: frontmatter.description || '', filename: baseFilename }`.

`DocInfo = { name, description, filename }`.

### `getDoc(name): DocContent`
1. Strip trailing `.md` from `name` → `baseName`; `filename = baseName + '.md'`.
2. Exact lookup `fileMap.get(filename)`. If found → `readDoc`.
3. Else fuzzy: `lowerBase = baseName.toLowerCase()`; filter `listDocs()` where `doc.filename.toLowerCase().includes(lowerBase)` OR `doc.name.toLowerCase().includes(lowerBase)` (substring match).
   - 0 matches → `throw new Error("Doc file not found: " + baseName)`.
   - >1 matches → `throw new Error('Multiple docs match "' + baseName + '": ' + matches.map(filename).join(', ') + '. Please be more specific.')`.
   - exactly 1 → `readDoc`.

`readDoc(filename, fullPath)` returns `DocContent = { name, description, filename, content (trimmed body), rawContent (full file), fullPath }`.

### `formatGetDocCommand(filename): string`
Determines how to tell the user to display a doc:
```ts
const subcommand = options.getDocSubcommand || 'get-doc';
const script = relative(process.cwd(), process.argv[1]);   // path to running script
const binName = basename(script);
if (binName === '.' || binName.endsWith('.js') || binName.endsWith('.mjs') || binName.endsWith('.ts'))
    return `node ${script} ${subcommand} ${filename}`;
return `${binName} ${subcommand} ${filename}`;
```
⚠️ `relative(cwd, argv[1])` can produce `'.'` (when argv[1] equals cwd) → first branch. In Rust there's no `argv[1]` script path concept; reimplement based on `std::env::args().nth(0)` (the executable path). For a real installed binary `binName = "candle"` → produces `candle get-doc <file>`. Keep the `get-doc` default subcommand.

### `printDocFileList()` — used by `list-docs`
Exact stdout format:
```
Available doc files:
<blank line>
  <name> (<getDocCommand>):
    <description>
<blank line>
```
- Header line `Available doc files:` followed by a blank line (`console.log('Available doc files:\n')` prints the header plus one extra newline).
- For each doc with a description: line 1 `  ${name} (${cmd}):`, line 2 `    ${description}\n` (trailing blank).
- For each doc with empty description: single line `  ${name} (${cmd})\n` (no colon, trailing blank).
- Two leading spaces for name line, four for description.

### `printDocFileContents(name)` — used by `get-doc`
- Calls `getDoc(name)`; on **any** throw → `console.error("Doc file not found: " + name)`, `console.error('Run with "list-docs" command to see available docs.')`, then `process.exit(1)`.
- On success: `console.log(doc.rawContent)` then `console.log("\n(File source: " + doc.fullPath + ")")`.
- ⚠️ Note it prints `rawContent` (with frontmatter), not the trimmed `content`. And error message uses the original `name` arg, not the normalized baseName.

### Rust mapping
Module `doc_files`. `parse_frontmatter(&str) -> ParsedDocument`. Struct `DocFilesHelper { file_map: IndexMap<String, PathBuf>, get_doc_subcommand: Option<String> }` (use an order-preserving map — `indexmap` crate — to match insertion-order iteration). Crate `regex` for the frontmatter regex (or hand-roll). Stdout strings must match exactly for test parity.

---

## 5. assertValidCommandName (`src/cli/assertValidCommandName.ts`)

```ts
export function assertValidCommandName(commandName: string) {
    getServiceInfoByName(commandName);
}
export function assertValidCommandNames(commandNames: string[]) {
    for (const c of commandNames) assertValidCommandName(c);
}
```

There are **no syntactic name rules**. "Valid" = resolvable via `getServiceInfoByName`. It throws if the name is neither a running process in the current project nor a configured service.

`getServiceInfoByName(commandName)` (`src/configFile.ts:373`):
1. `projectDir = findProjectDir()`.
2. Query DB: `findProcessesByCommandNameAndProjectDir(commandName, projectDir)`. If ≥1 running → return `{ commandName: proc.command_name, projectDir: proc.project_dir, runningProcess, serviceConfig? }` (config attached if found, else ignored — supports transient processes).
3. Else `getServiceConfigByName(commandName)` which:
   - `findConfigFile()` (throws `MissingSetupFileError` if no `.candle.json` found walking up).
   - exact match `config.services.find(s => s.name === commandName)`, else `findLooseCommandName(...)` (loose matching), else `throw MissingServiceWithNameError(projectDir, commandName)`.
   - returns `{ serviceConfig, projectDir }`.

So `assertValidCommandName` propagates `MissingSetupFileError` or `MissingServiceWithNameError` (both usage errors). `assertValidCommandNames` short-circuits on the first invalid name.

### Rust mapping
`fn assert_valid_command_name(name)` / `..._names(names: &[String])` delegating to `get_service_info_by_name`. Loose-matching and DB lookup live in the config/db subsystems — this module is a thin validator. Keep the fail-fast (first error wins) loop.

---

## 6. Public library API (`src/index.ts`)

This is the entrypoint exported as `@facetlayer/candle` for programmatic/GUI use. Exactly these exports:

**Process listing** (from `./list-command.ts`):
- `handleList`, `printListOutput`, `formatUptime`
- type `ListOutput`

**Process logs** (from `./logs/processLogs.ts` and filters):
- `getProcessLogs`
- type `ProcessLog`
- `LatestExecutionLogFilter` **re-exported under the alias** `AfterProcessStartLogFilter` (⚠️ name change at the API boundary — the internal class is `LatestExecutionLogFilter`).

**Configuration** (from `./configFile.ts`):
- `findConfigFile`, `getServiceConfigByName`
- types `CandleSetupConfig`, `ServiceConfig`

**Database access**:
- `getDatabase` (from `./database/database.ts`)
- `findAllProcesses`, `findProcessesByProjectDir`, `findProcessesByCommandNameAndProjectDir` (from `./database/processTable.ts`)
- type `ProcessEntry`

Note this public surface does **not** include the error classes, `debugLog`, `isRunByAgent`, `DocFilesHelper`, or `assertValidCommandName` — those are internal. In Rust, the equivalent "public crate API" is just this listing/logs/config/db set; the alias rename `LatestExecutionLogFilter → AfterProcessStartLogFilter` should be preserved if the GUI/consumer parity matters.

---

## 7. Version handling

`package.json`: `"name": "@facetlayer/candle"`, `"version": "0.13.3"`.

Two mechanisms:

1. **Early flag short-circuit** in `main()` (`src/main-cli.ts:289-295`): before any yargs/command parsing, if `process.argv` includes `-v` or `--version`, read `<scriptDir>/../package.json`, `console.log(packageJson.version)`, and `return` (exit 0). This runs *before* help handling.

2. **`findPackageJson()`** (`src/findPackageJson.ts`) — robust resolver used elsewhere (e.g. MCP). Tries `join(__dirname, '..', 'package.json')` first (bundled `dist/main-cli.js` layout), then `join(__dirname, '..', '..', 'package.json')` (source `src/mcp/mcp-main.ts` layout). Throws `Error("Could not find package.json at <a> or <b>")` if neither exists. Returns `{ name, version }`.

3. yargs `.version()` (line 234) is configured but the manual check at line 290 wins because it fires first.

⚠️ Two different path-resolution strategies coexist: the inline `--version` handler only checks `../package.json` (one level up from the script dir), while `findPackageJson` checks two candidate depths. A Rust port should compile the version in at build time (`env!("CARGO_PKG_VERSION")`) rather than reading `package.json` at runtime — simpler and avoids the dual-path fragility. Output is the bare version string + newline on stdout, exit 0.

---

## 8. wait-for-log command (`src/wait-for-log-command.ts`)

`handleWaitForLog(options)` polls logs until a target substring appears or timeout. Belongs partly to the logs subsystem but the control flow is self-contained.

Constants: `POLL_INTERVAL = 200` (ms), `LOG_COUNT_SEARCH_LIMIT = 1000`.

Options: `{ projectDir: string, commandNames: string[], message: string, timeoutMs?: number }`, default `timeoutMs = 30000`.

Algorithm:
1. Create `LogIterator({ projectDir, commandNames, limit: 1000 })`; `allInitialLogs = getNextLogs()`.
2. Create `LatestExecutionLogFilter({ showPastLogsBehavior: 'only_show_after_recent_launch' })`; call `checkLatestLaunchStatus(allInitialLogs)`; `initialLogs = filter.filter(allInitialLogs)`.
3. If `initialLogs.length === 0` → return `{ success: false, message: 'Process has not started yet' }` (no console output).
4. If none of initialLogs has `log_type === ProcessLogType.process_start_initiated` → `console.error("Process has not started yet")`, return `{ success: false }`.
5. Scan initialLogs: if any `log.content?.includes(message)` → `console.log('Found message "<message>" in existing logs.')`, return `{ success: true }`.
6. Poll loop (`timeStarted = Date.now()`):
   - If elapsed > timeoutMs → `console.log('wait-for-log failed: Timed out after <timeoutMs>ms and message "<message>" not found.')`, `printRecentLogs(...)`, return `{ success: false }`.
   - `rawLogs = logIterator.getNextLogs()`; `logs = logFilter.filter(rawLogs)`.
   - For each log: if `content.includes(message)` → `console.log('Found message "<message>" in logs.')`, return `{ success: true }`. If `log_type === process_exited` → `console.log('wait-for-log failed: Process exited before finding message "<message>"')`, `printRecentLogs(...)`, return `{ success: false }`.
   - `await sleep(200)`; repeat.

`printRecentLogs(projectDir, commandNames)`: prints `Recent logs for '<commandNames joined by ", ">':`, then fetches `getProcessLogs({ commandNames, limit: 100, projectDir })`, filters with a fresh `LatestExecutionLogFilter({ showPastLogsBehavior: 'only_show_after_recent_launch' })`, and prints each via `consoleLogRow(log, { format: 'pretty' })`.

`ProcessLogType` values referenced: `process_start_initiated`, `process_exited` (these are the relevant enum members — confirm full enum in `src/logs/ProcessLogType.ts` when porting the logs subsystem).

⚠️ Subtleties:
- `content?.includes` — content may be null/undefined; substring match is plain `String.includes` (not regex).
- The exact output strings (with embedded quotes around `message`) are likely asserted by tests — reproduce verbatim.
- Polling uses wall-clock `Date.now()`; use `Instant`/`SystemTime` in Rust with a 200ms sleep.
- The `LogIterator` is stateful — `getNextLogs()` returns only logs newer than the last call (cursor). The filter (`logFilter`) is also stateful across calls in the loop (same instance reused), distinct from `printRecentLogs` which creates a fresh filter.

### Rust mapping
`async fn handle_wait_for_log(opts) -> WaitForLogResult { success: bool, message: Option<String> }`. Depends on the logs subsystem (`LogIterator`, `LatestExecutionLogFilter`, `get_process_logs`, `console_log_row`, `ProcessLogType`).

---

## 9. External npm dependencies & Rust crate equivalents

| npm / Node API | Use here | Rust equivalent |
|---|---|---|
| `node:fs` (`appendFileSync`, `readFileSync`, `readdirSync`, `existsSync`) | debug log, doc reading, package.json | `std::fs` |
| `node:path` (`join`, `basename`, `relative`, `dirname`) | path building | `std::path::{Path, PathBuf}` |
| `node:url` (`fileURLToPath`) | resolve module dir | n/a (use `std::env::current_exe()` / build-time paths) |
| frontmatter regex | `parseFrontmatter` | `regex` crate |
| insertion-ordered `Map` | `fileMap` iteration order | `indexmap` crate |
| `process.env` | env reads | `std::env::var` |
| `yargs` (`.version()`) | CLI parsing/version | `clap` |
| `setTimeout`/promises | poll loop | `tokio::time::sleep` |

No third-party npm deps in this subsystem itself — `@facetlayer/docs-tool` was intentionally removed and replaced by the in-repo `DocFilesHelper` (per the file header comment).

---

## 10. Rust reimplementation notes (modules / functions & ordering)

Build bottom-up; later items depend on earlier ones.

1. **`error` module** — `CandleError` enum with variants `Usage(String)`, `ConfigFile(String)`, `MissingServiceWithName{cwd, command_name}`, `MissingSetupFile{cwd}`, `ProcessStartFailed{command_name, recent_logs}`. Implement `Display` (exact message templates), `is_usage_error() -> bool` (true for all except `ConfigFile`), and `name() -> &str` (literal `.name` strings incl. `"NeedRunCommandError"`, `"MissingSetupFile"`). No deps.

2. **`debug` module** — `debug_log(&str)` gated on non-empty `CANDLE_ENABLE_LOGS`, append to `cwd/candle.log`. No deps.

3. **`run_context` module** — `is_run_by_agent() -> bool` (`OnceLock`, env `CLAUDECODE` non-empty). No deps.

4. **`version`** — prefer `env!("CARGO_PKG_VERSION")`; provide `find_package_json()`-equivalent only if runtime JSON parity is required. Handle `-v`/`--version` before command dispatch, print bare version + exit 0.

5. **`doc_files` module** — `parse_frontmatter`, `DocFilesHelper` (fields, `format_get_doc_command`, `list_docs`, `get_doc`, `read_doc`, `print_doc_file_list`, `print_doc_file_contents`). Depends on `regex`, `indexmap`. Stdout strings must match exactly.

6. **`assert_valid_command_name`** — thin wrappers over `get_service_info_by_name` (in config/db subsystem). Depends on config + db modules (out of scope here) and on `error` module.

7. **`wait_for_log`** — `handle_wait_for_log`. Depends on logs subsystem (`LogIterator`, `LatestExecutionLogFilter`, `get_process_logs`, `console_log_row`, `ProcessLogType`) plus `tokio` for the 200ms poll loop.

8. **Public crate API surface** — re-export the equivalents of `index.ts`: list-command (`handle_list`, `print_list_output`, `format_uptime`, `ListOutput`), logs (`get_process_logs`, `ProcessLog`, `LatestExecutionLogFilter` aliased as `AfterProcessStartLogFilter`), config (`find_config_file`, `get_service_config_by_name`, `CandleSetupConfig`, `ServiceConfig`), db (`get_database`, `find_all_processes`, `find_processes_by_project_dir`, `find_processes_by_command_name_and_project_dir`, `ProcessEntry`). Do NOT expose error classes, debug, run_context, doc_files, or assert helpers publicly.

Cross-subsystem dependencies (config file resolution, DB process table, log filters/iterator, `ProcessLogType` enum) are referenced but defined outside cli-misc — port those subsystems first for items 6 and 7.