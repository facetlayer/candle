# Config subsystem

Covers config file discovery/parsing/validation, the `.candle.json` schema, state/database directory resolution, and the `add-service` / `remove-service` / `set-config` / `setup-project` commands.

The Rust implementation lives in `rust/src/config/` — `model.rs` (schema structs/constants), `paths.rs` (path validation), `validate.rs` (`validate_config`), `file.rs` (discovery, read, service lookup), and `commands.rs` (the mutating commands) — plus `rust/src/dirs.rs` for state-directory resolution. It mirrors the original `src/configFile.ts`, `src/addServerConfig.ts`, `src/removeServerConfig.ts`, `src/set-config-command.ts`, `src/setup-project-command.ts`, `src/dirs.ts`, and `src/findPackageJson.ts`, with CLI wiring in `rust/src/` (originally `src/main-cli.ts`) and DB-dir usage in the `db` module (originally `src/database/database.ts`).

## 1. The `.candle.json` file format

Modeled in `config/model.rs`.

### Top-level object (`CandleSetupConfig`, configFile.ts:21-25)

| Key | Type | Required | Default | Notes |
|-----|------|----------|---------|-------|
| `services` | array of ServiceConfig | no | `[]` | If missing/undefined it is normalized to `[]` (configFile.ts:60). Can also be supplied as an *object map* — see §3. |
| `logEviction` | object | no | — | Nested object, see below. |
| `logCollector` | string enum | no | `"node"` (resolved at point of use, not stored) | Exactly `"node"` or `"rust"`. |

There is no schema versioning field and no other top-level keys. Unknown extra keys are **not** rejected by `validateConfig` — they are silently preserved (the validator spreads `...config` back out at configFile.ts:185-188). The serializer (see §6) preserves arbitrary extra keys on round-trip.

### ServiceConfig (configFile.ts:9-14)

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `name` | string | **yes** | Must be a non-empty string and unique across services. |
| `shell` | string | **yes** | Must be a non-empty string. The shell command to run. |
| `root` | string | no | Working dir, relative to config file dir, OR absolute. Validated by `isValidRootPath`. |
| `enableStdin` | boolean | no | Enables stdin message polling from DB. |

### LogEvictionConfig (configFile.ts:16-19)

| Field | Type | Default (`LOG_EVICTION_DEFAULTS`, configFile.ts:221-224) |
|-------|------|---------|
| `maxLogsPerService` | positive integer | `1000` |
| `maxRetentionSeconds` | positive integer | `86400` (= 24*60*60) |

Defaults are applied at read time by `getLogEvictionConfig` (configFile.ts:231-236) using `??` (null/undefined coalescing) — they are NOT written into the file.

### Example file
```json
{
  "services": [
    { "name": "api", "shell": "npm run dev", "root": "packages/api" },
    { "name": "worker", "shell": "node worker.js" }
  ],
  "logEviction": { "maxLogsPerService": 5000, "maxRetentionSeconds": 172800 },
  "logCollector": "rust"
}
```

## 2. Config file discovery (upward search)

Implemented in `config/file.rs`. Two filenames, **priority order, first match wins** (configFile.ts:29-30):
```
CONFIG_FILENAMES = ['.candle.json', '.candle-setup.json']   // .candle-setup.json is deprecated
DEFAULT_CONFIG_FILENAME = '.candle.json'
```

`find_config_file(current_dir)` (mirrors `findConfigFile`, configFile.ts:68-98):
1. Resolve `current_dir` to absolute; if falsy, use the current working directory.
2. Loop: in the current dir, test each filename in `CONFIG_FILENAMES` order. First existing file is read+validated and returned as `{ config, projectDir: currentDir, configFilename }`.
3. If reading/parsing throws, it is re-wrapped as `Error("Invalid <filename> at <path>: <msg>")` (configFile.ts:84) — note this wrapping loses the `MissingSetupFileError` type for parse errors of an existing file.
4. Move to parent dir. Stop when the parent equals the current dir (filesystem root).
5. If none found, raise `MissingSetupFileError(startingDir)` — message: `No .candle.json file found in (or above) current directory: <startingDir>` (errors.ts:38-42). Note: the error reports the **original** starting dir, not the root.

`find_project_dir(cwd)` (mirrors `findProjectDir`, configFile.ts:37-44) just returns `find_config_file(cwd).projectDir`, re-raising `MissingSetupFileError` on miss.

