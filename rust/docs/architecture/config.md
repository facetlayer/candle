# Config subsystem

Covers config file discovery/parsing/validation, the `.candle.json` schema, state/database directory resolution, and the `add-service` / `remove-service` / `set-config` / `setup-project` commands.

The implementation lives in `rust/src/config/` — `model.rs` (schema structs/constants), `paths.rs` (path validation), `validate.rs` (`validate_config`), `file.rs` (discovery, read, service lookup), and `commands.rs` (the mutating commands) — plus `rust/src/dirs.rs` for state-directory resolution. CLI wiring is in `rust/src/main.rs` and DB-dir usage in the `db` module.

## 1. The `.candle.json` file format

Modeled in `config/model.rs`.

### Top-level object (`CandleSetupConfig`)

| Key | Type | Required | Default | Notes |
|-----|------|----------|---------|-------|
| `services` | array of ServiceConfig | no | `[]` | If missing (or `null`/`false`/`0`/`""`) it is normalized to `[]`. Can also be supplied as an *object map* — see §3. |
| `logEviction` | object | no | — | Nested object, see below. |

There is no schema versioning field and no other known top-level keys. A leftover `logCollector` key from older versions is treated like any other unknown key. Unknown extra top-level keys are **not** rejected by `validate_config`; they are kept in `CandleSetupConfig.extra` (with their position in `key_order`) and the serializer (see §6) writes them back verbatim on round-trip. Unknown **per-service** keys are also accepted and preserved: `validate_config` keeps each service's raw object in `CandleSetupConfig.service_raw` (keyed by name), and write-back starts from that object, updating the known keys in place. Whenever a config file is read, `read_config_file` prints a warning to stderr (once per process per message) for every unknown top-level or per-service key, suggesting a likely known key where it can (`cwd` → `root`, `command` → `shell`, case mismatches).

### ServiceConfig

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `name` | string | **yes** | Must be a non-empty string and unique across services. |
| `shell` | string | **yes** | Must be a non-empty string. The shell command to run. |
| `root` | string | no | Working dir, relative to config file dir, OR absolute. Validated by `is_valid_root_path`. |
| `enableStdin` | boolean | no | Enables stdin message polling from DB. |

### LogEvictionConfig

| Field | Type | Default (`LOG_EVICTION_DEFAULTS`) |
|-------|------|---------|
| `maxLogsPerService` | positive integer | `1000` |
| `maxRetentionSeconds` | positive integer | `86400` (= 24*60*60) |

Defaults are applied at read time by `get_log_eviction_config` (each unset field falls back to its default) — they are NOT written into the file.

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

Implemented in `config/file.rs`. Two filenames, **priority order, first match wins**:
```rust
pub const CONFIG_FILENAMES: [&str; 2] = [".candle.json", ".candle-setup.json"]; // .candle-setup.json is deprecated
pub const DEFAULT_CONFIG_FILENAME: &str = ".candle.json";
```

`find_config_file(start_dir)`, returning `FoundConfig { config, project_dir, config_filename }`:
1. Make `start_dir` absolute lexically (`std::path::absolute`, no symlink resolution).
2. Loop: in the current dir, test each filename in `CONFIG_FILENAMES` order. First existing file is read+validated and returned as `FoundConfig { config, project_dir: current, config_filename }`.
3. If reading/parsing fails, it is re-wrapped as `ConfigFileError("Invalid <filename> at <path>: <msg>")`, so a broken existing file is never reported as `MissingSetupFile`.
4. Move to parent dir. Stop when the parent equals the current dir (filesystem root).
5. If none found, return `CandleError::MissingSetupFile { cwd: start_dir, explicit: false }` — message: `No .candle.json file found in (or above) current directory: <start_dir>` plus a hint line (see [cli.md](cli.md) §1). Note: the error reports the **original** starting dir, not the root.

`find_project_dir(cwd)` just returns `find_config_file(cwd)?.project_dir`.

**Subtle:** the directory walk stops at FS root, when `Path::parent()` returns `None` (or the current dir itself). On macOS/Linux that's `/`. There is no `$HOME` boundary — it walks all the way to root.

## 3. Parsing & validation

