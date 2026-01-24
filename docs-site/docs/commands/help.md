# help

Display help information for Candle commands.

## Syntax

```bash
candle help [topic]
```

## Description

The `help` command displays usage information for Candle. When called without arguments, it shows the main help text with all available commands organized by category.

You can also use `--help` as an alternative to the `help` command.

## Arguments

- `topic` - Optional help topic for more detailed information on specific areas.

## Available Topics

- `port-reservation` - Commands for reserving ports for services

## Examples

### Show main help

```bash
candle help
```

### Show port reservation commands

```bash
candle help port-reservation
```

### Get help for a specific command

```bash
candle start --help
```

## See Also

- [mcp](mcp) - Enter MCP server mode