**Subtle:** the directory walk stops at FS root via the parent-equals-self fixpoint. On macOS/Linux that's `/`; the Rust code uses `Path::parent()` returning `None` as the stop condition. There is no `$HOME` boundary — it walks all the way to root.

## 3. Parsing & validation

`read_config_file(path)` (in `config/file.rs`, mirrors `readConfigFile`, configFile.ts:49-63):
- Read file as UTF-8, trimmed.
- **Empty file (after trim) is valid** and returns `{ services: [] }` (no validation run on this path).
- Otherwise parse JSON, normalize `config.services ||= []`, then `validate_config`.

`validate_config(config)` (in `config/validate.rs`, mirrors `validateConfig`, configFile.ts:103-189):
1. **services as object map** (configFile.ts:108-124): If `services` is not an array but is a non-null object, convert each `[key, value]` into `{ name: key, ...value }`. If it's a non-object non-array (e.g. string/number), raise `ConfigFileError: Config file error: Invalid value for 'services': <JSON>`.
2. For each service:
   - `name` must be truthy string → else `ConfigFileError: Config file error: Each service must have a "name" string`.
   - `shell` must be truthy string → else `ConfigFileError: Config file error: Service "<name>" must have a "shell" string`.
   - Duplicate name → `ConfigFileError: Config file error: Duplicate service name: "<name>"`.
   - If `root` present and `!isValidRootPath(root)` → `ConfigFileError: Service "<name>" has invalid root path: "<root>"`.
3. `logEviction` if present must be a non-null, non-array object, else `ConfigFileError: Config file error: Invalid value for 'logEviction': expected an object`. Each of `maxLogsPerService` / `maxRetentionSeconds`, if present, must be an integer `>= 1`, else `ConfigFileError: Config file error: 'logEviction.<field>' must be a positive integer`.
4. `logCollector` if present must be exactly `"node"` or `"rust"`, else `ConfigFileError: Config file error: Invalid value for 'logCollector': expected 'node' or 'rust', got '<value>'`.
5. Returns `{ ...config, services }` (object-map normalization persisted; defaults NOT injected).

**Validation does not reject unknown top-level or unknown per-service keys.**

### Path validation helpers (`config/paths.rs`, mirrors configFile.ts:192-219)
- `is_valid_root_path(p)`: absolute → always valid. Else lexically normalize; invalid if it `startsWith('..')`. (So `../x` invalid, `a/../b` → normalizes to `b` valid, `a/../../b` → `../b` invalid.)
- `is_valid_relative_path(p)`: absolute → invalid; else same `..` check.

**Subtle:** the normalization is *lexical* (does not touch the filesystem) and collapses `.`/`..` segments. The Rust code uses a lexical normalizer (segment folding), NOT `canonicalize` (which would hit the FS and resolve symlinks). The `startsWith("..")` check is a string prefix on the normalized result, so it also matches a path literally named `..foo` — this prefix-test (not segment-test) quirk is preserved exactly.

### Service cwd resolution (`get_service_cwd`, mirrors `getServiceCwd`, configFile.ts:238-244)
`configDir = dirname(configPath)`; if `service.root` set → resolve `root` against `configDir` (absolute root wins); else `configDir`.

## 4. State / database directory resolution (`dirs.rs`)

`get_state_directory()` (mirrors `getStateDirectory`, dirs.ts:9-22), in order:
1. `CANDLE_DATABASE_DIR` env set → return it **verbatim** (no `candle` suffix appended).
2. `XDG_STATE_HOME` env set → `join(XDG_STATE_HOME, 'candle')`.
3. Default → `join(homedir(), '.local', 'state', 'candle')`.

DB file: `join(stateDir, 'candle.db')` (database.ts:66). The state dir is created recursively if missing (database.ts:62-63). The database opener accepts an `override_directory` that bypasses `get_state_directory` entirely (database.ts:57-60).

**Platform note:** `os.homedir()` in the Node original is `$HOME` on Unix and `%USERPROFILE%` on Windows. The Rust code resolves the home portion the same way (`$HOME`/`USERPROFILE`). The default path uses literal `.local/state/candle` on ALL platforms (the original is not platform-branching here), so Windows AppData is not substituted — only the home portion follows `os.homedir()` semantics.

