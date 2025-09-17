# Candle Functional Tests

This directory contains functional tests for the Candle CLI tool. These tests verify that Candle works correctly as a subprocess, managing processes and storing data in its database.

## Test Structure

```
test/
├── README.md                  # This file
├── vitest.config.ts          # Vitest configuration
├── testUtils.ts              # Shared test utilities
├── basicWorkflow.test.ts     # Basic run/kill workflow tests (complex version)
├── namedCommands.test.ts     # Named command tests (complex version)
├── functional.test.ts        # Simplified functional tests
├── simple.test.ts            # Basic sanity test
├── sampleServers/            # Sample server scripts for testing
│   ├── simpleServer.js       # Basic HTTP server
│   └── echoServer.js         # Server that outputs to stdout/stderr
└── test-state/               # Test database and state (gitignored)
```

## Running Tests

From the candle root directory:

```bash
# Run all tests
yarn test

# Run tests in watch mode
yarn test:watch

# Run a specific test file
yarn test basicWorkflow.test.ts
```

## Test Environment

The tests use a special environment configuration:
- **CANDLE_DATABASE_DIR**: Set to `./test/test-state/` to isolate test data
- Database is cleaned before each test
- All processes are killed after each test

## Test Utilities

The `testUtils.ts` file provides shared functions:

### `runCandle(options)`
Runs the candle CLI as a subprocess. Options:
- `args`: Command line arguments
- `cwd`: Working directory
- `waitForOutput`: Wait for specific output before resolving
- `expectError`: Whether to expect an error exit code

### `verifyProcessInDatabase(options)`
Verifies a process exists in the database with given criteria:
- `commandName`: Name of the command
- `workingDirectory`: Working directory
- `isRunning`: Whether the process should be running

### `verifyCommandSaved(commandName, workingDirectory)`
Verifies a command was saved in the project configuration.

### `cleanTestState(testStateDir)`
Cleans up the test database and state directory.

### `killProcess(process)`
Gracefully kills a child process.

## Test Scenarios

### Basic Workflow Tests
1. **Error when no command set**: Verifies proper error message when running without a configured command
2. **Launch and kill process**: Tests the full lifecycle of starting and stopping a process
3. **Reuse existing command**: Verifies commands are saved and reused
4. **Handle already running**: Tests behavior when trying to run an already-running process

### Named Commands Tests
1. **Named command management**: Tests creating and running named commands
2. **Multiple simultaneous commands**: Verifies multiple named processes can run together
3. **Restart named commands**: Tests the restart functionality for named processes

## Sample Servers

Two sample servers are provided for testing:

### simpleServer.js
- Basic HTTP server
- Listens on PORT environment variable (default 3000)
- Handles SIGINT/SIGTERM gracefully
- Logs requests to stdout

### echoServer.js
- Outputs periodic messages to stdout
- Occasional stderr output
- Used for testing log capture
- Handles graceful shutdown

## Debugging Tests

If tests fail:
1. Check `test/test-state/candle.db` for database state
2. Look for orphaned processes with `ps aux | grep node`
3. Enable verbose output by modifying test timeout
4. Check that the candle binary was built with `yarn build`

## Current Test Status

The functional tests demonstrate:
- ✅ Basic error handling when no command is configured
- ✅ Setting and running commands
- ✅ Named command support
- ✅ Process logging capabilities
- ⚠️ Database verification (some tests fail due to timing/connection issues)

Note: Some database verification tests may fail due to SQLite connection timing when accessing the database from both the Candle process and the test process simultaneously.

## Adding New Tests

When adding new tests:
1. Create a new test file or add to existing ones
2. Use unique working directories to avoid conflicts
3. Always clean up processes in `afterEach`
4. Use the shared utilities for consistency
5. Add appropriate timeouts for subprocess operations
6. Consider database access timing when verifying state