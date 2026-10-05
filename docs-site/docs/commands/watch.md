# watch

Watch live output from running service(s).

## Syntax

```bash
candle watch [name...]
```

## Description

The `watch` command displays real-time output from running services. Press
`Ctrl+C` to exit watch mode (services keep running in the background).

The watch command is prevented for coding agents (such as Claude Code).
If the tool detects an agent, 
`watch` exits with an error and gives instructions to use `candle logs`.

- If called with no service names, `watch` always succeeds and watches every
  process in the project — including services that haven't launched yet, whose
  output will appear once they start.
- If called with service names, each named process must currently be running;
  otherwise `watch` fails with an error. A name that isn't configured in the
  project fails with `No service '<name>' configured for directory: <dir>`.

## Arguments

- `name` - Name of the service(s) to watch. Can specify multiple services. Each named service must be running.

## Options

- `--project-dir <dir>` - Act on the given project instead of the current directory. See [Targeting another project](../project-dir).

## Examples

### Watch a single running service

```bash
candle watch api
```

### Watch multiple running services

```bash
candle watch api web
```

### Watch everything in this project

```bash
candle watch
```

## See Also

- [start](start) - Start services (and watch them, when run interactively)
- [logs](logs) - View recent logs (non-interactive)
