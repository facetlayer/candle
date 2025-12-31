# Testing Strategy

This document describes the testing approach used in the Candle project.

## Overview

Candle uses CLI subprocess tests that run the CLI as end users would use it, testing the full integration from command-line input to final output.

## Test Workspaces

Each test suite uses a dedicated workspace directory under `test/workspaces/`. These workspaces:

- Contain committed `.candle.json` configuration files
- Use relative paths to reference scripts in `test/sampleServers/`
- Serve as both the working directory (`cwd`) and database directory (`CANDLE_DATABASE_DIR`)

### Directory Structure

```
test/
├── TestWorkspace.ts          # Main test helper class
├── sampleServers/            # Shared test server scripts
│   ├── testProcess.js        # Generic long-running process
│   ├── echoServer.js         # Outputs to stdout/stderr regularly
│   ├── delayedLogger.js      # Multi-stage startup for timing tests
│   └── simpleServer.js       # HTTP server
├── workspaces/               # Test workspace directories
│   ├── functional/           # General functional tests
│   ├── cli-start/            # Start command tests
│   ├── cli-kill/             # Kill command tests
│   ├── invalid-config/       # Error handling tests
│   └── ...                   # Other test-specific workspaces
└── cli/                      # CLI test files
```

### Workspace Configuration

Each workspace has a `.candle.json` file committed to git:

```json
{
  "services": [
    {
      "name": "web",
      "shell": "node ../sampleServers/testProcess.js"
    },
    {
      "name": "echo",
      "shell": "node ../sampleServers/echoServer.js"
    }
  ]
}
```

Note the relative paths `../sampleServers/` - this allows all workspaces to share the same test server scripts.

## TestWorkspace Class

The `TestWorkspace` class manages test isolation:

```typescript
import { TestWorkspace } from '../TestWorkspace';

const workspace = new TestWorkspace('cli-start');
const cli = workspace.createCli();

describe('CLI Start Command', () => {
    afterAll(() => workspace.cleanup());

    it('should start a service', async () => {
        const result = await cli(['start', 'web']);
        expect(result.code).toBe(0);
    });
});
```

### Key Properties

- `workspace.name` - The workspace name
- `workspace.dbDir` - Full path to the workspace directory

### createCli()

Creates a function to run CLI commands:

```typescript
const cli = workspace.createCli();

const result = await cli(['start', 'my-service']);
expect(result.code).toBe(0);
expect(result.stdout).toContain('Started');
```

The CLI function:
- Sets `CANDLE_DATABASE_DIR` to the workspace directory
- Sets `cwd` to the workspace directory by default
- Returns `{ stdout, stderr, code }`

To run a command in a different directory:

```typescript
const result = await cli(['list'], { cwd: '/other/path' });
```

### cleanup()

Runs `kill-all` to stop any running processes. Call this in `afterAll`:

```typescript
afterAll(() => workspace.cleanup());
```

Important: We never delete databases. This prevents orphaned processes that would occur if a database is deleted while processes are still running.

## Sample Servers

The `test/sampleServers/` directory contains reusable test processes:

| Script | Purpose |
|--------|---------|
| testProcess.js | Generic long-running process |
| echoServer.js | Outputs to stdout/stderr regularly |
| delayedLogger.js | Multi-stage startup for timing tests |
| simpleServer.js | HTTP server on port 3000 |

## Test Patterns

### Testing Process Lifecycle

```typescript
it('should show running service in list', async () => {
    await cli(['start', 'echo']);
    await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

    const result = await cli(['list']);
    expect(result.stdout).toContain('echo');
    expect(result.stdout).toContain('RUNNING');
});
```

### Testing Error Conditions

```typescript
it('should error for unknown service', async () => {
    const result = await cli(['start', 'nonexistent']);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain('nonexistent');
});
```

### Snapshot Testing

```typescript
import { normalizeOutput } from './utils';

it('should have consistent help format', async () => {
    const result = await cli(['--help']);
    const normalized = normalizeOutput(result.stdout);
    expect(normalized).toMatchSnapshot();
});
```

## Best Practices

1. **Always use afterAll cleanup** - Call `workspace.cleanup()` to stop processes
2. **Never delete databases** - Just kill processes; deleting databases causes orphaned processes
3. **Wait for process readiness** - Use `wait-for-log` before making assertions
4. **Use relative paths in configs** - Point to `../sampleServers/` for shared scripts
5. **One workspace per test suite** - Each describe block should have its own workspace

## Running Tests

```bash
# Run all tests
pnpm test

# Run specific test file
pnpm test test/cli/help.test.ts

# Run tests in watch mode
pnpm test:watch
```

## Adding a New Test Suite

1. Create a workspace directory: `test/workspaces/my-test/`
2. Add a `.candle.json` with your service configuration
3. Create your test file using `TestWorkspace`:

```typescript
import { TestWorkspace } from '../TestWorkspace';

const workspace = new TestWorkspace('my-test');
const cli = workspace.createCli();

describe('My Test Suite', () => {
    afterAll(() => workspace.cleanup());

    it('should do something', async () => {
        // tests
    });
});
```
