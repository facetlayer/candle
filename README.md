# candle

Candle is a process manager designed for local development, worktrees, and AI agents.

Other process managers (like `pm2`) are built to run on production backends, but they
can be overcomplicated for local development. Candle aims to be your favorite process
manager for running services locally during development.

## Features ##

A few things that Candle does well:

### Everything is scoped to project directories ###

When running a command like `candle start`, it will automatically find the settings for
the current directory's project (similar to other tools like `git`). This helps keep
the interface simple.

This design fits naturally with worktrees - each worktree is already a separate directory,
so Candle will launch separate services for separate worktrees.

### One process per instance ###

Candle makes sure that each service is only launched as one process at a time (per directory)

### Agent-friendly CLI ###

Candle detects when the CLI is being launched by a coding agent, and it will always use
non-blocking responses (where the shell prints something and exits immediately) for
agents, which works better for them. Agents can use `candle logs ...` to fetch the
console logs for any running service.

### Other quality of life commands ###

Candle ships with a few other QOL features. One example is `candle wait-for-log...` which blocks
until a service has printed a certain message (for example, "Now serving on port ..."). This
is useful for CI jobs and integration tests that need to launch a service and wait till it's ready.

Another one is `candle list-ports` which uses the OS
to detect what ports the service(s) are using, and `candle open-browser` which uses port
detection to open a web browser to a locally running service.

## Installation ##

Supported on macOS and Linux, on both x86_64 and arm64.

### Installation via Homebrew

With Homebrew:

    brew install facetlayer/tap/candle


### Installation via Curl ###

Run:

    curl -fsSL https://raw.githubusercontent.com/facetlayer/candle/main/install.sh | sh

