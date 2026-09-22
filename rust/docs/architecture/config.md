# Config subsystem

Covers config file discovery/parsing/validation, the `.candle.json` schema, state/database directory resolution, and the `add-service` / `remove-service` / `set-config` / `setup-project` commands.

The Rust implementation lives in `rust/src/config/` — `model.rs` (schema structs/constants), `paths.rs` (path validation), `validate.rs` (`validate_config`), `file.rs` (discovery, read, service lookup), and `commands.rs` (the mutating commands) — plus `rust/src/dirs.rs` for state-directory resolution. It was ported from the original Node implementation, which has been removed; the following files and the `configFile.ts:NN` line references below are historical pointers only: `src/configFile.ts`, `src/addServerConfig.ts`, `src/removeServerConfig.ts`, `src/set-config-command.ts`, `src/setup-project-command.ts`, `src/dirs.ts`, and `src/findPackageJson.ts`. CLI wiring is in `rust/src/main.rs` (originally `src/main-cli.ts`) and DB-dir usage in the `db` module (originally `src/database/database.ts`).

## 1. The `.candle.json` file format

Modeled in `config/model.rs`.

### Top-level object (`CandleSetupConfig`, configFile.ts:21-25)

| Key | Type | Required | Default | Notes |
|-----|------|----------|---------|-------|
| `services` | array of ServiceConfig | no | `[]` | If missing/undefined it is normalized to `[]` (configFile.ts:60). Can also be supplied as an *object map* — see §3. |
| `logEviction` | object | no | — | Nested object, see below. |

