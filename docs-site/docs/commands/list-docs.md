# list-docs

List the documentation built into the Candle binary.

## Syntax

```bash
candle list-docs [--json]
```

## Description

The `list-docs` command prints the name and a one-line description of each doc that ships inside the `candle` binary. These docs are available offline and always match the installed version. Show one with [`candle get-doc <name>`](get-doc).

## Options

- `--json` - Output as JSON: an array of objects, each with a `name` and a `description`.

## Example

```bash
candle list-docs
```

Example output:

```
Available docs (show one with 'candle get-doc <name>'):

  agents-intro         Short introduction to the Candle tool for AI agents
  mcp-usage            Run Candle as an MCP server for AI agents
  project-setup        Set up a project to use Candle, including the .candle.json config file
  transient-processes  Start one-off processes with --shell without adding them to .candle.json
  README               Full reference for every command (the project README)
```

## See Also

- [get-doc](get-doc) - Display a documentation file
- [help](help) - Show help for Candle commands