Version handling does not read `package.json` at runtime: the Rust build embeds the version via `env!("CARGO_PKG_VERSION")` (replacing the original `findPackageJson` which tried `../package.json` then `../../package.json` relative to the module file to read `{name, version}` for `--version`). The original `ProjectRootDir` (dirs.ts:6) = dir-above-`dirs.ts`, used to locate the installed package, is not relevant to config logic.

## 5. SQLite schema (for completeness — created by the config/DB bootstrap)

Schema name `CandleDatabase`, migration mode `safe-upgrades`, WAL + `busy_timeout=30000` set after load (database.ts:79-81). Tables (database.ts:16-50):
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
`RunningStatus`: `running=1, stopped=0` (database.ts:6-9).

## 6. Serialization (write-back)

All three mutating commands write with 2-space indent, no trailing newline (mirrors `JSON.stringify(config, null, 2)` — addServerConfig.ts:40, removeServerConfig.ts:20, set-config-command.ts:64). Object key order follows insertion order: for a freshly created file it is `{ "services": [] }`. For `set-config`, new keys are appended after existing ones. The Rust code uses an order-preserving serializer (`serde_json` with the `preserve_order` feature, backed by `indexmap`) and 2-space pretty printing to match byte-for-byte where tests check file contents.

## 7. Commands

Implemented in `config/commands.rs`, with CLI dispatch in `rust/src/`.

### `setup-project` (mirrors setup-project-command.ts; CLI main-cli.ts:185, 472)
- No args/options (`strictOptions`). Operates on the current working directory.
- If `find_config_file(cwd)` succeeds → print `Config file already exists at <configPath>` and return (no write). `<configPath>` = `join(projectDir, configFilename)`.
- Else (only if error is `MissingSetupFileError`; otherwise rethrow) → write `{ "services": [] }` to `join(cwd, '.candle.json')` and print `Created .candle.json in <cwd>`.

### `add-service <name> --shell <s> [--root <r>] [--enable-stdin]` (mirrors addServerConfig.ts; CLI main-cli.ts:186-205, 477)
- `name` positional required. `--shell` `demandOption: true`. `--root` string optional. `--enable-stdin` boolean optional. `strictOptions` (unknown flags rejected).
- CLI rejects multiple command names: prints `Error: Cannot use multiple command names for add-service` to stderr + `exit(1)`.
- `find_or_create_setup_file(cwd)`: if a config exists upward, use `join(projectDir, configFilename)`; if `MissingSetupFileError`, create `{ "services": [] }` at `join(cwd, '.candle.json')` (other errors rethrow).
- Read config. If a service with `name` exists → raise `Error("Service '<name>' already exists in configuration")`.
- Build new service object with fields in this exact insertion order: `name`, `shell`, then `root` **only if truthy**, then `enableStdin` **only if truthy** (addServerConfig.ts:26-31 — falsy values are omitted entirely).
- Push, `validate_config`, write, print `Service '<name>' added successfully to .candle.json`.
- On any thrown error the CLI prints `Error adding service: <message>` + `exit(1)` (main-cli.ts:490-492).

### `remove-service <name>` (mirrors removeServerConfig.ts; CLI main-cli.ts:207-213, 507)
- `name` positional required, `strictOptions`. Multiple names → `Error: Cannot use multiple command names for remove-service` + `exit(1)`.
- `find_config_file(cwd)` (does NOT create — raises `MissingSetupFileError` if absent). Filter out matching name.
- If length unchanged → raise `Error("Service '<name>' not found in configuration")`.
- `validate_config`, write, print `Service '<name>' removed from .candle.json`.
- CLI error wrapper: `Error removing service: <message>` + `exit(1)`.

### `set-config <key> <value>` (mirrors set-config-command.ts; CLI main-cli.ts:215-225, 497)
- Both positionals required, `strictOptions`. `value` arrives as a **string** from CLI.
- Allowed keys (`VALID_CONFIG_KEYS`, set-config-command.ts:6-36):
  - `logCollector` → must be `"node"`/`"rust"`, else `UsageError("Invalid value for 'logCollector': expected 'node' or 'rust', got '<value>'")`. Stored as string.
  - `logEviction.maxLogsPerService` → `Number(value)` must be integer `>= 1`, else `UsageError("Invalid value for 'logEviction.maxLogsPerService': expected a positive integer")`. Stored as **number**.
  - `logEviction.maxRetentionSeconds` → same rule/message with its key. Stored as number.
