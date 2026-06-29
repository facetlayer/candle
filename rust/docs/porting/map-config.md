I now have everything needed. Here is the spec.

---

# Candle "config" Subsystem — Rust Reimplementation Spec

Covers config file discovery/parsing/validation, the `.candle.json` schema, state/database directory resolution, and the `add-service` / `remove-service` / `set-config` / `setup-project` commands. Source files: `src/configFile.ts`, `src/addServerConfig.ts`, `src/removeServerConfig.ts`, `src/set-config-command.ts`, `src/setup-project-command.ts`, `src/dirs.ts`, `src/findPackageJson.ts`, with CLI wiring in `src/main-cli.ts` and DB dir usage in `src/database/database.ts`.

## 1. The `.candle.json` file format

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

Two filenames, **priority order, first match wins** (configFile.ts:29-30):
```
CONFIG_FILENAMES = ['.candle.json', '.candle-setup.json']   // .candle-setup.json is deprecated
DEFAULT_CONFIG_FILENAME = '.candle.json'
```

`findConfigFile(currentDir)` (configFile.ts:68-98):
1. Resolve `currentDir` to absolute (`path.resolve`); if falsy, use `process.cwd()`.
2. Loop: in the current dir, test each filename in `CONFIG_FILENAMES` order. First existing file is read+validated and returned as `{ config, projectDir: currentDir, configFilename }`.
3. If reading/parsing throws, it is re-wrapped as `Error("Invalid <filename> at <path>: <msg>")` (configFile.ts:84) — note this wrapping loses the `MissingSetupFileError` type for parse errors of an existing file.
4. Move to parent dir (`path.dirname`). Stop when `path.dirname(dir) === dir` (filesystem root).
5. If none found, throw `MissingSetupFileError(startingDir)` — message: `No .candle.json file found in (or above) current directory: <startingDir>` (errors.ts:38-42). Note: the error reports the **original** starting dir, not the root.

`findProjectDir(cwd = process.cwd())` (configFile.ts:37-44) just returns `findConfigFile(cwd).projectDir`, re-throwing `MissingSetupFileError` on miss.

**Subtle:** the directory walk stops at FS root via the `dirname(x) === x` fixpoint. On macOS/Linux that's `/`; in Rust use `Path::parent()` returning `None` as the stop condition. There is no `$HOME` boundary — it walks all the way to root.

## 3. Parsing & validation

`readConfigFile(path)` (configFile.ts:49-63):
- Read file as UTF-8, `.trim()`.
- **Empty file (after trim) is valid** and returns `{ services: [] }` (no validation run on this path).
- Otherwise `JSON.parse`, normalize `config.services ||= []`, then `validateConfig`.

`validateConfig(config)` (configFile.ts:103-189):
1. **services as object map** (configFile.ts:108-124): If `services` is not an array but is a non-null object, convert each `[key, value]` into `{ name: key, ...value }`. If it's a non-object non-array (e.g. string/number), throw `ConfigFileError: Config file error: Invalid value for 'services': <JSON>`.
2. For each service:
   - `name` must be truthy string → else `ConfigFileError: Config file error: Each service must have a "name" string`.
   - `shell` must be truthy string → else `ConfigFileError: Config file error: Service "<name>" must have a "shell" string`.
   - Duplicate name → `ConfigFileError: Config file error: Duplicate service name: "<name>"`.
   - If `root` present and `!isValidRootPath(root)` → `ConfigFileError: Service "<name>" has invalid root path: "<root>"`.
3. `logEviction` if present must be a non-null, non-array object, else `ConfigFileError: Config file error: Invalid value for 'logEviction': expected an object`. Each of `maxLogsPerService` / `maxRetentionSeconds`, if present, must be an integer `>= 1`, else `ConfigFileError: Config file error: 'logEviction.<field>' must be a positive integer`.
4. `logCollector` if present must be exactly `"node"` or `"rust"`, else `ConfigFileError: Config file error: Invalid value for 'logCollector': expected 'node' or 'rust', got '<value>'`.
5. Returns `{ ...config, services }` (object-map normalization persisted; defaults NOT injected).

**Validation does not reject unknown top-level or unknown per-service keys.**

