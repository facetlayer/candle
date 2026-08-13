# CLI & miscellaneous subsystem

Covers errors, debug logging, run context, doc files, command-name validation, the public library API, version handling, and the `wait-for-log` command.

The Rust implementation lives in `rust/src/` — `errors.rs`, `debug.rs`, `run_context.rs`, `doc_files.rs`, `commands/mod.rs` (command-name validation), `commands/wait_for_log.rs`, and `lib.rs` (module surface) — with CLI dispatch in `main.rs` and help/parsing in `cli/{help,parser}.rs`. It mirrors the original `src/errors.ts`, `src/debug.ts`, `src/runContext.ts`, `src/docFiles/DocFilesHelper.ts`, `src/index.ts`, `src/cli/assertValidCommandName.ts`, `src/wait-for-log-command.ts`, plus `src/findPackageJson.ts` and version handling in `src/main-cli.ts`.

## 1. Errors (`errors.rs`, mirrors `src/errors.ts`)

The codebase uses a **structural** error-classification convention, not `instanceof`. An error is "usage" if and only if it carries a truthy `isUsageError` property. The original top-level handler tests it dynamically:

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

In the Node original all extend JS `Error`. The `.name` value is set explicitly (and notably does NOT always equal the class name — see two cases below). `super(message)` sets `.message`.

| Class | `isUsageError` | `.name` | Extra fields | Message template |
|---|---|---|---|---|
| `UsageError` | `true` | `"UsageError"` | — | (caller-supplied) |
| `ConfigFileError` | **absent** (not a usage error) | `"ConfigFileError"` | — | (caller-supplied) |
| `MissingServiceWithNameError` | `true` | `"NeedRunCommandError"` ⚠️ | `cwd: string`, `commandName: string` | `` No service '${commandName}' configured for directory: ${cwd} `` |
| `MissingSetupFileError` | `true` | `"MissingSetupFile"` ⚠️ | `cwd: string` | `` No .candle.json file found in (or above) current directory: ${cwd} `` |
| `ProcessStartFailedError` | `true` | `"ProcessStartFailedError"` | — | `` Process '${commandName}' failed to start. Recent logs: ${recentLogs.map(l => l.content).join('\n')} `` |

⚠️ **Easy to get wrong:** `MissingServiceWithNameError.name === "NeedRunCommandError"` and `MissingSetupFileError.name === "MissingSetupFile"` — the `name` strings do not match the class identifiers. These literal strings are preserved because tests and logs may depend on `error.name`.

`ProcessStartFailedError` is constructed from a single options object `{ commandName: string, recentLogs: ProcessLog[] }` (named/destructured, not positional). `recentLogs` joins each entry's `.content` with newlines.

`ConfigFileError` is the only one **without** `isUsageError`, so it prints with full detail.

### Rust implementation
A single error enum (`CandleError`) with a method `is_usage_error(&self) -> bool`. Each variant carries its data (`cwd`, `command_name`, `recent_logs`). `Display` produces the exact message templates above. A separate `name()` accessor returns the literal `.name` strings for parity. The top-level handler: if `is_usage_error()`, print only the `Display`/message to stderr; otherwise print a fuller debug representation. Exit code on any uncaught error is `1` (matching `process.exit(1)` in the original `.catch`).

## 2. Debug logging (`debug.rs`, mirrors `src/debug.ts`)

The Node original:

```ts
export function debugLog(message: string) {
    if (process.env.CANDLE_ENABLE_LOGS) {
        fs.appendFileSync(path.join(process.cwd(), 'candle.log'), message + '\n');
    }
}
```

Behavior:
- Gated entirely on env var `CANDLE_ENABLE_LOGS` being **truthy/present** (any non-empty value enables; the JS check `if (process.env.CANDLE_ENABLE_LOGS)` is true for any non-empty string, including `"false"`). The Rust code enables when the var is set AND its value is a non-empty string — matching JS semantics, not parsing as boolean.
- Appends `message + "\n"` to a file literally named `candle.log` in the **current working directory** at call time (NOT the package dir, NOT the database dir).
- Synchronous append; creates the file if missing. No flush/close management needed.
- Used widely as `debugLog('[component] ...' + JSON.stringify(...))` in monitor and startup code. Each call is one line. Callers do their own string formatting.

### Rust implementation
`fn debug_log(msg: &str)` → if `std::env::var("CANDLE_ENABLE_LOGS").map(|v| !v.is_empty()).unwrap_or(false)`, open `cwd/candle.log` with `OpenOptions::new().create(true).append(true)` and write `msg` + `"\n"`. cwd is resolved via `std::env::current_dir()` on each call (it can change). IO errors are swallowed to match the fire-and-forget nature — silently ignored so the CLI never crashes on a read-only cwd.

## 3. Run context (`run_context.rs`, mirrors `src/runContext.ts`)

The Node original:

```ts
export const isRunByAgent = !!process.env.CLAUDECODE;
```

