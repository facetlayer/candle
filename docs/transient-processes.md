# Transient Processes

Transient processes are processes started via CLI flags rather than being defined in your `.candle.json` configuration file. They provide a quick way to run one-off processes without modifying your config.

## Usage

```bash
candle start <name> --shell <command> [--root <path>]
```

### Examples

Start a simple process:
```bash
candle start myserver --shell "node server.js"
```

Start a process in a subdirectory:
```bash
candle start api --shell "npm run dev" --root "packages/api"
```

## Requirements

- A `.candle.json` file must exist in the current directory or a parent directory
- The `--shell` flag is required when starting a transient process
- If `--root` is provided, it must be a relative path within the project directory

## Behavior

### All Commands Work

Transient processes work with all standard candle commands:

```bash
candle logs myserver    # View process output
candle kill myserver    # Stop the process
candle restart myserver # Restart with same shell/root
candle ls               # Shows transient processes alongside config-defined ones
```

### Name Collision

If you start a transient process with the same name as a config-defined service (or another running process), the existing process is killed first:

```bash
# If 'web' is defined in .candle.json and running:
candle start web --shell "node other-server.js"
# Kills the running 'web' process and starts the new transient one
```

### Visibility in `ls`

- Transient processes appear in `candle ls` while running
- When killed, they disappear from the list (unlike config-defined services which show as "NOT LAUNCHED")
- If a transient process shadows a config-defined service, `ls` will show a `[config changed]` warning

### Restart Behavior

When you restart a transient process, it uses the shell command and root directory stored in the database from when it was originally started:

```bash
candle start foo --shell "npm start" --root "src"
# Later...
candle restart foo  # Uses the same "npm start" and "src" root
```

## Use Cases

- **Quick debugging**: Run a modified version of a service without changing config
- **Temporary services**: Start a one-off process that doesn't belong in permanent config
- **Testing**: Try different shell commands before committing to config
- **Development**: Run additional processes alongside your configured services

## MCP Integration

For MCP (Model Context Protocol) users:

- `StartService`: Only works with config-defined services
- `StartTransientService`: Use this action to start transient processes

This separation allows administrators to permit config-based services while restricting arbitrary command execution.
