
# Repo organization #

 ./src/ - Main source code
 ./test/ - Automated tests
 ./test/sampleServers/ - Sample implementations of test services.
 ./test/workspaces/ - Directories used to run Candle during tests.
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

The Candle command uses an environment variable `CANDLE_DATABASE_DIR` which can
override the database folder (which defaults to ~/.local/state/candle). Setting
this var is a good way to run the command for a local test.

Additionally another environment variable is `CANDLE_ENABLE_LOGS=true`. When enabled,
the tool will write a `candle.log` file in the current directory with extra logs.

## Running the test suite

When running the test suite, prefer to use `candle` itself to manage the
test suite. Use `candle logs test:watch` (or the MCP GetLogs action for 'test:watch')

If the local runner is broken then you may need to run `pnpm test` separately.

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