`read_config_file(path)` (in `config/file.rs`):
- Read file as UTF-8, trimmed. A read error → `ConfigFileError("Failed to read <path>: <e>")`.
- **Empty file (after trim) is valid** and returns `{ services: [] }` (no validation run on this path).
- Otherwise parse JSON (a parse error → `ConfigFileError(<serde message>)`), then `validate_config`, which performs the missing-`services` → `[]` normalization itself. Unknown-key warnings (§1) are emitted just before validation.

`validate_config(value: serde_json::Value) -> Result<CandleSetupConfig, CandleError>` (in `config/validate.rs`). A non-object top level is reported as an invalid `services` value:
1. **services as object map**: If `services` is not an array but is an object, convert each `[key, value]` into an object with `name: key` followed by the value's own fields (so a `name` inside the value wins). If it's a non-object non-array (e.g. string/number), raise `ConfigFileError: Config file error: Invalid value for 'services': <JSON>`.
2. For each service:
   - `name` must be a non-empty string → else `ConfigFileError: Config file error: Each service must have a "name" string`.
   - `shell` must be a non-empty string → else `ConfigFileError: Config file error: Service "<name>" must have a "shell" string`.
   - Duplicate name → `ConfigFileError: Config file error: Duplicate service name: "<name>"`.
   - If `root` is a non-empty string and `!is_valid_root_path(root)` → `ConfigFileError: Service "<name>" has invalid root path: "<root>"` (no `Config file error:` prefix). A non-string `root` is ignored.
   - `enableStdin` is read only if it is a JSON boolean.
3. `logEviction` if present must be a non-null, non-array object, else `ConfigFileError: Config file error: Invalid value for 'logEviction': expected an object`. Each of `maxLogsPerService` / `maxRetentionSeconds`, if present, must be an integer `>= 1`, else `ConfigFileError: Config file error: 'logEviction.<field>' must be a positive integer`.
4. Returns a `CandleSetupConfig { services, log_eviction, key_order, extra, service_raw }` (object-map normalization persisted; defaults NOT injected).

**Validation does not reject unknown top-level or unknown per-service keys** (they only warn, see §1).

### Path validation helpers (`config/paths.rs`)
- `is_valid_root_path(p)`: absolute → always valid. Else lexically normalize; invalid if it starts with `..`. (So `../x` invalid, `a/../b` → normalizes to `b` valid, `a/../../b` → `../b` invalid.)
- `is_valid_relative_path(p)`: absolute → invalid; else same `..` check.

**Subtle:** the normalization is *lexical* (`lexical_normalize`, segment folding; absolute is a plain `starts_with('/')` test), NOT `canonicalize` (which would hit the FS and resolve symlinks). The `..` check is a string prefix on the normalized result, so it also matches a path literally named `..foo` — a deliberate prefix-test (not segment-test) quirk.

### Service cwd resolution (`get_service_cwd`)
`config_dir = config_path.parent()`; if `service.root` is non-empty → `path_resolve(config_dir, root)` (lexical; absolute root wins); else `config_dir`.

## 4. State / database directory resolution (`dirs.rs`)

`get_state_directory()` (pure core: `resolve_state_dir`), in order (an empty env value counts as unset):
1. `CANDLE_DATABASE_DIR` env set → return it **verbatim** (no `candle` suffix appended).
2. `XDG_STATE_HOME` env set → `$XDG_STATE_HOME/candle`.
3. Default → `$HOME/.local/state/candle`.

DB file: `<state_dir>/candle.db` (`dirs::candle_db_path()`). The state dir is created recursively if missing. `db::get_database(override_dir)` accepts an override directory that bypasses `get_state_directory` entirely. The state dir also holds the per-service start locks under `locks/` (see [start-flow.md](start-flow.md)).

**Platform note:** the Rust code reads only `$HOME` (empty if unset); there is no `USERPROFILE` fallback, since Candle targets macOS/Linux. The default path uses literal `.local/state/candle`.

## 5. SQLite schema (for completeness — created by the config/DB bootstrap)