- Unknown key → `UsageError("Unknown config key '<key>'. Valid keys: logCollector, logEviction.maxLogsPerService, logEviction.maxRetentionSeconds")`.
- Locates config via `find_config_file(cwd)` (raises if absent — does NOT create).
- Sets via dot-path: 1 part → top-level key; 2 parts → create parent object `{}` if absent then set child (set-config-command.ts:53-60). Deeper nesting not supported (silently no-op for >2 parts).
- `validate_config`, write, print `Set '<key>' to '<value>' in <configFilename>` (note: prints the original string `value`, and the resolved `configFilename` which may be `.candle-setup.json`).
- CLI error wrapper: `Error: <message>` + `exit(1)`.

**Subtle:** `Number(value)` parsing — JS `Number("")` = 0 (fails `<1`), `Number("3.5")` not integer (fails), `Number("3abc")` = NaN (fails `Number.isInteger`), `Number(" 5 ")` = 5 (whitespace trimmed, passes), `Number("0x10")`=16, `Number("1e3")`=1000 (integer, passes). The Rust code uses a parser that mimics JS `Number()` coercion rather than `str::parse::<i64>()`, to reproduce exactly — especially the leading/trailing-whitespace and `1e3`/hex acceptance.

## 8. Error types (`errors.rs`, mirrors errors.ts)
- `UsageError` — has `isUsageError = true`, `name='UsageError'`.
- `ConfigFileError` — `name='ConfigFileError'` (no `isUsageError`).
- `MissingServiceWithNameError` — `name='NeedRunCommandError'`, `isUsageError=true`, message `No service '<commandName>' configured for directory: <cwd>`.
- `MissingSetupFileError` — `name='MissingSetupFile'`, `isUsageError=true`, message `No .candle.json file found in (or above) current directory: <cwd>`.

These are modeled as an enum (`ConfigError`) with variants carrying the same data; the `is_usage_error()` flag distinguishes user-facing (exit 1, no stack) from internal errors at the CLI boundary.

## 9. Service lookup (used by start/logs/etc., relevant for config consumers)

In `config/file.rs`.
- `find_service_by_name` / `get_all_service_names` (configFile.ts:246-252): plain find/map by exact name.
- `resolve_command_names_or_all` (configFile.ts:259-269): empty list → all service names; if config has zero services → `UsageError("No services configured in .candle.json")`.
- `get_service_config_by_name` (configFile.ts:326-349): exact match first; else `find_loose_command_name`.
- `find_loose_command_name` (configFile.ts:276-324) — substring + directory-aware loose matching: finds services whose `name` *contains* `commandName`; among those, prefers ones whose resolved root equals the search dir; walks up parent dirs (stopping at projectDir or FS root); multiple dir-matches → `Error("Ambiguous service name "<x>". Multiple services match in current directory: <names>")`; falls back to the single substring match if exactly one exists at the top. This is intricate, and the loose resolution drives `start <partial>` matching.

## 10. External npm dependencies → Rust crates

The Node original's dependencies map onto the following crates in the Rust implementation:

| npm dep | Used for | Rust crate |
|---------|----------|------------|
| `@facetlayer/sqlite-wrapper` (`DatabaseLoader`, `SqliteDatabase`) | SQLite open + migrations (`safe-upgrades`) | `rusqlite` (+ a migration runner). `PRAGMA journal_mode=WAL` and `PRAGMA busy_timeout=30000` are set after open. |
| Node `fs` / `path` | file IO, lexical path ops | `std::fs`, `std::path`; lexical normalization is hand-rolled (not `canonicalize`). |
| Node `os.homedir()` | home dir | `$HOME`/`USERPROFILE` env resolution. |
| `JSON.parse` / `JSON.stringify(_,null,2)` | config IO with key-order preservation | `serde_json` with `preserve_order` feature (uses `indexmap` internally) + `serde_json::to_string_pretty` (2-space). |
| `yargs` | CLI parsing (`strictOptions`, `demandOption`, positionals) | hand-rolled parser in `rust/src/cli/parser.rs` (strict, required args). |

Tests most sensitive to exact behavior: the stdout success strings, error messages (verbatim), 2-space JSON formatting with preserved key order and omitted falsy fields, empty-file handling, object-map `services`, the `..`-prefix path rule, and JS `Number()` coercion in `set-config`.
