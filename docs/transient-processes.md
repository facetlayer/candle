---
name: transient-processes
description: Start one-off processes with --shell without adding them to .candle.json
---

# Transient Processes

A transient process is started from the command line instead of `.candle.json`.
Use it to run something once without editing the config.

```bash
candle start <name> --shell <command> [--root <dir>]
```

Examples:

```bash
candle start myserver --shell "node server.js"
candle start api --shell "npm run dev" --root packages/api
```

A `.candle.json` must exist in the current directory or a parent, since the
process still belongs to a project. `--shell` is required. `--root` is relative
to the project root and can't escape it.

## Behavior

 - Once started, a transient process works with every command: `logs`, `kill`,
   `restart`, `ls`, `list-ports`, `wait-for-log`.
 - `restart` reuses the `--shell` and `--root` it was started with. If the name
   is also defined in `.candle.json`, `restart` uses the config instead.
 - To change a running transient process's command, pass the new one to
   `restart`: `candle restart myserver --shell "node server.js --port 4000"`.
 - If a service with that name is already running (configured or transient),
   `start` leaves it alone: with the same command it reports that it's already
   running, and with a different command it fails and points at
   `candle restart <name> --shell <cmd>`. When a transient process replaces a
   configured service of the same name, `ls` flags it with `[config changed]`.
 - It appears in `ls` only while running. Once killed or exited it drops off the
   list, unlike a configured service, which stays listed as not running.

## MCP

The MCP server exposes transient starts as a separate tool,
`StartTransientService`, so a client can allow `StartService` while denying
arbitrary commands. See `candle get-doc mcp-usage`.