There is no schema versioning field and no other known top-level keys. The former `logCollector` key (which chose between the Node and Rust collector sidecars) is **retired**: it is no longer validated or settable, and a leftover entry is treated like any other unknown key. Unknown extra top-level keys are **not** rejected by `validate_config`; they are kept in `CandleSetupConfig.extra` (with their position in `key_order`) and the serializer (see §6) writes them back verbatim on round-trip. Unknown **per-service** keys are also accepted and preserved: `validate_config` keeps each service's raw object in `CandleSetupConfig.service_raw` (keyed by name), and write-back starts from that object, updating the known keys in place. Whenever a config file is read, `read_config_file` prints a warning to stderr (once per process per message) for every unknown top-level or per-service key, suggesting a likely known key where it can (`cwd` → `root`, `command` → `shell`, case mismatches).

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
  "logEviction": { "maxLogsPerService": 5000, "maxRetentionSeconds": 172800 }
}
```

## 2. Config file discovery (upward search)

Implemented in `config/file.rs`. Two filenames, **priority order, first match wins** (configFile.ts:29-30):
```
CONFIG_FILENAMES = ['.candle.json', '.candle-setup.json']   // .candle-setup.json is deprecated
DEFAULT_CONFIG_FILENAME = '.candle.json'
```

`find_config_file(start_dir)` (originally `findConfigFile`, configFile.ts:68-98), returning `FoundConfig { config, project_dir, config_filename }`:
1. Make `start_dir` absolute lexically (`std::path::absolute`, no symlink resolution).
2. Loop: in the current dir, test each filename in `CONFIG_FILENAMES` order. First existing file is read+validated and returned as `{ config, projectDir: currentDir, configFilename }`.
3. If reading/parsing fails, it is re-wrapped as `ConfigFileError("Invalid <filename> at <path>: <msg>")` (configFile.ts:84), so a broken existing file is never reported as `MissingSetupFile`.
4. Move to parent dir. Stop when the parent equals the current dir (filesystem root).
5. If none found, return `CandleError::MissingSetupFile { cwd: start_dir }` — message: `No .candle.json file found in (or above) current directory: <startingDir>` (errors.ts:38-42). Note: the error reports the **original** starting dir, not the root.

`find_project_dir(cwd)` (originally `findProjectDir`, configFile.ts:37-44) just returns `find_config_file(cwd)?.project_dir`.

**Subtle:** the directory walk stops at FS root via the parent-equals-self fixpoint. On macOS/Linux that's `/`; the Rust code uses `Path::parent()` returning `None` as the stop condition. There is no `$HOME` boundary — it walks all the way to root.

## 3. Parsing & validation

`read_config_file(path)` (in `config/file.rs`, originally `readConfigFile`, configFile.ts:49-63):
- Read file as UTF-8, trimmed. A read error → `ConfigFileError("Failed to read <path>: <e>")`.
- **Empty file (after trim) is valid** and returns `{ services: [] }` (no validation run on this path).
- Otherwise parse JSON (a parse error → `ConfigFileError(<serde message>)`), then `validate_config`, which performs the `services || []` normalization itself.

`validate_config(value: serde_json::Value) -> Result<CandleSetupConfig, CandleError>` (in `config/validate.rs`, originally `validateConfig`, configFile.ts:103-189). A non-object top level is reported as an invalid `services` value:
1. **services as object map** (configFile.ts:108-124): If `services` is not an array but is a non-null object, convert each `[key, value]` into `{ name: key, ...value }`. If it's a non-object non-array (e.g. string/number), raise `ConfigFileError: Config file error: Invalid value for 'services': <JSON>`.
2. For each service:
   - `name` must be truthy string → else `ConfigFileError: Config file error: Each service must have a "name" string`.
   - `shell` must be truthy string → else `ConfigFileError: Config file error: Service "<name>" must have a "shell" string`.
   - Duplicate name → `ConfigFileError: Config file error: Duplicate service name: "<name>"`.
   - If `root` is a non-empty string and `!is_valid_root_path(root)` → `ConfigFileError: Service "<name>" has invalid root path: "<root>"` (no `Config file error:` prefix). A non-string `root` is ignored.
   - `enableStdin` is read only if it is a JSON boolean.
3. `logEviction` if present must be a non-null, non-array object, else `ConfigFileError: Config file error: Invalid value for 'logEviction': expected an object`. Each of `maxLogsPerService` / `maxRetentionSeconds`, if present, must be an integer `>= 1`, else `ConfigFileError: Config file error: 'logEviction.<field>' must be a positive integer`.
4. Returns a `CandleSetupConfig { services, log_eviction, key_order, extra }` (object-map normalization persisted; defaults NOT injected). `logCollector` gets no special treatment (see §1).

**Validation does not reject unknown top-level or unknown per-service keys** (they only warn, see §1).

### Path validation helpers (`config/paths.rs`, originally configFile.ts:192-219)
- `is_valid_root_path(p)`: absolute → always valid. Else lexically normalize; invalid if it `startsWith('..')`. (So `../x` invalid, `a/../b` → normalizes to `b` valid, `a/../../b` → `../b` invalid.)
- `is_valid_relative_path(p)`: absolute → invalid; else same `..` check.

**Subtle:** the normalization is *lexical* (does not touch the filesystem) and collapses `.`/`..` segments. The Rust code uses a lexical normalizer (`lexical_normalize`, segment folding; absolute is a plain `starts_with('/')` test), NOT `canonicalize` (which would hit the FS and resolve symlinks). The `startsWith("..")` check is a string prefix on the normalized result, so it also matches a path literally named `..foo` — this prefix-test (not segment-test) quirk is preserved exactly.

### Service cwd resolution (`get_service_cwd`, originally `getServiceCwd`, configFile.ts:238-244)
`configDir = dirname(configPath)`; if `service.root` set → resolve `root` against `configDir` (absolute root wins); else `configDir`.

## 4. State / database directory resolution (`dirs.rs`)

`get_state_directory()` (originally `getStateDirectory`, dirs.ts:9-22), in order (an empty env value counts as unset):
1. `CANDLE_DATABASE_DIR` env set → return it **verbatim** (no `candle` suffix appended).
2. `XDG_STATE_HOME` env set → `join(XDG_STATE_HOME, 'candle')`.
3. Default → `join($HOME, '.local', 'state', 'candle')`.

DB file: `join(stateDir, 'candle.db')` (`dirs::candle_db_path()`). The state dir is created recursively if missing. `db::get_database(override_dir)` accepts an override directory that bypasses `get_state_directory` entirely. The state dir also holds the per-service start locks under `locks/` (see [start-flow.md](start-flow.md)).

**Platform note:** the Rust code reads only `$HOME` (empty if unset); there is no `USERPROFILE` fallback, since Candle targets macOS/Linux. The default path uses literal `.local/state/candle`.

Version handling does not read `package.json` at runtime: the Rust build embeds the version via `env!("CARGO_PKG_VERSION")` (replacing the original `findPackageJson` which tried `../package.json` then `../../package.json` relative to the module file to read `{name, version}` for `--version`). The original `ProjectRootDir` (dirs.ts:6) = dir-above-`dirs.ts`, used to locate the installed package, is not relevant to config logic.

## 5. SQLite schema (for completeness — created by the config/DB bootstrap)

Applied by `db::get_database` / `open_database_at` as `create ... if not exists` statements (the Node original used schema name `CandleDatabase` with migration mode `safe-upgrades`), with WAL + `busy_timeout=30000` set on every connection. Tables:
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
The Node original also defined a `RunningStatus` enum (`running=1, stopped=0`, database.ts:6-9); it has no Rust counterpart and no column uses it.

## 6. Serialization (write-back)

All the commands that write the file (`setup-project`, `add-service`, `remove-service`, `set-config`) write with 2-space indent and a trailing newline (the Node original had none). Object key order follows insertion order: for a freshly created file it is `{ "services": [] }`. For `set-config`, new keys are appended after existing ones. The Rust code rebuilds the object in `key_order` (`CandleSetupConfig::to_value`), using `serde_json` with the `preserve_order` feature, and writes `to_string_pretty` (2-space) output to match byte-for-byte where tests check file contents. Each service is written as `name`, `shell`, then `root` (if non-empty) and `enableStdin` (only if `true`).

## 7. Commands

Implemented in `config/commands.rs` (each handler returns the success message; `main.rs` prints it), with CLI dispatch in `rust/src/main.rs`. All four operate on the current working directory and do not accept `--project-dir`.

### `setup-project` (originally setup-project-command.ts)
- No args/options (`strictOptions`). Operates on the current working directory.
- If `find_config_file(cwd)` succeeds → print `Config file already exists at <configPath>` and return (no write). `<configPath>` = `join(projectDir, configFilename)`.
- Else (only if error is `MissingSetupFile`; other errors propagate) → write `{ "services": [] }` to `join(cwd, '.candle.json')` and print `Created .candle.json in <cwd>`.

### `add-service <name> --shell <s> [--root <r>] [--enable-stdin]` (originally addServerConfig.ts)
- `name` positional required (missing → `Error: Service name is required`, exit 1). `--shell` required and non-empty (missing → `Missing required argument: shell`, exit 1). `--root` string optional. `--enable-stdin` boolean optional. Unknown flags are rejected.
- CLI rejects multiple command names: prints `Error: Cannot use multiple command names for add-service` to stderr + `exit(1)`.
- Before any file is created: the name must be non-empty and use only ASCII letters, digits, `-`, `_`, `.` (else `UsageError("Invalid service name '<name>': use only letters, digits, '-', '_' and '.'")`). A non-empty, otherwise valid `--root` must resolve (against the discovered project dir, or `cwd` if none) to an existing directory (else `UsageError("Root directory does not exist: <abs path>")`).
- `find_or_create_setup_file(cwd)`: if a config exists upward, use `join(projectDir, configFilename)`; if `MissingSetupFile`, create `{ "services": [] }` at `join(cwd, '.candle.json')` (other errors rethrow).
- Read config. If a service with `name` exists → `ConfigFileError("Service '<name>' already exists in configuration")`.
- Build new service object with fields in this exact insertion order: `name`, `shell`, then `root` **only if truthy**, then `enableStdin` **only if truthy** (addServerConfig.ts:26-31 — falsy values are omitted entirely).
- Push, revalidate (`validate_config(config.to_value())`), write, print `Service '<name>' added successfully to .candle.json`.
- On any error the CLI prints `Error adding service: <message>` to stderr + `exit(1)`.

### `remove-service <name>` (originally removeServerConfig.ts)
- `name` positional required (missing → `Error: Service name is required`), unknown flags rejected. Multiple names → `Error: Cannot use multiple command names for remove-service` + `exit(1)`.
- `find_config_file(cwd)` (does NOT create; `MissingSetupFile` if absent). Filter out matching name.
- If length unchanged → `ConfigFileError("Service '<name>' not found in configuration")`.
- Revalidate, write, print `Service '<name>' removed from .candle.json`.
- CLI error wrapper: `Error removing service: <message>` + `exit(1)`.

### `set-config <key> <value>` (originally set-config-command.ts)
- Both positionals required (else `Error: set-config requires a <key> and a <value>`, exit 1), unknown flags rejected. `value` arrives as a **string** from CLI.
- Allowed keys (`VALID_CONFIG_KEYS`). `logCollector` is no longer one of them:
  - `logEviction.maxLogsPerService` → `Number(value)` must be integer `>= 1`, else `UsageError("Invalid value for 'logEviction.maxLogsPerService': expected a positive integer")`. Stored as **number**.
  - `logEviction.maxRetentionSeconds` → same rule/message with its key. Stored as number.
- Unknown key → `UsageError("Unknown config key '<key>'. Valid keys: logEviction.maxLogsPerService, logEviction.maxRetentionSeconds")`. Key/value are validated **before** the config file is read.
- Locates config via `find_config_file(cwd)` (errors if absent; does NOT create).
- Sets the field on `log_eviction` (creating it if absent, and appending `logEviction` to `key_order` if new). There is no generic dot-path setter.
- Revalidate, write, print `Set '<key>' to '<value>' in <configFilename>` (note: prints the original string `value`, and the resolved `configFilename` which may be `.candle-setup.json`).
- CLI error wrapper: `Error: <message>` + `exit(1)`.

**Subtle:** `Number(value)` parsing — JS `Number("")` = 0 (fails `<1`), `Number("3.5")` not integer (fails), `Number("3abc")` = NaN (fails `Number.isInteger`), `Number(" 5 ")` = 5 (whitespace trimmed, passes), `Number("0x10")`=16, `Number("1e3")`=1000 (integer, passes). The Rust code uses `js_number` / `js_positive_int`, which mimic JS `Number()` coercion rather than `str::parse::<i64>()`, to reproduce exactly — especially the leading/trailing-whitespace, `1e3`, and `0x`/`0o`/`0b` acceptance.

## 8. Error types (`errors.rs`, originally errors.ts)
- `UsageError` — has `isUsageError = true`, `name='UsageError'`.
- `ConfigFileError` — `name='ConfigFileError'` (no `isUsageError`).
- `MissingServiceWithNameError` — `name='NeedRunCommandError'`, `isUsageError=true`, message `No service '<commandName>' configured for directory: <cwd>`.
- `MissingSetupFileError` — `name='MissingSetupFile'`, `isUsageError=true`, message `No .candle.json file found in (or above) current directory: <cwd>`.

These are variants of the crate-wide `CandleError` enum (`UsageError`, `ConfigFileError`, `MissingServiceWithName`, `MissingSetupFile`), carrying the same data; see [cli.md](cli.md) §1. `is_usage_error()` still distinguishes them, but the CLI prints the message and exits 1 either way.

## 9. Service lookup (used by start/logs/etc., relevant for config consumers)

In `config/file.rs`.
- `find_service_by_name` / `get_all_service_names` (configFile.ts:246-252): plain find/map by exact name.
- `resolve_command_names_or_all` (configFile.ts:259-269): empty list → all service names; if config has zero services → `UsageError("No services configured in .candle.json")`.
- `get_service_config_by_name(command_name, current_dir: Option<&Path>)` (configFile.ts:326-349): exact match first; else `find_loose_command_name`; neither → `MissingServiceWithName { command_name, cwd: project_dir }`. Returns `FoundServiceConfig { service_config, project_dir }`.
- `find_loose_command_name` (configFile.ts:276-324) — substring + directory-aware loose matching: finds services whose `name` *contains* `commandName`; among those, prefers ones whose resolved root equals the search dir; walks up parent dirs (stopping at projectDir or FS root); multiple dir-matches → `ConfigFileError("Ambiguous service name "<x>". Multiple services match in current directory: <names>")`; falls back to the single substring match if exactly one exists at the top. This is intricate, and the loose resolution drives `start <partial>` matching.

## 10. External npm dependencies → Rust crates

The Node original's dependencies map onto the following crates in the Rust implementation:

| npm dep | Used for | Rust crate |
|---------|----------|------------|
| `@facetlayer/sqlite-wrapper` (`DatabaseLoader`, `SqliteDatabase`) | SQLite open + migrations (`safe-upgrades`) | `rusqlite` (+ `if not exists` schema statements). `PRAGMA journal_mode=WAL` and `PRAGMA busy_timeout=30000` are set after open. |
| Node `fs` / `path` | file IO, lexical path ops | `std::fs`, `std::path`; lexical normalization is hand-rolled (not `canonicalize`). |
| Node `os.homedir()` | home dir | `$HOME` env var. |
| `JSON.parse` / `JSON.stringify(_,null,2)` | config IO with key-order preservation | `serde_json` with `preserve_order` feature + explicit `key_order` tracking + `serde_json::to_string_pretty` (2-space). |
| `yargs` | CLI parsing (`strictOptions`, `demandOption`, positionals) | hand-rolled parser in `rust/src/cli/parser.rs` (strict, required args). |

Tests most sensitive to exact behavior: the stdout success strings, error messages (verbatim), 2-space JSON formatting with preserved key order and omitted falsy fields, empty-file handling, object-map `services`, the `..`-prefix path rule, and JS `Number()` coercion in `set-config`.
