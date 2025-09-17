# candle

Candle is a process manager optimized for local development and AI agents.

# Motivation / Features #

### Local development ###

Most process managers like `pm2` are designed to manage services in production,
so they present you with a lot of configuration options to be able to handle
production demands. This comes at the expense of usability.

Candle is instead optimized for easy local development instead of production.

This affects a few design decisions:

 - Candle will only have one instance of a given service at a time.
 - Has convenient commands like `candle wait-for-log` which helps when running a service in a CI job.
 - The configuration and commands are local to the current project directory.
   - For example, `candle ls` only lists processes related to the current directory, not any processes
     for other project directories on the system.

### Optimized for AI agents ###

Candle comes with an MCP integration, so agents like Claude Code can use it
to launch and restart services, and check their logs.
This helps make the agent more productive, compared to having the agent
run the service as a subprocess inside their chat.

Whenever you're using AI agents, you generally want to give them tools that are
as simple and foolproof as possible, so this tool presents a very simple way
to interact with locally running services.

# Installation #

## Installing as a command line tool ##

The NPM package name is: `@facetlayer/candle`.

Running directly with `npx`:

    $ npx @facetlayer/candle

Installing globally with `npm`:

    $ npm i -g @facetlayer/candle

## Installing as an MCP integration ##

Installing the MCP mode will allow coding agents to use the tool.

We've tested the tool primarily using Claude Code.

Adding the MCP to Claude using `npx`:

    $ claude mcp add candle -s user npx -- @facetlayer/candle --mcp

Installing globally and adding the command to Claude:

    $ npm i -g @facetlayer/candle
    $ claude mcp add candle -s user candle -- --mcp

For other agents: Install as a stdin command using either `npx @facetlayer/candle --mcp` or `candle --mcp`,
depending on whether you've installed the tool using `npm -g`.

# Setting Up #

The main setup file is stored in `.candle-setup.json`. This file should be in the root directory of your project.

When you trigger `candle`, it will find the nearest `.candle-setup.json`, which can either be in the current
directory or a parent directory. Triggering `candle` inside a subdirectory in your project will work fine.

Example .candle-setup.json file:

```
{
    "services": [{
        "name": "periodic",
        "shell": "node periodicLogger.js",
        "root": "test/sampleServers"
    }]
}
```

Details for one 'services' entry:

| field     | description      |
| --------- | ---------------- |
| `name`    | The name of the service, this will be used when interacting with the service. |
| `shell`   | The shell command to run when launching the service. |
| `root`    | (Optional) The current working directory to use when launching the service. |

Tip: The MCP integration provides a command `AddServerConfig` which will update or create the .candle-setup.json file.
So, you can tell your coding agent (such as Claude Code) to set up this configuration file.

# MCP Usage #

Details on the MCP integration

General notes:

With coding agents, you will sometimes need to direct the agent to
use this tool. This can be done in your prompts, or in your setup .md files.
For example, telling the agent to "Start the server using Candle" should work.

Here are the MCP tools available:

| name | description |
| -------------- | ------------------------------------------------- |
| AddServerConfig     | Saves a new server configuration to .candle-setup.json |
| StartService   | Start the service for the current directory. |
| GetLogs        | List the recent stdout & stderr logs for the locally running service |
| KillService    | Kill the running service process.    |
| RestartService | Restart the running service process. |
| ListServices   | List the currently running services. |

# CLI Usage #

The `candle` tool can also be used on the command line:

### `candle --help`

List all CLI commands.

### `candle run [name]`

Launch the process and start watching logs.

If `[name]` is not provided: Run the first command listed in .candle-setup.json

Candle does not run the same service more than once. If this command is already running,
then `run` will first perform `restart` to kill the current process and restart it.

Watch mode can be exited with control-C. Note that this does not kill the process, it
will still run in the background until `candle kill` is called on it.

### `candle start [name]`

`start` is similar to `run` but it does not enter watch mode.
The service will be launched in the background and logs will not be printed.

### `candle list` or `candle ls`

List the active processes for this project directory.

### `candle watch [name]`

Enter watch mode for the given running service. This will interactively print any log messages from the service
as they happen.

If `[name]` is not provided: Watch the first command listed in .candle-setup.json

If the service is not currently running then `watch` will show an error.

### `candle logs [name]`

Show the recent logs for the given service. This is non-interactive, it will just print the recent
logs and then exit. (unlike `watch` which will continue to print new logs).

This command works even if the service is not running, it will show the logs that occurred
before the exit.

If `[name]` is not provided: Show logs for the first command listed in .candle-setup.json

### `candle kill [name]`

Kill a process.

If `[name]` is not provided: Kill all the processes for this project directory.

### `candle restart [name]`

Restart the process for this current directory.

If `[name]` is not provided: Restart the first command listed in .candle-setup.json

### `candle wait-for-log [name] --message [message]

Waits until the given process has printed a message to stdout or stderr that includes `[message]` as a substring.

This command is designed for CI jobs, which need to wait until a service has fully started up before
they can use or test that service.

Example usage:

```
    candle start api
    candle wait-for-log api --message "server now listening"
```

These two commands will launch the `api` service in the background, and then
wait until the service has printed a console log that includes `"server now listening"`.

The command will continue to wait until a certain timeout. The timeout defaults to 60 seconds and can be
set on the command line as `--timeout [seconds]`.

#### Handling startup race conditions 

The `wait-for-log` command is safe against race conditions - it will immediately return successfully if the given log
line has already been printed, as long as that log message was for the latest 'run' of the service.

Here are more details explaining what this means:

Lets say that the `candle logs` for a certain process look like this:

```
    > yarn dev
    [info] API server now listening on port 3444
```

If you run `candle wait-for-log api --message "server now listening"` at this point, the command will
immediately return successfully, because the target log message is already there.

But, let's say `candle logs` looks like this:

```
    > yarn dev
    [info] API server now listening on port 3444
    [Process exited with code 1]
    > yarn dev
```

In this situation, `wait-for-log` will not return immediately, instead it will start waiting
for the message show up. This is because the existing "server now listening" log message was for
the previous process run (before the "Process exited" event), so those old logs are ignored.

The short version of all this is: If you follow the above pattern of calling `candle start` and then `candle wait-for-log`,
then it will correctly do what you expect.

### `candle-mcp` or `candle --mcp`

Run Candle in MCP mode, using stdin as the transport.

# Infrequently used commands #

More CLI commands that are not typically used:

### `candle list-all`

List all processes (across the entire system) that were launched by Candle.

The standard `list` command is limited to the current project directory,
but this command covers everything on the system.

### `candle kill-all`

Kill all processes (across the entire system) that were launched by Candle.

Similar to `list` vs `list-all`. The `kill-all` command affects
everything on the system.

### `candle clear-database`

Delete the database stored in `~/.candle`.

This command can help if the database is corrupted or it needs a full SQL schema rebuild.

If there are any existing processes then running `clear-database` will leave those processes 'orphaned'
(you won't be able to interact with them using `candle` after that), and you'll need to manually find and delete
those processes yourself.

# Technical Details

When running, Candle will create an SQLite database located at `~/.candle/candle.db`. This database
stores a table of actively running processes, and another table of all the observed log events (from
stdout / stderr and subprocess related events).

# Debugging

Setting the `CANDLE_ENABLE_LOGS` environment variable to `1` will enable writing to a local `candle.log` file,
this will show logs for all MCP requests and responses.

