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

```bash
candle start            # start every service
candle start api        # start one
candle ls               # status, command and directory of each service
candle logs api         # recent output
candle kill api         # stop it
```

`candle start` returns once the process is launched. To wait until it is actually
ready, use `candle wait-for-log api --message "listening on"`.

## Optional: log retention

Candle keeps each service's output in a local database. Two optional keys
control how much is kept:

```bash
candle set-config logEviction.maxLogsPerService 5000     # default 1000 lines
candle set-config logEviction.maxRetentionSeconds 172800  # default 86400 (one day)
```

## Optional: commit the file

`.candle.json` contains nothing machine-specific, so it is safe to commit and
share with your team. Each clone or worktree of the project gets its own
independent set of services.
