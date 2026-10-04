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
fine too; changes take effect the next time a service is launched. A service that is
already running keeps its old command until you run `candle restart <name>`.

## 3. Start and check

What `candle start` does after launching depends on who runs it:

 - **In a terminal**, it launches the service and then watches its output. Press
   Ctrl-C to stop watching; **the service keeps running** in the background.
 - **From a script, a pipe, or a coding agent**, it returns as soon as the service
   is launched. Pass `--bg` to get this behavior anywhere.

Services only stop when you run `candle kill`.

```bash
candle start --bg                                    # start every service that isn't running, don't watch
candle wait-for-log api --message "listening on"     # block until api is ready
candle restart api                                   # kill and relaunch it, picking up config changes
candle ls                                            # status, command and directory of each
candle logs api                                      # recent output
candle list-ports                                    # which ports each service is listening on
candle kill                                          # stop every service (or: candle kill api)
```

`wait-for-log` only matches output from the current run, so text from an earlier run
of the service never counts as ready. It exits with an error if the message doesn't
show up before the timeout (`--timeout <seconds>`).

When the service is already running:

 - `candle start api` leaves it alone and says so. This makes it safe to call from
   setup scripts and agent routines.
 - `candle restart api` kills it and starts it again with the current `.candle.json`.
   `candle restart` with no names does this for every service, starting any that are stopped.

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
   `candle logs` keeps its output. Once you start it again, `candle logs` shows the
   new run; the crash output is still there with `candle logs <name> --previous`
   (or every run with `--all-runs`).
 - Each project (and each Git worktree) has its own services, but ports are still
   shared by the whole machine. If two worktrees both run a server on port 3000,
   the second one fails with "address in use". Give each checkout its own port, for
   example through a variable in its `shell` command.

## Troubleshooting: a service is running but `candle logs` is empty

Candle reads a service's output through a pipe, not a terminal. Many programs
notice that and stop printing line by line: they collect output in a buffer and
only write it out when the buffer fills (often 4-8 KB) or the program exits. The
service works, but its "listening on ..." line hasn't been written yet, so
`candle logs` shows nothing and `candle wait-for-log` times out. If the service is
killed, whatever was still in the buffer is lost.

Node.js, Go, and most programs that write through a logging library are not
affected. Python, Ruby, and C programs that use `printf` are. Error output
(stderr) is usually unbuffered, which is why you may see tracebacks but no
ordinary output.

The fix is to tell the program not to buffer, in the `shell` command:

| Program | Change the command to |
|---|---|
| Python | `python3 -u app.py`, or `PYTHONUNBUFFERED=1 python3 app.py` (also works for `flask`, `uvicorn`, `gunicorn`, `manage.py` and other Python entry points) |
| Ruby | Put `$stdout.sync = true` at the top of the program, or `ruby -e 'STDOUT.sync = true; load "./app.rb"'` |
| C / C++ and other programs using stdio | `stdbuf -oL ./server` (if macOS has no `stdbuf`: `brew install coreutils`, then `gstdbuf -oL ./server`) |
| `grep`, `sed`, `awk` in a pipeline | `grep --line-buffered`, `sed -u` (GNU sed; `sed -l` on macOS), `awk '{ print; fflush() }'` |
| Anything else | `script -q /dev/null ./server` on macOS, or `script -qfec "./server" /dev/null` on Linux, which runs the program on a terminal so it prints line by line |

For example:

```bash
candle add-service api --shell "PYTHONUNBUFFERED=1 python3 app.py"
```

To check whether buffering is the cause, run `candle kill <name>` and then
`candle logs <name>`: output that only shows up in bulk when the program exits
normally, or never shows up for a killed one, was sitting in a buffer.

## Commands that put something in the background

A `shell` command can start a process in the background and return, for example
`"./start-server.sh"` where the script ends with `server &`. Candle keeps the
service listed as running, and keeps collecting its output, for as long as
anything the command started is still running. `candle kill` stops all of it.
The exit status reported at the end is the shell command's own.

The exception is a program that detaches itself completely (a daemon that calls
`setsid`): Candle can't follow it. Run such programs in their foreground mode
(often a `--foreground` or `--no-daemon` flag).

If the command fails during startup, anything it had already put in the
background is stopped too, so a failed start leaves nothing running.

## Optional: log retention

Candle keeps each service's output in a local database. Two optional keys
control how much is kept:

```bash
candle set-config logEviction.maxLogsPerService 5000     # default 1000 lines
candle set-config logEviction.maxRetentionSeconds 172800  # default 86400 (one day)
```

These are cleanup targets, not hard caps. Old logs are removed at most once every
10 minutes, so a chatty service can go past the limit until the next cleanup. There
is a ceiling, though: a service that prints continuously is trimmed as it goes, to
its most recent 100,000 to 200,000 lines (or about 64 MB), and is slowed down to
the speed its output can be stored at rather than piling up in memory. A line longer
than 64 KB is stored as several lines.

## Optional: commit the file

With relative `root` paths and portable commands, `.candle.json` can be committed
and shared with your team. Absolute `root` paths also work but only make sense on
one machine, and a `shell` string can contain secrets or paths specific to one
machine. Check for both before committing. Each clone or worktree of the project
gets its own independent set of services.
