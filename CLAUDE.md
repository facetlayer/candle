
# Development #

When running the test suite, prefer to use `candle` itself to manage the
test suite. Use `candle logs test:watch` (or the MCP GetLogs action for 'test:watch')

If the local runner is broken then you may need to run `pnpm test` separately.

## Helpful development tools

### docs

Generic CLI tool to fetch documentation for a target NPM library. This will
search local node_modules first and will search NPM if necessary.

Syntax:

    docs list <library-name>          # List the documentation files for <library-name>
    docs show <library-name> <file>   # Show a documentation file

Example:

    docs list yargs
    docs show yargs README.md

### npm-status

Compares the library in NPM versus local, to see if there are changes that are
not yet published to NPM.

Example:

    npm-status status
