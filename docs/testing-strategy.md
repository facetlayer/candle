# Testing Strategy

This document describes the testing approach used in the Candle project.

## Overview

Candle uses a comprehensive testing strategy with multiple layers:

1. **CLI Subprocess Tests** - Test the CLI as end users would use it
2. **Unit Tests** - Test individual components in isolation
3. **MCP Integration Tests** - Test the MCP server functionality

## CLI Subprocess Testing

The CLI tests in `test/cli/` run the Candle CLI as a subprocess and make assertions on its output. This approach tests the full integration from command-line input to final output.

### Test Structure

```
test/cli/
├── utils.ts              # Shared utilities for CLI testing
├── fixtures/             # Test fixtures with sample configs
│   ├── basic/           # Single service configuration
│   ├── empty/           # Empty services array
│   ├── multi-service/   # Multiple services
│   └── with-root/       # Services with root directories
├── help.test.ts         # --help flag tests
├── version.test.ts      # --version flag tests
├── list.test.ts         # list/ls command tests
├── list-all.test.ts     # list-all command tests
├── start.test.ts        # start command tests
├── kill.test.ts         # kill/stop command tests
├── kill-all.test.ts     # kill-all command tests
├── restart.test.ts      # restart command tests
├── logs.test.ts         # logs command tests
├── clear-logs.test.ts   # clear-logs command tests
├── add-service.test.ts  # add-service command tests
├── list-docs.test.ts    # list-docs command tests
├── get-doc.test.ts      # get-doc command tests
├── erase-database.test.ts  # erase-database command tests
├── wait-for-log.test.ts    # wait-for-log command tests
└── errors.test.ts       # Error handling tests
```

### Running CLI as Subprocess

Tests use the `createCli()` utility to create a function that runs the CLI:

```typescript
import { createCli, ensureCleanDbDir, getSampleServersDir } from './utils';

const dbDir = ensureCleanDbDir('my-test');
const cli = createCli(dbDir, getSampleServersDir());

// Run a command
const result = await cli(['start', 'my-service']);

// Check results
expect(result.code).toBe(0);
expect(result.stdout).toContain('Started');
expect(result.stderr).toBe('');
```

### Database Isolation

Each test suite uses an isolated database directory to prevent interference:

```typescript
const dbDir = ensureCleanDbDir('unique-test-name');
const cli = createCli(dbDir, workingDirectory);
```

The `CANDLE_DATABASE_DIR` environment variable is set for each test.

### Test Fixtures

Fixtures provide different configurations for testing:

- **basic/** - Single service for simple tests
- **empty/** - Empty config to test edge cases
- **multi-service/** - Multiple services for concurrent process tests
- **with-root/** - Services with root directories for path handling tests

Each fixture has a `.candle.json` file and any necessary scripts.

### Pattern: Testing Quick-Exit Commands

For commands that exit quickly (non-blocking):

```typescript
it('should start and exit quickly', async () => {
    const startTime = Date.now();
    const result = await cli(['start', 'my-service']);
    const elapsed = Date.now() - startTime;

    expect(result.code).toBe(0);
    expect(elapsed).toBeLessThan(5000);
});
```

### Pattern: Testing with Process Lifecycle

For tests that need a running process:

```typescript
it('should show running service in list', async () => {
    // Start service
    await cli(['start', 'echo']);

    // Wait for it to be ready
    await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

    // Make assertions
    const result = await cli(['list']);
    expect(result.stdout).toContain('echo');
    expect(result.stdout).toContain('RUNNING');
});

afterEach(async () => {
    // Clean up
    await cli(['kill-all']).catch(() => {});
});
```

### Pattern: Testing Error Conditions

```typescript
it('should error for unknown service', async () => {
    const result = await cli(['start', 'nonexistent']);

    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain('nonexistent');
});
```

### Pattern: Snapshot Testing

For output format testing, normalize and snapshot:

```typescript
import { normalizeOutput } from './utils';

it('should have consistent help format', async () => {
    const result = await cli(['--help']);
    const normalized = normalizeOutput(result.stdout);

    expect(normalized).toMatchSnapshot();
});
```

The `normalizeOutput()` function removes dynamic content like:
- Timestamps and uptimes
- PIDs
- Absolute paths
- Temp directories

## Test Utilities

### `createCli(dbDir, workingDir)`

Creates a function to run CLI commands with isolated database.

### `ensureCleanDbDir(testName)`

Creates a clean database directory for a test.

### `getFixtureDir(name)`

Gets path to a test fixture directory.

### `getSampleServersDir()`

Gets path to the shared sample servers directory.

### `normalizeOutput(output)`

Normalizes output for snapshot testing by removing dynamic values.

### `createTempFixture(name, config)`

Creates a temporary fixture with custom configuration.

## Sample Servers

The `test/sampleServers/` directory contains reusable test processes:

| Server | File | Purpose |
|--------|------|---------|
| web | simpleServer.js | HTTP server on port 3000 |
| echo | echoServer.js | Outputs to stdout/stderr regularly |
| delayed-logger | delayedLogger.js | Multi-stage startup for timing tests |
| test-format | testProcess.js | Generic long-running process |

## Best Practices

1. **Always clean up** - Use `afterEach` to kill all processes
2. **Use isolated databases** - Each test suite should have its own database
3. **Wait for process readiness** - Use `wait-for-log` before making assertions
4. **Test both success and failure** - Include error case tests
5. **Keep tests fast** - Most tests should complete in under 5 seconds
6. **Use descriptive test names** - Names should describe what is being tested

## Running Tests

```bash
# Run all tests
pnpm test

# Run specific test file
pnpm test test/cli/help.test.ts

# Run tests in watch mode
pnpm test:watch

# Run with verbose output
pnpm test -- --reporter=verbose
```

## Test Timeouts

The default test timeout is 30 seconds (configured in `vitest.config.ts`). For longer tests, specify a custom timeout:

```typescript
it('should wait for slow startup', async () => {
    // test code
}, 60000); // 60 second timeout
```

## Debugging Tests

1. **Check process list** - Run `candle list-all` to see what's running
2. **Check logs** - Run `candle logs <name>` to see process output
3. **Isolate test** - Run single test file with `.only`
4. **Add delays** - Temporarily add `await new Promise(r => setTimeout(r, 5000))` to inspect state
5. **Check database** - Look in the test temp directory for database state