Applied by `db::get_database` / `open_database_at` as `create ... if not exists` statements, with WAL + `busy_timeout=30000` set on every connection. Tables:
```sql
create table processes(
  id integer primary key autoincrement,
  command_name text not null, project_dir text not null,
  pid integer not null, log_collector_pid integer,
  start_time integer not null,
  created_at integer not null default (strftime('%s','now')),
  killed_at integer, shell text, root text, run_id integer);
create table process_output(
  id integer primary key autoincrement,
  command_name text not null, project_dir text not null,
  content text, log_type integer not null,
  timestamp integer not null default (strftime('%s','now')),
  run_id integer);
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
create index idx_process_output_run on process_output(project_dir, command_name, run_id);
create index idx_process_output_launches on process_output(project_dir, command_name, log_type, id);
-- plus trigger process_output_assign_run; see database.md
```
## 6. Serialization (write-back)

All the commands that write the file (`setup-project`, `add-service`, `remove-service`, `set-config`) write with 2-space indent and a trailing newline. Object key order follows insertion order: for a freshly created file it is `{ "services": [] }`. For `set-config`, new keys are appended after existing ones. `CandleSetupConfig::to_value` rebuilds the object in `key_order`, using `serde_json` with the `preserve_order` feature, and `to_json_string` writes `to_string_pretty` (2-space) output plus `\n`; tests check file contents byte-for-byte. Each service is written as `name`, `shell`, then `root` (if non-empty) and `enableStdin` (only if `true`).

## 7. Commands

Implemented in `config/commands.rs` (each handler returns the success message; `main.rs` prints it), with CLI dispatch in `rust/src/main.rs`. All four operate on the current working directory and do not accept `--project-dir`.

### `setup-project` (`handle_setup_project`)
- No args/options (unknown flags rejected). Operates on the current working directory.
- If `find_config_file(cwd)` succeeds → print `Config file already exists at <config_path>` and return (no write). `<config_path>` = `project_dir.join(config_filename)`.
- Else (only if error is `MissingSetupFile`; other errors propagate) → write `{ "services": [] }` to `cwd.join(".candle.json")` and print `Created .candle.json in <cwd>`.

### `add-service <name> --shell <s> [--root <r>] [--enable-stdin]` (`add_server_config`)
- `name` positional required (missing → `Error: Service name is required`, exit 1). `--shell` required and non-empty (missing → `Error: --shell <command> is required`, exit 1). `--root` string optional. `--enable-stdin` boolean optional. Unknown flags are rejected.
- CLI rejects multiple command names: prints `Error: Cannot use multiple command names for add-service` to stderr, exit 1.
- Before any file is created: the name must be non-empty and use only ASCII letters, digits, `-`, `_`, `.` (else `UsageError("Invalid service name '<name>': use only letters, digits, '-', '_' and '.'")`). A non-empty, otherwise valid `--root` must resolve (against the discovered project dir, or `cwd` if none) to an existing directory (else `UsageError("Root directory does not exist: <abs path>")`).
- `find_or_create_setup_file(cwd)`: if a config exists upward, use `project_dir.join(config_filename)`; if `MissingSetupFile`, create `{ "services": [] }` at `cwd.join(".candle.json")` (other errors propagate).
- Read config. If a service with `name` exists → `ConfigFileError("Service '<name>' already exists in configuration")`.
- Build the new service with fields in this exact order: `name`, `shell`, then `root` **only if non-empty**, then `enableStdin` **only if `true`** (empty/false values are omitted entirely).
- Push, revalidate (`validate_config(config.to_value())`), write, print `Service '<name>' added successfully to .candle.json`.
- On any error the CLI prints `Error: <message>` to stderr, exit 1.

### `remove-service <name>` (`remove_server_config`)
- `name` positional required (missing → `Error: Service name is required`), unknown flags rejected. Multiple names → `Error: Cannot use multiple command names for remove-service`, exit 1.
- `find_config_file(cwd)` (does NOT create; `MissingSetupFile` if absent). Filter out matching name.
- If length unchanged → `ConfigFileError("Service '<name>' not found in configuration")`.
- Revalidate, write, print `Service '<name>' removed from .candle.json`.
- CLI error wrapper: `Error: <message>`, exit 1.

