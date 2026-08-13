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

Initialize a `.candle.json` file in the root directory of your project (usually the same
place that has the `.git` directory):

    candle setup-project

Add a service:

    candle add-service <service name> --shell <shell command>

Launch it:

    candle start                # all services
    candle start <service name> # one services

# All Commands #

### `candle --help`

List all CLI commands.

## Main usage commands ##

### `candle start [names]`

    candle start
    candle start backend

Launch the service(s).

If no `[names]` are provided: then launch all services in the project.

If the service(s) are already running then the existing instances are killed first.

If called in interactive mode (see "interactive mode detection" below), `start` will
then enter watch mode, where it watches and prints the new process's logs.

Options:

 - `--watch` - Force interactive mode: watch logs after starting.
 - `--bg` - Force non-interactive mode: exit once started.

### `candle check-start [names]`

Like `start` but only starts the service(s) if they are not already running.

### `candle run [names]`

Alias for `candle start`, does the same thing as `start`.

### `candle list` or `candle ls`

```
$ candle ls
```

List the services for this project directory, including active and inactive services.

### `candle watch [names]`

Enter watch mode for the running service(s).

This will interactively print any log messages from the service as they happen.

If no `[names]` are provided: Watch every process in the project (including
any processes that are launched after `watch` is started)

If multiple services are being watched, then the output lines will include prefixes
that looks like `[<service name>] ...`

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

The pattern of calling `start` then `wait-for-log` will do what you expect: it will wait
for the most recent process instance to print the log message, and won't be triggered if a
previous recent run has the same message.

## Port detection commands ##

### `candle list-ports [names...]`

```
$ candle list-ports
$ candle list-ports backend
$ candle list-ports backend frontend
```

Uses the operating system to detect and list the active open ports for running services.

This command searches Candle managed processes and also child processes. It uses `lsof`
to find to find TCP ports that are in a LISTEN state

If no `[names]` are provided: Show ports for all running services in the current project.

### `candle open-browser [name]`

```
$ candle open-browser
$ candle open-browser frontend
```

Attempts to detect the listening port for a target service, then opens a web
browser to `http://localhost:<port>` for that service.

The port is auto-detected using the same logic as `list-ports`.

The `open-browser` command isn't perfect, and it can be confused by certain
situations (such as if your service has multiple listening ports). But in
most simple cases it works pretty well.

## Project setup commands ##

### `candle setup-project`

Create a new `.candle.json` config file in the current directory.

### `candle add-service <name> --shell <command>`

Add a new service to the nearest `.candle.json` config file.

If the config file doesn't exist yet, it will be created.

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

### `candle list-ports-all`

Like `list-ports` but shows open ports for all Candle-managed processes across the entire system.

### `candle erase-database`

Delete the database stored in `~/.local/state/candle`.

This command can help if the database is corrupted or it needs a full SQL schema rebuild.

Warning: If there are any existing processes, then running `erase-database` will leave those processes 'orphaned'
(they will still be running but they won't be tracked by Candle). It's recommended to run `candle kill-all`
before doing this.

# Interactive mode detection #

Several Candle commands have different behavior depending if they are running in an interactive
or non-interactive context.

Candle uses **interactive mode** only when:

 - Stdout is a TTY. Piping or redirecting output (`candle start | tee log.txt`) makes it non-interactive.
 - And, no coding-agent environment variables are detected. If any are, Candle assumes an agent is
   driving the CLI. This is "agent mode".

The agent markers Candle looks for, each set by the agent itself on the commands it runs:

| Variable | Set by |
|---|---|
| `CLAUDECODE` | Claude Code |
| `GEMINI_CLI` | Gemini CLI |
| `CURSOR_AGENT` | Cursor |

Any non-empty value counts, so `CLAUDECODE=false` still means agent mode.

Codex is intentionally not on this list. Its `CODEX_SANDBOX` variable means "a sandbox is active",
not "Codex is driving" — it is unset under `--sandbox danger-full-access`, so keying on it would
silently miss anyone who turns the sandbox off. Codex still gets non-interactive behavior via the
TTY check, which is what matters for `start` and `restart`.

What changes:

 - `start`, `run`, and `restart` stay attached and watch the new process's logs when interactive;
   when non-interactive they exit as soon as the launch is confirmed and print a hint pointing at
   `candle logs`. Pass `--watch` or `--bg` to force one or the other.
 - `check-start` always exits immediately, so scripts get the same behavior either way.
 - `watch` blocks forever by design, so in agent mode it is hidden from `candle --help` and exits
   with an error suggesting `candle logs` instead. Note this keys on the agent variables only, not
   on the TTY check — `watch` still works when you pipe its output.

# License #

[MIT](./LICENSE)

