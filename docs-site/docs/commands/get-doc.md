# get-doc

Display one of the documentation files built into the Candle binary.

## Syntax

```bash
candle get-doc <name>
```

## Description

The `get-doc` command prints a doc that ships inside the `candle` binary. `<name>` is one of the names shown by [`candle list-docs`](list-docs).

If no doc has that name, the command prints an error and exits with a non-zero status.

## Example

```bash
candle get-doc project-setup
```

## See Also

- [list-docs](list-docs) - List available documentation
- [help](help) - Show help for Candle commands
