# watch

Watch live output from running service(s).

## Syntax

```bash
candle watch [name...]
```

## Description

The `watch` command displays real-time output from running services. Press
`Ctrl+C` to exit watch mode (services keep running in the background).

`watch` only observes — it never launches processes. To launch a service, use
[start](start), which also enters watch mode when run interactively.

- If called with no service names, `watch` always succeeds and watches every
  process in the project — including services that haven't launched yet, whose
  output will appear once they start.
- If called with service names, each named process must currently be running;
  otherwise `watch` fails with an error.

## Arguments

- `name` - Name of the service(s) to watch. Can specify multiple services. Each named service must be running.

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
