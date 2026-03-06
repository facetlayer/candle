
# Repo organization #

 ./src/ - Main source code
 ./test/ - Automated tests
 ./test/sampleServers/ - Sample implementations of test services.
 ./test/workspaces/ - Directories used to run Candle during tests.
 ./test/cli/ - Tests related to each CLI command
 ./docs/ - Internal docs, aimed at tools and developers
 ./docs-site/ - Public documentation website
 ./docs-site/docs/ - Contents for the public documentation site.
 ./docs-site/docs/commands/ - Public documentation for each CLI command.
 ./README.md - Front page documentation that appears on Github

# Documentation

### Updating docs ###

If you change the publically facing behavior of the app, including any
CLI commands, make sure to update the corresponding documentation
inside ./docs-site/

# Development #

## Tricks for running Candle locally

Use `bin/test-candle.ts` to run Candle with custom environment settings for testing:

    bin/test-candle.ts --database-dir /tmp/test-db list
    bin/test-candle.ts --database-dir ./test-workspace start my-service
    bin/test-candle.ts --enable-logs list

Options:
- `--database-dir <path>` - Sets `CANDLE_DATABASE_DIR` to use a custom database folder
- `--enable-logs` - Sets `CANDLE_ENABLE_LOGS=true` to write a `candle.log` file in the current directory

Without these flags, it passes through to Candle normally.

# Testing

We have an extensive test suite in ./test using Vitest.

Many tests follow a pattern of using the TestWorkspace helper, which
creates an isolated project directory and isolated SQLite database that
is specific to that test suite.

## Helpful development tools

These are various CLI tools that you are encouraged to use during development.

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