- A single cached value: true iff **any** of the agent marker env vars `CLAUDECODE` (Claude Code), `GEMINI_CLI` (Gemini CLI), or `CURSOR_AGENT` (Cursor) is present and non-empty (empty string → false). Codex's `CODEX_SANDBOX` is deliberately excluded: it marks an active sandbox, not the agent, and is unset under `--sandbox danger-full-access`.
- Evaluated **once at process start**. The Rust code computes it once via `OnceLock`, reading the marker vars.

Effects of `isRunByAgent` (from the CLI dispatch, originally `src/main-cli.ts`):
- Help text: when true, the `watch [name...]` line is **omitted** from grouped help (`const watchLines = isRunByAgent ? '' : "...watch..."`, line 42).
- Line 435: alters behavior of an agent-aware command branch — outside this subsystem's core, but the flag drives CLI presentation/behavior differences. The flag is globally accessible.

### Rust implementation
`pub fn is_run_by_agent() -> bool { static V: OnceLock<bool> = ...; *V.get_or_init(|| detect_agent(|n| std::env::var(n).ok())) }`, where `detect_agent` tests each name in `AGENT_ENV_VARS` for a non-empty value.

## 4. DocFilesHelper (`doc_files.rs`, mirrors `src/docFiles/DocFilesHelper.ts`)

An in-repo implementation of the `list-docs` / `get-doc` subset of `@facetlayer/docs-tool`. No external dependency.

### Where docs come from
In the Node original, constructed in `src/main-cli.ts:36-39`:
```ts
const docFiles = new DocFilesHelper({
  dirs: [join(__packageRoot, 'docs')],     // __packageRoot = dirname(main-cli) + '/..'
  files: [join(__packageRoot, 'README.md')],
});
```
- `dirs`: scanned for `*.md` files (non-recursive, top-level only).
- `files`: explicit individual files added by basename.
- `__packageRoot` resolves relative to the running script's directory. The docs dir contents: `agents-intro.md`, `getting-started.md`, `testing-strategy.md`, `transient-processes.md`, plus `README.md`.

### Internal model
`fileMap: Map<basename, fullPath>`. Built in the constructor:
1. For each dir: read entries, skip those not ending in `.md`, set `fileMap[file] = join(dir, file)` (key = bare filename incl. `.md`).
2. For each file in `files`: `fileMap[basename(filePath)] = filePath`.

Note: `files` are added after `dirs`, so a `files` entry with the same basename overrides a dir entry. Order of map iteration = insertion order (dirs first, then files) — `listDocs` preserves this order.

### Frontmatter parsing (`parseFrontmatter`)
Regex: `^---\r?\n([\s\S]*?)\r?\n---\r?\n([\s\S]*)$`.
- If no match: `{ frontmatter: {}, content: <full text unchanged> }`.
- If match: split block on `\n`; for each line find first `:`; if none, skip line; else `key = trim(before colon)`, `value = trim(after colon)`. Only simple `key: value`; later duplicate keys overwrite earlier.
- `content` is the body **trimmed**.
- Recognized keys used downstream: `name`, `description` (any others stored but unused).

⚠️ Subtle: frontmatter must be at the very start (`^---`). Handles both `\n` and `\r\n` line endings. The non-greedy `[\s\S]*?` matches the smallest first block.

### `listDocs(): DocInfo[]`
For each `[baseFilename, fullPath]` in fileMap insertion order:
- Read the file as UTF-8; if not found (`ENOENT`), **silently skip** (file deleted after construction); any other read error is propagated.
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
Determines how to tell the user to display a doc. The Node original:
```ts
const subcommand = options.getDocSubcommand || 'get-doc';
const script = relative(process.cwd(), process.argv[1]);   // path to running script
const binName = basename(script);
if (binName === '.' || binName.endsWith('.js') || binName.endsWith('.mjs') || binName.endsWith('.ts'))
    return `node ${script} ${subcommand} ${filename}`;
return `${binName} ${subcommand} ${filename}`;
```
⚠️ `relative(cwd, argv[1])` can produce `'.'` (when argv[1] equals cwd) → first branch. The Rust code bases this on `std::env::args().nth(0)` (the executable path) instead of an `argv[1]` script path. For a real installed binary `binName = "candle"` → produces `candle get-doc <file>`. The default `get-doc` subcommand is kept.

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
- ⚠️ Note it prints `rawContent` (with frontmatter), not the trimmed `content`. And the error message uses the original `name` arg, not the normalized baseName.

### Rust implementation
Module `doc_files`. `parse_frontmatter(&str) -> ParsedDocument`. Struct `DocFilesHelper { file_map: IndexMap<String, PathBuf>, get_doc_subcommand: Option<String> }` (an order-preserving `indexmap` to match insertion-order iteration). The `regex` crate handles the frontmatter regex. Stdout strings match exactly for test parity.

## 5. assertValidCommandName (`commands/mod.rs`, mirrors `src/cli/assertValidCommandName.ts`)