This downloads the latest [release](https://github.com/facetlayer/candle/releases) for your
platform, and installs into `~/.local/bin`. No Rust toolchain needed.

Confirm it worked:

    candle --version

If that prints `command not found`, you may need to add `~/.local/bin` to your `PATH`. Add this
to your shell profile (`~/.zshrc`, `~/.bashrc`) and open a new terminal:

    export PATH="$HOME/.local/bin:$PATH"

To install somewhere else, use `--bin-dir`:

    curl -fsSL https://raw.githubusercontent.com/facetlayer/candle/main/install.sh | sudo sh -s -- --bin-dir /usr/local/bin

### Installation from source ###

    git clone https://github.com/facetlayer/candle.git
    cd candle && ./install-local.sh

### Uninstalling ###

If installed with the script:

    curl -fsSL https://raw.githubusercontent.com/facetlayer/candle/main/install.sh | sh -s -- --uninstall

If installed with Homebrew:

    candle kill-all
    brew uninstall candle

## Quick Start ##

Initialize a `.candle.json` file in the root directory of your project.

    candle setup-project

Add services:

    candle add-service <service name> --shell <shell command> --root <optional root directory>

Launch it:

    candle start                # all services
    candle start [service name] # one service

# All Commands #

### `candle --help`

List all CLI commands. `candle help` does the same, and `candle help <command>` shows one command's help.

Run `candle <command> --help` to see the options for one command.

## Main usage commands ##

### `candle start`

    candle start
    candle start [service name(s)]

Launch the service(s).

If no `service names` are provided: then launch all services in the project.

If the service(s) are already running then the existing instances are killed first.
Concurrent starts of the same service are serialized, so they never leave duplicate instances.

If called in interactive mode (see "interactive mode detection" below), `start` will
then start watching the service and printing console messages. Press Ctrl-C to leave this mode.

Options:

 - `--watch` - Force interactive mode: watch logs after starting.
 - `--bg` - Force non-interactive mode: exit once started.

### `candle run`

Alias for `candle start`.

### `candle check-start`

Similar to `start` but only starts the service(s) if they are not already running.
If the service is running already, this command is a no-op. Unlike `start`, it never
watches logs afterward.

### `candle list`

    candle list
    candle list [service name(s)]

Alias: `candle ls`

Show details for the services in this project directory, including running and inactive services.
Each entry shows the service's full shell command and directory, untruncated:

```
$ candle list
web  RUNNING  pid 12345  uptime 3m 5s
  command:   npm run dev
  directory: /Users/andy/proj/web

api  not running
  command:   npm run api
  directory: /Users/andy/proj
```

Pass one or more service names to show only those services. Add `--json` for machine-readable output.

### `candle ps`

    candle ps
    candle ps [service name(s)]

Alias: `candle status`

The same services as `candle list`, but as a compact table. It leaves out the command and
directory columns so it stays narrow:

```
$ candle ps
NAME  STATUS       PID    UPTIME
----  -----------  -----  ------
web   RUNNING      12345  3m 5s
api   not running  -      -
```

Also accepts service names to filter, and `--json`.

### `candle watch`

    candle watch
    candle watch [service name(s)]

Start watching the logs for the service(s)

This will interactively print any log messages from the service as they happen.

If no `service names` are provided: Watch every process in the project (including
any processes that are launched after `watch` is started)

If service names are provided, those services must already be running. `watch` never launches
processes.

If multiple services are being watched, then the output lines will include prefixes
that looks like `[<service name>] ...`

`watch` blocks until Ctrl-C, so it refuses to run in agent mode (see "Interactive mode detection"
below), and agent mode leaves it out of `candle --help`. Agents should use `candle logs` instead.

Example:

    $ candle watch frontend backend
    [backend] Backend server now listening on port 3000
    [frontend] Web server available at http://localhost:8080


### `candle logs [names] [--count <number>] [--start-at <id>] [--json]`

Show the recent logs for the given service, from its most recent run.

If `[name]` is not provided: Show recent logs across all services in the project directory.
When more than one service is shown, each line is prefixed with `[<service name>]`, and the
`--count` limit applies to each service separately.

When `--count` cuts off earlier lines from the latest run, the output starts with
`-- showing the last N lines; use --count to see more --`.

A name that isn't configured and has no stored logs is an error (exit 1).

Options:

 - `--count <number>` - Number of log lines to show per service (default: 100).
 - `--start-at <id>` - Only show logs with an ID greater than `<id>`. IDs appear in `--json` output.
 - `--json` - Print the logs as a JSON array of `{ id, service, type, content, timestamp }`.

### `candle kill`

    candle kill
    candle kill [service name(s)]

Alias: `candle stop`

Kill named service(s)

If no `service names` are provided: Kill all services for this project directory.

Candle sends `SIGTERM` to the service and its child processes, and escalates to `SIGKILL` for any of
them still running 5 seconds later. The escalation is reported on stderr.

### `candle restart`

    candle restart
    candle restart [service name(s)]

Restart running service(s) for this current directory.

If no `service names` are provided: Restart all running services for this project directory

Config-defined services are reloaded from `.candle.json`, so edits to `shell` or `root` take
effect. Like `start`, `restart` watches the logs afterward in interactive mode, and accepts
`--watch` and `--bg`.

### `candle wait-for-log`

    candle wait-for-log [service name] --message [message]

Waits until the service has printed text to stdout or stderr that includes `[message]`.

This command is meant especially for CI jobs. In the CI context you often need to wait until
a service has fully launched before moving on to the next step.

Example usage:

```
    # Start the api server
    candle start api

    # Wait until it is ready
    candle wait-for-log api --message "server now listening"

    # Now run tests
    npm run test
```

The command will continue to wait until a certain timeout. The timeout defaults to 30 seconds and can be
set on the command line as `--timeout [seconds]`.

If the service isn't running (or its latest run already exited without printing the message), it fails
right away instead of waiting. On failure it prints the last 20 lines of the latest run and suggests
`candle logs <name>` for the rest.

The pattern of calling `start` then `wait-for-log` will do what you expect: it will wait
for the most recent process instance to print the log message, and won't be triggered if a
previous recent run has the same message.

## Port detection commands ##

### `candle list-ports`

    candle list-ports
    candle list-ports [service name(s)]

Uses the operating system to detect and list the active open ports for running services.

This command searches Candle managed processes and also child processes. It uses `lsof`
to find to find TCP ports that are in a LISTEN state

If no `[names]` are provided: Show ports for all running services in the current project.
A name that isn't a service in this project is an error. Pass `--json` for machine-readable output.

### `candle open-browser`

    candle open-browser
    candle open-browser [service name]

Attempts to detect the listening port for a target service, then opens a web
browser to `http://localhost:<port>` for that service.

If no service name is provided, the project must have exactly one service running.

The port is auto-detected using the same logic as `list-ports`. If the service is
listening on more than one port, Candle opens the lowest-numbered one, which may not
be the one you want. But in most simple cases it works pretty well.

## Project setup commands ##

### `candle setup-project`

Create a new `.candle.json` config file in the current directory.

### `candle add-service`

    candle add-service [service name] --shell [command]
    candle add-service [service name] --shell [command] --root [root directory]

Add a new service to the nearest `.candle.json` config file.

If the config file doesn't exist yet, it will be created in the current directory.

### `candle remove-service`

    candle remove-service [service name]

Remove a service from the nearest `.candle.json` config file.

### `candle set-config`

    candle set-config <key> <value>

Set a configuration option in `.candle.json`. The valid keys are `logEviction.maxLogsPerService`
(default 1000) and `logEviction.maxRetentionSeconds` (default 86400). Both take a positive integer.

# Less frequently used commands #

Other CLI commands that are not typically used:

### `candle mcp` or `candle --mcp`

Run Candle in MCP mode, using stdin as the transport.

Note that it's now recommended for coding agents to use the `candle` CLI over Bash,
instead of using the MCP server. But this is provided as an option.

### `candle list-all`

List all processes (across the entire system) that were launched by Candle.

### `candle kill-all`

Kill all processes (across the entire system) that were launched by Candle.

### `candle find-orphans`

List running services whose project no longer accounts for them: the project directory was
deleted, its config file was removed, or the service was dropped from the config. Scans every
project on the system.

Supports `--json`.

Clean one up with `candle kill --project-dir <project> <service>`, which accepts a project
directory that no longer exists.

### `candle list-ports-all`

Like `list-ports` but shows open ports for all Candle-managed processes across the entire system.
Works from any directory, and also takes `--json`.

### `candle clear-logs`

    candle clear-logs [service name(s)]

Delete the stored logs for the named service(s) in this project.

If no `service names` are provided: Delete the logs for every service in this project.

### `candle list-docs` and `candle get-doc <name>`

List and print the documentation files built into the binary (the files in `./docs` plus this README).
`get-doc` takes the name `list-docs` shows (for example `candle get-doc getting-started`) and matches it
exactly.

### `candle erase-database`

Delete Candle's database (by default in `~/.local/state/candle`; see "Database location" below).

This command can help if the database is corrupted or it needs a full SQL schema rebuild.

It refuses to run while Candle-managed services are still running, since erasing would leave them
running untracked. Run `candle kill-all` first, or pass `--force` to erase anyway.

# Targeting another project #

By default Candle finds your project by looking for a `.candle.json` in the current directory,
then in each parent directory. `--project-dir <dir>` overrides that: the named directory *is*
the project, and the current directory is ignored. The path may be relative.

    candle ps --project-dir ~/work/api
    candle start --project-dir ~/work/api web

Accepted by the commands that act on a single project: `start`, `run`, `check-start`, `restart`,
`kill`, `list`, `ps`, `logs`, `watch`, `wait-for-log`, `clear-logs`, `list-ports`, and
`open-browser`. The system-wide commands (`list-all`, `list-ports-all`, `kill-all`,
`find-orphans`) don't take it.

Unlike the default search, `--project-dir` never falls back to a parent directory. Naming a
subdirectory of a project is an error rather than a silent match on the parent.

## Projects that no longer exist ##

`kill`, `logs`, `clear-logs`, and `wait-for-log` work purely from Candle's own records, so they
accept a `--project-dir` that has been deleted or no longer has a config file:

    candle kill --project-dir /path/to/deleted-project api

Because there's no config left to check the name against, an unrecognized service name there
reports that nothing is running instead of failing. Use `candle find-orphans` to list the
services in this situation.

# Interactive mode detection #

Several Candle commands have different behavior depending if they are running in an interactive
or non-interactive context.

Candle uses **interactive mode** only when:

 - Stdout is a TTY. Piping or redirecting output (`candle start | tee log.txt`) makes it non-interactive.
 - And, no coding-agent environment variables are detected. If any are, Candle assumes an agent is
   driving the CLI. This is "agent mode".

Candle currently checks for these environment variables to detect a coding agent: `CLAUDECODE`,
`GEMINI_CLI`, `CURSOR_AGENT`. A variable counts only when it is set to a non-empty value.

# Database location #

Candle stores service state and logs in a SQLite database, `candle.db`, in its state directory:

 - `$CANDLE_DATABASE_DIR` if set,
 - otherwise `$XDG_STATE_HOME/candle` if `XDG_STATE_HOME` is set,
 - otherwise `~/.local/state/candle`.

# License #

[MIT](./LICENSE)

