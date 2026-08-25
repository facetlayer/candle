# kill

Kill running services.

## Syntax

```bash
candle kill [name...] [--project-dir <dir>]
```

## Description

The `kill` command stops running services. `candle stop` is an alias for this command.

When called without arguments, it kills all services in the current project directory.

## Arguments

- `name` - Name of the service(s) to kill. If omitted, kills all services in the current project.

## Options

- `--project-dir <dir>` - Kill services in the given project instead of the current directory. See [Targeting another project](../project-dir).

## Examples

### Kill a specific service

```bash
candle kill api
```

### Kill multiple services

```bash
candle kill api web worker
```

### Kill all services in current project

```bash
candle kill
```

### Kill services in a project that has been deleted

Unlike other commands, `kill` accepts a `--project-dir` that no longer exists, or that no longer has a config file. This is how you clean up services left running by a project you have since removed:

```bash
candle kill --project-dir /path/to/deleted-project api
```

Because there is no config left to check against, an unrecognized name is not an error here — the command reports that nothing by that name is running. Use [find-orphans](find-orphans) to list services in this state.

## See Also

- [find-orphans](find-orphans) - Find services whose project is gone
- [kill-all](kill-all) - Kill all services globally
- [start](start) - Start services
- [restart](restart) - Restart services
