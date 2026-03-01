# candle

Candle is a process manager optimized for local development, worktrees, and AI agents.

It's a good fit for locally running services as part of the development process.

## Quick Start ##

| task | command |
| ---- | ------- |
| Install the tool from NPM | `npm i -g @facetlayer/candle` |
| Or run the tool using NPX | `npx @facetlayer/candle ...` |
| Set up a project config | `candle setup-project` |
| Add a service to the config | `candle add-service main --shell <shell command>` |
| Start a service and watch logs | `candle run` |
| Start a service in the background | `candle start` |
| Fetch recent logs | `candle logs` |
| Watch logs as they happen | `candle watch` |
| List services | `candle ls` |

## Features & Design Decisions ##

### Simple interface ###

Compared to other process managers like pm2, Candle is a lot simpler, and it doesn't include
a lot of the complexity that's needed when running a service in the cloud.

Basic commands include:
 * `candle run`
 * `candle logs`
 * `candle restart`
 * `candle watch`
 * `candle ls`

### One instance per service ###

For simplicity, Candle won't launch the same service twice in one project.

If you launch the service with `candle start`/`candle run` and that service is already running, the
existing instance will be killed first.

### Organized by project directory ###

A running Candle process is associated with the project directory that it's launched from.

Most Candle commands like `candle ls` will only show you services for your current directory.
This keeps things simpler for coding agents.

#### Worktrees ####

This approach works extremely well if you use Git worktrees.

Since each worktree uses a different directory, each worktree will also have a seperate
set of Candle services. You can run local services in both worktrees simultaneously.

Note that when doing this, you'll probably need to assign unique network ports to each worktree,
so that your services don't conflict when trying to listen on the same port.

### Optimized for coding agents ###

Candle was built and tuned to be used successfully by coding agents like Claude Code.

#### Non-interactive commands ####

One major decision that helps with coding agents is emphasizing **non-interactive commands**. A
non-interactive command is one that exits immediately, instead of commands that continue to run until killed.

This includes:

 - `candle start` launches a service and then exits immediately (compared to `candle run` which launches and watches logs)
 - `candle logs` can fetch recent log messages (compared to `candle watch` which interactively prints logs as they happen)

Additionally the `candle --help` command detects if it's being used by Claude Code, and if so,
it will hide the interactive commands so the agent is not aware of them.

Coding agents do have the *ability* to run interactive/backgrounded commands, but in our experience,
they are much more successful with non-interactive commands.

#### MCP server ####

Candle does ship with an MCP server integration, although these days, most users just use the Bash CLI.

# Documentation #

### The config file ###

In order to use Candle, your project should have a `.candle.json` file at the project's top level.
A new config can be created with `candle setup-project`.

Candle will look for this file when working with local services. You can run `candle` in a subdirectory
of your project and it will find the nearest `.candle.json` in a parent directory.

### Named services ###

You can add multiple services in your project. With multiple services, each command supports a `[name]` parameter
to pick the target.

`candle run backend` - Run the service named 'backend'

Some commands have a default behavior if no service is named:

`candle run` - Run all services.

Some commands accept multiple services at once:

`candle run backend frontend` - Run the 'backend' and 'frontend' services.

# Commands #

### `candle --help`

List all CLI commands.

### `candle start [...names]`

Launch the service(s).

If no `[...names]` are provided, then launch all services in the project.

If the service(s) are already running then the existing instances are killed first.

### `candle run [...names]`

Launches the service(s) just like `candle start`, and then enters watch mode.
During watch mode, the service logs are printed as they happen.

Watch mode can be exited with control-C.

Note that exiting `candle run` does not kill the process - the process
will keep running in the background until `candle kill` is called.

### `candle list` or `candle ls`

List the services for this project directory, including active and inactive services.

### `candle watch [...names]`

Enter watch mode for the running service(s).

This will interactively print any log messages from the service
as they happen.

If no `[...names]` are provided: Watch all running services.

If multiple services are being watched, then each log message will have a prefix that looks like
`[<service name>]`

Example:

    $ candle watch frontend backend
    [backend] Backend server now listening on port 3000
    [frontend] Web server available at http://localhost:8080


### `candle logs [...names]`

Show the recent logs for the given service. This is non-interactive, it will just print the recent
logs and then exit. (unlike `watch` which will continue to print new logs).

This command works even if the service is not running, it will show the logs that occurred
before the exit.

If `[name]` is not provided: Show logs for all services in the project directory.

### `candle kill [...names]`

Kill named service(s)

If no `[...names]` are provided: Kill all services for this project directory.

### `candle restart [...names]`

Restart running service(s) for this current directory.

If no `[...names]` are provided: Restart all running services for this project directory

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

Create a new `.candle.json` config file in the current directory. If a config file already exists
in the current or a parent directory, it will report its location without making changes.

### `candle add-service <name> --shell <command>`

Add a new service to the `.candle.json` config file. If the file doesn't exist, it will be created.

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

