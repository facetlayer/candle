# Testing Strategy

This document describes the testing approach used in the Candle project.

## Overview

Candle uses CLI subprocess tests that run the CLI as end users would use it, testing the full integration from command-line input to final output.

## Test Workspaces

Each test suite uses a dedicated workspace directory under `test/workspaces/`. These workspaces:

- Contain committed `.candle.json` configuration files
- Use relative paths (`../../sampleServers/`) to reference scripts in `test/sampleServers/`
- Serve as both the working directory (`cwd`) and database directory (`CANDLE_DATABASE_DIR`)

### Directory Structure

```
test/
├── TestWorkspace.ts          # Main test helper class
├── sampleServers/            # Shared test server scripts
│   ├── testProcess.js        # Generic long-running process
│   ├── echoServer.js         # Outputs to stdout/stderr regularly
│   ├── delayedLogger.js      # Multi-stage startup for timing tests
│   ├── simpleServer.js       # HTTP server
│   └── ...                   # Other sample servers
├── workspaces/               # Test workspace directories
│   ├── functional/           # General functional tests
│   ├── cli-start/            # Start command tests
│   ├── cli-kill/             # Kill command tests
│   ├── invalid-config/       # Error handling tests
│   └── ...                   # Other test-specific workspaces
└── cli/                      # CLI test files (import TestWorkspace from ./utils)
```

### Workspace Configuration

Each workspace has a `.candle.json` file committed to git:

```json
{
  "services": [
    {
      "name": "web",
      "shell": "node ../../sampleServers/testProcess.js"
    },
    {
      "name": "echo",
      "shell": "node ../../sampleServers/echoServer.js"
    }
  ]
}
```

Note the relative paths `../../sampleServers/` - this allows all workspaces to share the same test server scripts.

## TestWorkspace Class

The `TestWorkspace` class manages test isolation:

```typescript
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-start');

describe('CLI Start Command', () => {
    afterAll(() => workspace.cleanup());

    it('should start a service', async () => {
        const result = await workspace.runCli(['start', 'web']);
        expect(result.stdoutAsString()).toContain('Started');
    });
});
```

### Key Properties

- `workspace.name` - The workspace name
- `workspace.dbDir` - Full path to the workspace directory

### runCli()

Runs a CLI command against the compiled binary at `rust/target/release/candle`:

```typescript
const result = await workspace.runCli(['start', 'my-service']);
expect(result.stdoutAsString()).toContain('Started');
```

`runCli`:
- Sets `CANDLE_DATABASE_DIR` to the workspace directory
- Sets `cwd` to the workspace directory by default
- Blanks the coding-agent variables (`CLAUDECODE`, `GEMINI_CLI`, `CURSOR_AGENT`) so tests always run in non-agent mode
- Returns a `SubprocessResult` (`stdoutAsString()`, `stderrAsString()`, `failed()`, ...)
- Throws if the command exits non-zero, unless `ignoreExitCode: true` is passed

To run a command in a different directory:

```typescript
const result = await workspace.runCli(['list'], { cwd: '/other/path' });
```

For MCP tests, `workspace.createMcpApp()` starts `candle --mcp` in the workspace.

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
    await workspace.runCli(['start', 'echo']);
    await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

    const result = await workspace.runCli(['list']);
    expect(result.stdoutAsString()).toContain('echo');
    expect(result.stdoutAsString()).toContain('RUNNING');
});
```

### Testing Error Conditions

```typescript
it('should error for unknown service', async () => {
    const result = await workspace.runCli(['start', 'nonexistent'], { ignoreExitCode: true });
    expect(result.failed()).toBe(true);
    expect(result.stderrAsString()).toContain('nonexistent');
});
```

### Snapshot Testing

```typescript
import { normalizeOutput } from './utils';

it('should have consistent help format', async () => {
    const result = await workspace.runCli(['--help']);
    const normalized = normalizeOutput(result.stdoutAsString());
    expect(normalized).toMatchSnapshot();
});
```

## Best Practices

1. **Always use afterAll cleanup** - Call `workspace.cleanup()` to stop processes
2. **Never delete databases** - Just kill processes; deleting databases causes orphaned processes
3. **Wait for process readiness** - Use `wait-for-log` before making assertions
4. **Use relative paths in configs** - Point to `../../sampleServers/` for shared scripts
5. **One workspace per test suite** - Each describe block should have its own workspace

## Running Tests

```bash
# Run all tests (builds the release binary first)
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
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('my-test');

describe('My Test Suite', () => {
    afterAll(() => workspace.cleanup());

    it('should do something', async () => {
        // tests
    });
});
```