### Path validation helpers (configFile.ts:192-219)
- `isValidRootPath(p)`: absolute → always valid. Else `path.normalize(p)`; invalid if it `startsWith('..')`. (So `../x` invalid, `a/../b` → normalizes to `b` valid, `a/../../b` → `../b` invalid.)
- `isValidRelativePath(p)`: absolute → invalid; else same `..` check.

**Subtle for Rust:** `path.normalize` is *lexical* (does not touch the filesystem) and collapses `.`/`..` segments. Use a lexical normalizer (e.g. the `path-clean` crate or manual segment folding) — do NOT use `canonicalize` (which hits the FS and resolves symlinks). The `startsWith("..")` check is a string prefix on the normalized result, so it also matches a path literally named `..foo` — match this quirk exactly (prefix test, not segment test).

### Service cwd resolution (`getServiceCwd`, configFile.ts:238-244)
`configDir = dirname(configPath)`; if `service.root` set → `path.resolve(configDir, root)` (absolute root wins); else `configDir`.

## 4. State / database directory resolution (`dirs.ts`)

`getStateDirectory()` (dirs.ts:9-22), in order:
1. `CANDLE_DATABASE_DIR` env set → return it **verbatim** (no `candle` suffix appended).
2. `XDG_STATE_HOME` env set → `join(XDG_STATE_HOME, 'candle')`.
3. Default → `join(homedir(), '.local', 'state', 'candle')`.

DB file: `join(stateDir, 'candle.db')` (database.ts:66). The state dir is created recursively if missing (database.ts:62-63). `getDatabase` accepts an `overrideDirectory` that bypasses `getStateDirectory` entirely (database.ts:57-60).

**Platform note:** `os.homedir()` in Node is `$HOME` on Unix and `%USERPROFILE%` on Windows. In Rust, replicate with the `dirs`/`directories` crate or `$HOME`/`USERPROFILE` env. The default path uses literal `.local/state/candle` on ALL platforms (Node code is not platform-branching here), so do not substitute Windows AppData unless matching Node's `os.homedir()` semantics for the home portion only.

`ProjectRootDir` (dirs.ts:6) = dir-above-`dirs.ts` — used to locate the installed package; not relevant to config logic. `findPackageJson` (findPackageJson.ts) tries `../package.json` then `../../package.json` relative to the module file to read `{name, version}` — used for `--version`; a Rust build embeds version via `env!("CARGO_PKG_VERSION")` instead.

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

All three mutating commands write with `JSON.stringify(config, null, 2)` (2-space indent, no trailing newline) — addServerConfig.ts:40, removeServerConfig.ts:20, set-config-command.ts:64. Object key order follows JS object insertion order: for a freshly created file it is `{ "services": [] }`. For `set-config`, new keys are appended after existing ones. **Rust must use an order-preserving serializer** (e.g. `serde_json` with `preserve_order` feature on `Map`, or `indexmap`) and 2-space pretty printing to match byte-for-byte where tests check file contents.

## 7. Commands

### `setup-project` (setup-project-command.ts; CLI main-cli.ts:185, 472)
- No args/options (`strictOptions`). Operates on `process.cwd()`.
- If `findConfigFile(cwd)` succeeds → print `Config file already exists at <configPath>` and return (no write). `<configPath>` = `join(projectDir, configFilename)`.
- Else (only if error is `MissingSetupFileError`; otherwise rethrow) → write `{ "services": [] }` to `join(cwd, '.candle.json')` and print `Created .candle.json in <cwd>`.

### `add-service <name> --shell <s> [--root <r>] [--enable-stdin]` (addServerConfig.ts; CLI main-cli.ts:186-205, 477)
- `name` positional required. `--shell` `demandOption: true`. `--root` string optional. `--enable-stdin` boolean optional. `strictOptions` (unknown flags rejected by yargs).
- CLI rejects multiple command names: prints `Error: Cannot use multiple command names for add-service` to stderr + `exit(1)`.
- `findOrCreateSetupFile(cwd)`: if a config exists upward, use `join(projectDir, configFilename)`; if `MissingSetupFileError`, create `{ "services": [] }` at `join(cwd, '.candle.json')` (other errors rethrow).
- Read config. If a service with `name` exists → throw `Error("Service '<name>' already exists in configuration")`.
- Build new service object with fields in this exact insertion order: `name`, `shell`, then `root` **only if truthy**, then `enableStdin` **only if truthy** (addServerConfig.ts:26-31 — falsy values are omitted entirely).
- Push, `validateConfig`, write, print `Service '<name>' added successfully to .candle.json`.
- On any thrown error the CLI prints `Error adding service: <message>` + `exit(1)` (main-cli.ts:490-492).

