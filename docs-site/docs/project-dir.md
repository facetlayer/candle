# Targeting another project

By default, Candle works out which project you mean from your current directory: it looks for a `.candle.json` there, then in each parent directory, and the first one it finds is the project.

The `--project-dir <dir>` option overrides that. The directory you name *is* the project, and your current directory is ignored:

```bash
candle ps --project-dir ~/work/api
candle start --project-dir ~/work/api web
candle logs --project-dir ~/work/api web
```

The path may be absolute or relative to your current directory.

## Supported commands

`--project-dir` is accepted by the commands that act on a single project:

[start](commands/start) · [run](commands/run) · [check-start](commands/check-start) · [restart](commands/restart) · [kill](commands/kill) · [list](commands/list) · [ps](commands/ps) · [logs](commands/logs) · [watch](commands/watch) · [wait-for-log](commands/wait-for-log) · [clear-logs](commands/clear-logs) · [list-ports](commands/list-ports) · [open-browser](commands/open-browser)

It is not accepted by the system-wide commands — [list-all](commands/list-all), [list-ports-all](commands/list-ports-all), [kill-all](commands/kill-all), and [find-orphans](commands/find-orphans) — which already cover every project.

## The directory must be the project itself

Unlike directory-based discovery, `--project-dir` never searches parent directories. Naming a subdirectory of a project is an error rather than a silent match on the parent:

```bash
candle ps --project-dir ~/work/api/src
# No .candle.json in /Users/you/work/api/src (--project-dir doesn't search parent directories)
```

This keeps a single command from reading its services out of one project while acting on another.

## Projects that no longer exist

There is one deliberate exception. Commands that work purely from Candle's own records — [kill](commands/kill), [logs](commands/logs), [clear-logs](commands/clear-logs), and [wait-for-log](commands/wait-for-log) — accept a `--project-dir` that has been deleted, or that no longer holds a config file.

This is what lets you clean up after a project you have removed:

```bash
candle kill --project-dir /path/to/deleted-project api
```

Because there is no config left to check the name against, an unrecognized service name is not an error here — the command simply reports that nothing by that name is running.

Use [find-orphans](commands/find-orphans) to list the running services in this situation.