The Node original:

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

### Rust implementation
`fn assert_valid_command_name(name)` / `..._names(names: &[String])` delegate to `get_service_info_by_name`. Loose-matching and DB lookup live in the config/db subsystems — this module is a thin validator with a fail-fast (first error wins) loop.

## 6. Public library API (`lib.rs`, mirrors `src/index.ts`)

The Node original is the entrypoint exported as `@facetlayer/candle` for programmatic/GUI use, with exactly these exports. The Rust crate re-exports the equivalents:

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

This public surface does **not** include the error classes, `debugLog`, `isRunByAgent`, `DocFilesHelper`, or `assertValidCommandName` — those are internal. The Rust public crate API is the same listing/logs/config/db set; the alias rename `LatestExecutionLogFilter → AfterProcessStartLogFilter` is preserved for GUI/consumer parity.

## 7. Version handling

`package.json` in the Node original: `"name": "@facetlayer/candle"`, `"version": "0.13.3"`.

The Node original used two mechanisms:

1. **Early flag short-circuit** in `main()` (`src/main-cli.ts:289-295`): before any command parsing, if `process.argv` includes `-v` or `--version`, read `<scriptDir>/../package.json`, `console.log(packageJson.version)`, and `return` (exit 0). This runs *before* help handling.

2. **`findPackageJson()`** (`src/findPackageJson.ts`) — robust resolver used elsewhere (e.g. MCP). Tries `join(__dirname, '..', 'package.json')` first (bundled `dist/main-cli.js` layout), then `join(__dirname, '..', '..', 'package.json')` (source `src/mcp/mcp-main.ts` layout). Throws `Error("Could not find package.json at <a> or <b>")` if neither exists. Returns `{ name, version }`.

3. yargs `.version()` (line 234) is configured but the manual check at line 290 wins because it fires first.

⚠️ In the original, two different path-resolution strategies coexisted: the inline `--version` handler only checked `../package.json` (one level up from the script dir), while `findPackageJson` checked two candidate depths. The Rust implementation compiles the version in at build time via `env!("CARGO_PKG_VERSION")` (in `rust/src/`) rather than reading `package.json` at runtime — avoiding the dual-path fragility. `-v`/`--version` is handled before command dispatch; output is the bare version string + newline on stdout, exit 0.

## 8. wait-for-log command (`commands/wait_for_log.rs`, mirrors `src/wait-for-log-command.ts`)

`handle_wait_for_log(options)` polls logs until a target substring appears or timeout. Belongs partly to the logs subsystem but the control flow is self-contained.

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

`ProcessLogType` values referenced: `process_start_initiated`, `process_exited` (the relevant enum members; the full enum lives in `src/logs/ProcessLogType.ts` / the Rust logs subsystem).

⚠️ Subtleties:
- `content?.includes` — content may be null/undefined; substring match is plain `String.includes` (not regex).
- The exact output strings (with embedded quotes around `message`) are asserted by tests — reproduced verbatim.
- Polling uses wall-clock `Date.now()`; the Rust code uses `Instant`/`SystemTime` with a 200ms sleep.
- The `LogIterator` is stateful — `getNextLogs()` returns only logs newer than the last call (cursor). The filter (`logFilter`) is also stateful across calls in the loop (same instance reused), distinct from `printRecentLogs` which creates a fresh filter.

### Rust implementation
`async fn handle_wait_for_log(opts) -> WaitForLogResult { success: bool, message: Option<String> }`, depending on the logs subsystem (`LogIterator`, `LatestExecutionLogFilter`, `get_process_logs`, `console_log_row`, `ProcessLogType`) plus `tokio` for the 200ms poll loop.

## 9. External npm dependencies & Rust crate equivalents

The Node original's dependencies map onto the following in the Rust implementation:

| npm / Node API | Use here | Rust equivalent |
|---|---|---|
| `node:fs` (`appendFileSync`, `readFileSync`, `readdirSync`, `existsSync`) | debug log, doc reading, package.json | `std::fs` |
| `node:path` (`join`, `basename`, `relative`, `dirname`) | path building | `std::path::{Path, PathBuf}` |
| `node:url` (`fileURLToPath`) | resolve module dir | n/a (uses `std::env::current_exe()` / build-time paths) |
| frontmatter regex | `parseFrontmatter` | `regex` crate |
| insertion-ordered `Map` | `fileMap` iteration order | `indexmap` crate |
| `process.env` | env reads | `std::env::var` |
| `yargs` (`.version()`) | CLI parsing/version | hand-rolled parser in `rust/src/cli/parser.rs`; version via `env!("CARGO_PKG_VERSION")` |
| `setTimeout`/promises | poll loop | `tokio::time::sleep` |

No third-party npm deps in this subsystem itself — `@facetlayer/docs-tool` was intentionally removed and replaced by the in-repo `DocFilesHelper` (per the file header comment).
