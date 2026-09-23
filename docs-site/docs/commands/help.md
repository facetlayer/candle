# help

Display help information for Candle commands.

## Syntax

```bash
candle help [command]
```

## Description

The `help` command displays usage information for Candle. When called without arguments, it shows the main help text with all available commands organized by category: Process Management, Port Detection, Logs, Configuration, Documentation, Troubleshooting & Maintenance, and Other (`help` and `mcp`), followed by the global options `--version`, `--project-dir` and `--json`.

With a command name, it shows that command's help: `candle help start` prints the same text as `candle start --help`.

You can also use `--help` as an alternative to the `help` command.

## Examples

### Show main help

```bash
candle help
```

### Get help for a specific command

```bash
candle help start
# or
candle start --help
```

## See Also

- [mcp](mcp) - Enter MCP server mode
