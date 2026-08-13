# candle

Candle is a process manager designed for local development, worktrees, and AI agents.

Other process managers (like `pm2`) are built to run on production backends but they
can be overcomplicated for local development. Candle aims to be your favorite process
manager when developing and running things locally.

## Features ##

A few things that Candle does well:

### Everything is scoped to project directories ###

When running a command like `candle start`, it will automatically find the services for
the current directory's project, just like other tools like `git`. This helps keep
the commands simple.

This also fits naturally with worktrees - each worktree is already a separate directory,
so Candle will launch separate services for separate worktrees.

### One process per instance ###

Candle will make sure that each service is only launched into one process at a time (per directory)

### Agent-friendly CLI ###

Candle detects when the CLI is being launched by a coding agent, and it will always use
non-blocking responses (where the shell prints something and exits immediately) for
agents, which works better for them. Agents can use `candle log ...` to fetch & search the
console logs for any running service.

### Other quality of life commands ###

Candle ships with a few other QOL features. One example is `candle wait-for-log...` which blocks
until a service has printed a certain message (for example, "Now serving on port ..."). This
command can be dropped in to integration tests to help them wait until a service is fully launched.

Another example is port detection - Candle ships with `candle list-ports` which uses the OS
to detect what ports the service(s) are using, and `candle open-browser` which uses port
detection to open a web browser to a localhost service. This also helps in worktrees since
you'll typically have different trees using different port assignments.

## Installation ##

Supported on macOS and Linux, on both x86_64 and arm64.

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

### Installation via Homebrew

With Homebrew:

    brew install facetlayer/tap/candle

This installs the same prebuilt binary as the curl installer, so no Rust toolchain is needed.

### Uninstalling ###

Shut down running services first — Candle launches them detached, so they outlive the binary:

    candle kill-all

Then remove it:

    # installed with the script
    curl -fsSL https://raw.githubusercontent.com/facetlayer/candle/main/install.sh | sh -s -- --uninstall

    # installed with Homebrew
    brew uninstall candle

## Quick Start ##

Initialize a `.candle.json` file in the root directory of your project (usually the same
place that has the `.git` directory):

    $ candle setup-project

Add a service:

    $ candle add-service <service name> --shell <shell command>

Launch it:

    $ candle start                # all services
    $ candle start <service name> # one services

# All Commands #

### `candle --help`

List all CLI commands.

### `candle start [names]`

```
$ candle start
$ candle start backend
```

Launch the service(s).

If no `[names]` are provided, then launch all services in the project.

If the service(s) are already running then the existing instances are killed first.

When run interactively, `start` then watches the new process's logs; press Ctrl+C to
stop watching (the process keeps running in the background). When run non-interactively
(agents, scripts, pipes), `start` exits as soon as the launch is confirmed.

Options:

 - `--watch` - Force interactive mode: watch logs after starting.
 - `--bg` - Force non-interactive mode: exit once started.

### `candle check-start [names]`

Like `start` but only starts the service(s) if they are not already running.

### `candle run [names]`

Alias for `candle start`. Both commands do exactly the same thing.

### `candle list` or `candle ls`

```
$ candle ls
```

List the services for this project directory, including active and inactive services.

### `candle watch [names]`

Enter watch mode for the running service(s).

This will interactively print any log messages from the service
as they happen. `watch` never launches processes — use `candle start` for that.

If no `[names]` are provided: Watch every process in the project (this always
succeeds, even for services that haven't launched yet).

If `[names]` are provided: Each named process must currently be running,
otherwise the command fails.

If multiple services are being watched, then each log message will have a prefix that looks like
`[<service name>]`

Example:

    $ candle watch frontend backend
    [backend] Backend server now listening on port 3000
    [frontend] Web server available at http://localhost:8080


### `candle logs [names] [--count <number>] [--start-at <id>]`

Show the recent logs for the given service.

If `[name]` is not provided: Show recent logs across all services in the project directory.

Options:

 - `--count <number>` - Number of log lines to show (default: 100).
 - `--start-at <id>` - Only show logs after this log ID. Useful for pagination.

### `candle kill [names]`

Kill named service(s)

If no `[names]` are provided: Kill all services for this project directory.

### `candle restart [names]`

Restart running service(s) for this current directory.

If no `[names]` are provided: Restart all running services for this project directory

### `candle wait-for-log [name] --message [message]`

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

### `candle setup-project`

Create a new `.candle.json` config file in the current directory.

### `candle add-service <name> --shell <command>`

Add a new service to the nearest `.candle.json` config file.

If the config file doesn't exist yet, it will be created.

### `candle list-ports [names...]`

```
$ candle list-ports
$ candle list-ports backend
$ candle list-ports backend frontend
```

Uses the operating system to detect and list the active open ports for running services.

This queries `lsof` to find TCP ports that are in a LISTEN state, filtering to processes
managed by Candle. It also detects ports opened by child processes of a service.

If no `[names]` are provided: Show ports for all running services in the current project.

### `candle open-browser [name]`

```
$ candle open-browser
$ candle open-browser frontend
```

Open a web browser to `http://localhost:<port>` for a running service.

The port is auto-detected using the same logic as `list-ports`.

#### Disambiguation Logic

If a service has multiple ports open, then `open-browser` will use the lowest port number.

If the command finds multiple running services, then it will give an error.

### `candle mcp` or `candle --mcp`

Run Candle in MCP mode, using stdin as the transport.

# More commands #

Other CLI commands that are not typically used:

### `candle list-all`

List all processes (across the entire system) that were launched by Candle.

The standard `list` command is limited to the current project directory,
but this command covers everything on the system.

### `candle kill-all`

Kill all processes (across the entire system) that were launched by Candle.

Similar to `list` vs `list-all`. The `kill-all` command affects
everything on the system.

### `candle list-ports-all`

Like `list-ports` but shows open ports for all Candle-managed processes across the entire system,
not just the current project directory.

### `candle erase-database`

Delete the database stored in `~/.local/state/candle`.

This command can help if the database is corrupted or it needs a full SQL schema rebuild.

If there are any existing processes then running `erase-database` will leave those processes 'orphaned'
(they will still be running but they won't be tracked by Candle). If you do need to run this command
then run `candle kill-all` first.

# Technical Details #

When running, Candle will create an SQLite database located at `~/.local/state/candle/candle.db`. This database
stores a table of actively running processes, and another table of all the observed log events (from
stdout / stderr and subprocess related events).

# License #

[MIT](./LICENSE)

