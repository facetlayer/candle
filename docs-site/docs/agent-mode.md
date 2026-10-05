# Interactive and Agent Mode

Some Candle commands behave differently depending on who is running them. A person at a terminal usually wants to see a service's output right away. A script or a coding agent needs the command to print something and exit.

Candle picks one of two modes each time it runs:

- **Interactive mode** - a person is at a terminal.
- **Non-interactive mode** - the output is going to a script, a pipe or a coding agent.

## How the mode is chosen

Candle uses interactive mode only when both of these are true:

- Standard output is a terminal. Piping or redirecting the output (`candle start | tee log.txt`) makes the run non-interactive.
- No coding agent is detected.

Candle detects a coding agent by checking these environment variables:

| Variable | Set by |
|----------|--------|
| `CLAUDECODE` | Claude Code |
| `GEMINI_CLI` | Gemini CLI |
| `CURSOR_AGENT` | Cursor |

A variable counts only when it is set to a non-empty value. When one is detected, Candle is in **agent mode**: it is non-interactive even if standard output is a terminal.

## What changes

| Command | Interactive mode | Non-interactive mode |
|---------|------------------|----------------------|
| [start](commands/start), [restart](commands/restart) | Launches the service, then shows its output until you press `Ctrl-C`. The service keeps running. | Exits as soon as the launch is confirmed, and prints a hint to use `candle logs`. |

Agent mode changes two more things:

| Command | Agent mode |
|---------|------------|
| [watch](commands/watch) | Exits with an error that points at `candle logs`, because `watch` blocks until `Ctrl-C`. |
| [help](commands/help) | Leaves `watch` out of the command list. |

A script or a pipe can still run `watch`; only a detected agent is refused.

## Choosing the mode yourself

`start` and `restart` take two options that override the detection:

- `--watch` - watch the logs after launching, as in interactive mode.
- `--bg` - exit once the service is launched, as in non-interactive mode.

```bash
candle start api --bg
```

## Using Candle from a coding agent

The recommended way for a coding agent to use Candle is the command line, the same way you do. No setup is needed beyond installing Candle, because the CLI detects the agent and never blocks.

Tell your agent that the project uses Candle and that it should run this to learn the commands:

```bash
candle --help
```

For example, in the instructions file your agent reads (such as `CLAUDE.md` or `AGENTS.md`):

```
This project runs its dev servers with Candle. Run `candle --help` to see the commands.
```

Candle also has an [MCP server](mcp-integration), for clients that can't run shell commands.

## See Also

- [start](commands/start) - Start services
- [watch](commands/watch) - Watch live output
- [logs](commands/logs) - View recent logs, which works in every mode