### `remove-service <name>` (removeServerConfig.ts; CLI main-cli.ts:207-213, 507)
- `name` positional required, `strictOptions`. Multiple names → `Error: Cannot use multiple command names for remove-service` + `exit(1)`.
- `findConfigFile(cwd)` (does NOT create — throws `MissingSetupFileError` if absent). Filter out matching name.
- If length unchanged → throw `Error("Service '<name>' not found in configuration")`.
- `validateConfig`, write, print `Service '<name>' removed from .candle.json`.
- CLI error wrapper: `Error removing service: <message>` + `exit(1)`.

### `set-config <key> <value>` (set-config-command.ts; CLI main-cli.ts:215-225, 497)
- Both positionals required, `strictOptions`. `value` arrives as a **string** from CLI.
- Allowed keys (`VALID_CONFIG_KEYS`, set-config-command.ts:6-36):
  - `logCollector` → must be `"node"`/`"rust"`, else `UsageError("Invalid value for 'logCollector': expected 'node' or 'rust', got '<value>'")`. Stored as string.
  - `logEviction.maxLogsPerService` → `Number(value)` must be integer `>= 1`, else `UsageError("Invalid value for 'logEviction.maxLogsPerService': expected a positive integer")`. Stored as **number**.
  - `logEviction.maxRetentionSeconds` → same rule/message with its key. Stored as number.
- Unknown key → `UsageError("Unknown config key '<key>'. Valid keys: logCollector, logEviction.maxLogsPerService, logEviction.maxRetentionSeconds")`.
- Locates config via `findConfigFile(cwd)` (throws if absent — does NOT create).
- Sets via dot-path: 1 part → top-level key; 2 parts → create parent object `{}` if absent then set child (set-config-command.ts:53-60). Deeper nesting not supported (silently no-op for >2 parts).
- `validateConfig`, write, print `Set '<key>' to '<value>' in <configFilename>` (note: prints the original string `value`, and the resolved `configFilename` which may be `.candle-setup.json`).
- CLI error wrapper: `Error: <message>` + `exit(1)`.

**Subtle:** `Number(value)` parsing — JS `Number("")` = 0 (fails `<1`), `Number("3.5")` not integer (fails), `Number("3abc")` = NaN (fails `Number.isInteger`), `Number(" 5 ")` = 5 (whitespace trimmed, passes), `Number("0x10")`=16, `Number("1e3")`=1000 (integer, passes). Match this with a parser that mimics JS `Number()` coercion, not Rust's `str::parse::<i64>()`, to be exact — especially the leading/trailing-whitespace and `1e3`/hex acceptance.

## 8. Error types (errors.ts)
- `UsageError` — has `isUsageError = true`, `name='UsageError'`.
- `ConfigFileError` — `name='ConfigFileError'` (no `isUsageError`).
- `MissingServiceWithNameError` — `name='NeedRunCommandError'`, `isUsageError=true`, message `No service '<commandName>' configured for directory: <cwd>`.
- `MissingSetupFileError` — `name='MissingSetupFile'`, `isUsageError=true`, message `No .candle.json file found in (or above) current directory: <cwd>`.

In Rust model these as an enum (e.g. `ConfigError`) with variants carrying the same data; the `isUsageError` flag distinguishes user-facing (exit 1, no stack) from internal errors at the CLI boundary.

## 9. Service lookup (used by start/logs/etc., relevant for config consumers)
- `findServiceByName` / `getAllServiceNames` (configFile.ts:246-252): plain `.find`/`.map` by exact name.
- `resolveCommandNamesOrAll` (configFile.ts:259-269): empty list → all service names; if config has zero services → `UsageError("No services configured in .candle.json")`.
- `getServiceConfigByName` (configFile.ts:326-349): exact match first; else `findLooseCommandName`.
- `findLooseCommandName` (configFile.ts:276-324) — substring + directory-aware loose matching: finds services whose `name` *contains* `commandName`; among those, prefers ones whose resolved root equals the search dir; walks up parent dirs (stopping at projectDir or FS root); multiple dir-matches → `Error("Ambiguous service name "<x>". Multiple services match in current directory: <names>")`; falls back to the single substring match if exactly one exists at the top. This is intricate — replicate carefully if `start <partial>` resolution must match.

