
# Repo organization #

 Candle is implemented in Rust. The former Node.js/TypeScript implementation has been removed;
 the only remaining TypeScript is the Vitest acceptance suite under ./test.

 ./rust/ - Main source code. One crate, one binary (`candle`); `src/main.rs` is the entry point.
 ./rust/src/cli/ - Help text, argument parsing, and the `--monitor` entry point
 ./rust/src/monitor/ - Monitor mode: the per-service supervision loop run by `candle --monitor`
 ./rust/docs/ - Rust architecture reference
 ./test/ - Automated tests (Vitest, run against the compiled Rust binary)
 ./test/sampleServers/ - Sample implementations of test services.
 ./test/workspaces/ - Directories used to run Candle during tests.
 ./test/cli/ - Tests related to each CLI command
 ./docs/ - Docs built into the binary and shown by `candle list-docs` / `get-doc`
 ./docs/dev/ - Developer docs for working on Candle (not shown by `list-docs`)
 ./docs-site/ - Public documentation website
 ./docs-site/docs/ - Contents for the public documentation site.
 ./docs-site/docs/commands/ - Public documentation for each CLI command.
 ./formal/ - Lean 4 models and proofs of core behavior (log filter, start/restart protocol); `cd formal && lake build`
 ./install-local.sh - Build and install candle locally (for development)
 ./install.sh - Public one-line installer; downloads prebuilt binaries from GitHub Releases
 ./README.md - Front page documentation that appears on Github

# Documentation

Important doc files:

 - ./docs/dev/releasing.md - How to publish a new version
 - ./docs/dev/testing-strategy.md - How to test the apps

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

There is an extensive test suite in ./test using Vitest.

# Commits

Save a git commit after each chunk of work (a finished task, a fix, or a
self-contained step) without waiting to be asked. Stage only the specific files you
changed. Use conventional-commits style.

For user-facing changes, add a ` - ` bullet under `# Unreleased` at the top of
CHANGELOG.md (create the heading if missing). Skip it for docs-only or internal
changes.