### `set-config <key> <value>` (`handle_set_config`)
- Both positionals required (else `Error: set-config requires a <key> and a <value>`, exit 1), unknown flags rejected. `value` arrives as a **string** from CLI.
- Allowed keys (`VALID_CONFIG_KEYS`):
  - `logEviction.maxLogsPerService` → `js_positive_int(value)` must yield an integer `>= 1`, else `UsageError("Invalid value for 'logEviction.maxLogsPerService': expected a positive integer")`. Stored as **number**.
  - `logEviction.maxRetentionSeconds` → same rule/message with its key. Stored as number.
- Unknown key → `UsageError("Unknown config key '<key>'. Valid keys: logEviction.maxLogsPerService, logEviction.maxRetentionSeconds")`. Key/value are validated **before** the config file is read.
- Locates config via `find_config_file(cwd)` (errors if absent; does NOT create).
- Sets the field on `log_eviction` (creating it if absent, and appending `logEviction` to `key_order` if new). There is no generic dot-path setter.
- Revalidate, write, print `Set '<key>' to '<value>' in <config_filename>` (note: prints the `value` string as given, and the resolved `config_filename` which may be `.candle-setup.json`).
- CLI error wrapper: `Error: <message>`, exit 1.

**Subtle:** value parsing is lenient numeric coercion (`js_number` / `js_positive_int`), not `str::parse::<i64>()`: surrounding whitespace is trimmed (`" 5 "` → 5, passes), `""` → 0 (fails), `"3.5"` is not an integer (fails), `"3abc"` does not parse (fails), `"0x10"` → 16 and `"1e3"` → 1000 (pass; `0o`/`0b` prefixes are accepted too), and `Infinity`/`NaN` spellings fail.

## 8. Error types (`errors.rs`)
Config code raises these variants of the crate-wide `CandleError` enum (see [cli.md](cli.md) §1):
- `UsageError(String)` — bad CLI input (`set-config` key/value, invalid service name, missing root dir, no services configured).
- `ConfigFileError(String)` — invalid/unreadable config, duplicate or missing service in a mutating command, ambiguous loose match.
- `MissingServiceWithName { command_name, cwd }` — message `No service '<command_name>' configured for directory: <cwd>`.
- `MissingSetupFile { cwd, explicit }` — message `No .candle.json file found in (or above) current directory: <cwd>` plus a hint line.

The CLI prints every one the same way (`Error: <message>` on stderr, exit 1).

## 9. Service lookup (used by start/logs/etc., relevant for config consumers)

In `config/file.rs`.
- `find_service_by_name` / `get_all_service_names`: plain find/map by exact name.
- `resolve_command_names_or_all(project_dir, command_names)`: empty list → all service names; if config has zero services → `UsageError("No services configured in .candle.json")`.
- `get_service_config_by_name(command_name, current_dir: Option<&Path>)` (`None` → the process cwd): exact match first; else `find_loose_command_name`; neither → `CandleError::unknown_service(command_name, project_dir)`. Returns `FoundServiceConfig { service_config, project_dir }`.
- `find_loose_command_name(command_name, config, project_dir, current_dir)` — substring + directory-aware loose matching: finds services whose `name` *contains* `command_name`; among those, prefers ones whose resolved root equals the search dir; walks up parent dirs (stopping at `project_dir` or FS root); multiple dir-matches → `ConfigFileError("Ambiguous service name "<x>". Multiple services match in current directory: <names>")`; falls back to the single substring match if exactly one exists at the top. This is intricate, and the loose resolution drives `start <partial>` matching.

## 10. Dependencies

| Crate / API | Used for |
|---------|----------|
| `rusqlite` | SQLite open + `if not exists` schema statements; `PRAGMA journal_mode=WAL` and `PRAGMA busy_timeout=30000` are set after open. |
| `std::fs`, `std::path` | file IO, lexical path ops; lexical normalization is hand-rolled (not `canonicalize`). |
| `$HOME` env var | home dir for the default state dir. |
| `serde_json` (`preserve_order` feature) | config IO with key-order preservation, plus explicit `key_order` tracking and `to_string_pretty` (2-space). |
| `rust/src/cli/parser.rs` | hand-rolled CLI parser (strict: unknown flags rejected). |

Tests most sensitive to exact behavior: the stdout success strings, error messages (verbatim), 2-space JSON formatting with preserved key order and omitted falsy fields, empty-file handling, object-map `services`, the `..`-prefix path rule, and numeric coercion in `set-config`.
