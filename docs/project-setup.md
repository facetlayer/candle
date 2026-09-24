---
name: project-setup
description: Set up a project to use Candle, including the .candle.json config file
---

# Project Setup

Candle organizes services by project. A project is any directory that has a
`.candle.json` file; commands run from that directory (or any subdirectory)
act on the services defined there.

## 1. Create the config file

From the root of your project:

```bash
candle setup-project
```

This writes an empty `.candle.json`. You can also create the file by hand.

## 2. Add services

Each service has a name and a shell command. Add one with `add-service`:

```bash
candle add-service api --shell "npm run dev"
candle add-service worker --shell "npm run worker" --root packages/worker
```

`--root` is optional and sets the directory the command runs in, relative to
the project root. Without it, the command runs in the project root.

After those two commands, `.candle.json` looks like:

```json
{
  "services": [
    {
      "name": "api",
      "shell": "npm run dev"
    },
    {
      "name": "worker",
      "shell": "npm run worker",
      "root": "packages/worker"
    }
  ]
}
```

Fields for each service:

 - `name` (required): how you refer to the service in commands. Letters, digits, `-`, `_` and `.`.
 - `shell` (required): the command to run, executed by the shell.
 - `root` (optional): working directory for the command. Relative paths are resolved
   against the project root and can't escape it.

Remove a service with `candle remove-service <name>`. Editing the file by hand is
fine too; changes take effect the next time a service is started or restarted.

## 3. Start and check

What `candle start` does after launching depends on who runs it:

 - **In a terminal**, it launches the service and then watches its output. Press
   Ctrl-C to stop watching; **the service keeps running** in the background.
 - **From a script, a pipe, or a coding agent**, it returns as soon as the service
   is launched. Pass `--bg` to get this behavior anywhere.

Services only stop when you run `candle kill`.

```bash
candle start --bg                                    # start every service, don't watch
candle wait-for-log api --message "listening on"     # block until api is ready
candle ls                                            # status, command and directory of each
candle logs api                                      # recent output
candle list-ports                                    # which ports each service is listening on
candle kill                                          # stop every service (or: candle kill api)
```

`wait-for-log` only matches output from the current run, so text from an earlier run
of the service never counts as ready. It exits with an error if the message doesn't
show up before the timeout (`--timeout <seconds>`).

Three ways to launch, which differ when the service is already running:

 - `candle start api`: replaces it (kills the running instance, then starts a new one).
 - `candle check-start api`: leaves it alone and only starts api if it's not running.
   This is usually the right call in setup scripts and agent routines.
 - `candle restart api`: kills it and starts it again with the current `.candle.json`.

## How services run

 - Each `shell` string runs with `sh -c`, so shell syntax (`&&`, pipes, `$VAR`) works,
   but aliases and functions from your interactive shell profile don't.
 - A service gets the environment of the command that started it, captured at that
   moment. To pick up a changed variable, run `candle restart` from a shell that has
   it. Candle doesn't read `.env` files; load them in the command itself
   (e.g. `"shell": "set -a && . ./.env && npm run dev"`).
 - Services are independent: there's no start order or dependency graph. Use
   `wait-for-log` to wait for one before starting another.
 - Candle doesn't restart a service when it crashes, and doesn't restart services
   after a reboot. `candle ls` shows a crashed service as `EXITED (<code>)`, and
   `candle logs` keeps its output.
 - Each project (and each Git worktree) has its own services, but ports are still
   shared by the whole machine. If two worktrees both run a server on port 3000,
   the second one fails with "address in use". Give each checkout its own port, for
   example through a variable in its `shell` command.

## Optional: log retention

Candle keeps each service's output in a local database. Two optional keys
control how much is kept:

```bash
candle set-config logEviction.maxLogsPerService 5000     # default 1000 lines
candle set-config logEviction.maxRetentionSeconds 172800  # default 86400 (one day)
```

These are cleanup targets, not hard caps. Old logs are removed at most once every
10 minutes, so a very chatty service can go well past the limit (and use more disk)
until the next cleanup.

## Optional: commit the file

With relative `root` paths and portable commands, `.candle.json` can be committed
and shared with your team. Absolute `root` paths also work but only make sense on
one machine, and a `shell` string can contain secrets or paths specific to one
machine. Check for both before committing. Each clone or worktree of the project
gets its own independent set of services.
