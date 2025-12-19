# Config File Behavior

This document describes how Candle finds and loads configuration files.

## Config Filenames

Candle supports two config filenames:
- `.candle.json` (preferred, checked first)
- `.candle-setup.json` (deprecated, checked second for backwards compatibility)

When creating a new config file (e.g., via `add-service`), Candle uses `.candle.json` by default.

Note: `.candle-setup.json` is deprecated. New projects should use `.candle.json`. Existing projects using `.candle-setup.json` will continue to work, but consider renaming to `.candle.json`.

## Finding the Config File

Candle uses the `findConfigFile` function (in `src/configFile.ts`) to locate the configuration file. The search behavior is:

1. Start from the current working directory (or a specified directory)
2. Look for `.candle.json` in that directory
3. If not found, look for `.candle-setup.json` in that directory
4. If neither is found, move to the parent directory and repeat
5. Continue until either:
   - A config file is found, OR
   - The filesystem root is reached

## When Errors Are Thrown

### MissingSetupFileError

The `MissingSetupFileError` is thrown when:

- No config file (`.candle.json` or `.candle-setup.json`) exists in the current directory or any parent directory
- The command being run requires a config file (e.g., `run`, `start`, `list`, `kill`, etc.)

This error is thrown by `findConfigFile` and `findProjectDir` functions.

### ConfigFileError

The `ConfigFileError` is thrown when:

- The config file exists but contains invalid JSON
- The `services` field has an invalid type (not an array or object)
- A service is missing required fields (`name` or `shell`)
- Duplicate service names are detected
- A service has an invalid `root` path (absolute paths or paths that escape the config directory)

## When Errors Are NOT Thrown

### add-service Command

The `add-service` command has special handling. When no config file exists:

1. The `findOrCreateSetupFile` function (in `src/addServerConfig.ts`) catches the `MissingSetupFileError`
2. Instead of failing, it creates a new `.candle.json` file in the current directory
3. The new config file is initialized with an empty services array: `{ "services": [] }`
4. The requested service is then added to this new config file

When a config file already exists (either `.candle.json` or `.candle-setup.json`), the service is added to the existing file.

This allows users to quickly set up a new project without manually creating the config file first.

## Config File Structure

```json
{
  "services": [
    {
      "name": "web",
      "shell": "npm start"
    },
    {
      "name": "api",
      "shell": "node server.js",
      "root": "backend"
    }
  ]
}
```

### Service Fields

- `name` (required): Unique identifier for the service
- `shell` (required): Shell command to run the service
- `root` (optional): Relative path from the config file location to the service's working directory

### Alternative Object Syntax

Candle also supports an object-style configuration that gets automatically converted to the array format:

```json
{
  "services": {
    "web": { "shell": "npm start" },
    "api": { "shell": "node server.js", "root": "backend" }
  }
}
```

## Project Directory

When a config file is found, the directory containing that file is considered the "project directory". This is important because:

- Service `root` paths are resolved relative to this directory
- Process records in the database are associated with this project directory
- The `list` command shows services for the current project directory

## Validation Rules

1. **Service names must be unique** - Duplicate names result in a `ConfigFileError`
2. **Root paths must be relative** - Absolute paths are rejected
3. **Root paths cannot escape** - Paths like `../outside` are rejected (after normalization)
4. **Shell commands are required** - Each service must have a `shell` field