## 10. External npm dependencies → Rust crates

| npm dep | Used for | Rust replacement |
|---------|----------|------------------|
| `@facetlayer/sqlite-wrapper` (`DatabaseLoader`, `SqliteDatabase`) | SQLite open + migrations (`safe-upgrades`) | `rusqlite` (+ a hand-rolled migration runner, or `rusqlite_migration`). Set `PRAGMA journal_mode=WAL` and `PRAGMA busy_timeout=30000` after open. |
| Node `fs` / `path` | file IO, lexical path ops | `std::fs`, `std::path`; lexical normalize via `path-clean` crate (do not `canonicalize`). |
| Node `os.homedir()` | home dir | `dirs`/`directories` crate or `$HOME`/`USERPROFILE`. |
| `JSON.parse` / `JSON.stringify(_,null,2)` | config IO with key-order preservation | `serde_json` with `preserve_order` feature (uses `indexmap` internally) + `serde_json::to_string_pretty` (2-space). |
| `yargs` | CLI parsing (`strictOptions`, `demandOption`, positionals) | `clap` (strict, required args). |

## Rust reimplementation notes — modules & ordering

Build bottom-up; each item depends on the ones above it.

1. **`errors`** — `ConfigError` enum (UsageError/ConfigFileError/MissingService/MissingSetupFile variants) with `is_usage_error()` and exact `Display` strings from §3/§7/§8.
2. **`dirs`** — `get_state_directory()` (env precedence `CANDLE_DATABASE_DIR` → `XDG_STATE_HOME`+"candle" → `home/.local/state/candle`); version via `env!("CARGO_PKG_VERSION")` (replaces `findPackageJson`).
3. **`config::model`** — structs `CandleSetupConfig`, `ServiceConfig`, `LogEvictionConfig`; serde with `preserve_order`, `skip_serializing_if` for optional `root`/`enableStdin`, and a custom deserializer for `services` accepting both array and object-map forms. Constants `CONFIG_FILENAMES`, `DEFAULT_CONFIG_FILENAME`, `LOG_EVICTION_DEFAULTS`.
4. **`config::paths`** — `is_valid_root_path`, `is_valid_relative_path` (lexical normalize + `..` prefix test), `get_service_cwd`.
5. **`config::validate`** — `validate_config` with the exact error messages and object-map coercion; returns normalized config.
6. **`config::file`** — `read_config_file` (trim/empty handling, JSON parse, normalize+validate), `find_config_file` (upward walk, filename priority, parse-error wrapping), `find_project_dir`, plus `get_log_eviction_config`, `find_service_by_name`, `get_all_service_names`, `resolve_command_names_or_all`, and the loose-match resolver (`find_loose_command_name` / `get_service_config_by_name`).
7. **`commands`** — `handle_setup_project`, `add_server_config` (incl. `find_or_create_setup_file`), `remove_server_config`, `handle_set_config` (with `VALID_CONFIG_KEYS` table, JS-`Number()`-style coercion, dot-path setter). Each prints the exact stdout strings in §7 and writes via the order-preserving 2-space serializer.
8. **`database` (bootstrap only)** — `get_database(override_dir)` creating state dir, opening `candle.db` with the §5 schema + WAL/busy_timeout. Needed by service-lookup paths (`getServiceInfoByName`) but not by pure config mutation.
9. **CLI wiring** (clap) — map `setup-project` / `add-service` / `remove-service` / `set-config` with the flags in §7, the "multiple command names" guards, and the `Error ...: <msg>` + `exit(1)` wrappers.

Tests most sensitive to exact behavior: the stdout success strings, error messages (verbatim), 2-space JSON formatting with preserved key order and omitted falsy fields, empty-file handling, object-map `services`, the `..`-prefix path rule, and JS `Number()` coercion in `set-config`.