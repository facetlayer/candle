import { describe, it, expect, afterAll, beforeAll } from 'vitest';
import { TestWorkspace } from './TestWorkspace';

const workspace = new TestWorkspace('transient-processes');

beforeAll(() => workspace.ensureSubdir('test'));
afterAll(() => workspace.cleanup());

describe('Basic Operations', () => {
    it('should start a transient process with --shell', async () => {
        const result = await workspace.runCli([
            'start',
            'my-transient',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);

        expect(result.stdoutAsString()).toContain('Started');
        expect(result.stdoutAsString()).toContain('my-transient');

        // Verify it appears in list
        const listResult = await workspace.runCli(['list']);
        expect(listResult.stdoutAsString()).toContain('my-transient');
        expect(listResult.stdoutAsString()).toContain('RUNNING');
    });

    it('should start a transient process with --shell and --root', async () => {
        // Use 'test' subdirectory which exists in the workspace
        // When root is 'test', the shell command runs from within the test subdirectory
        // so we need to go up one more level to reach sampleServers
        const result = await workspace.runCli([
            'start',
            'rooted-transient',
            '--shell',
            'node ../../../sampleServers/testProcess.js',
            '--root',
            'test',
        ]);

        expect(result.stdoutAsString()).toContain('Started');
    });

    it('should show logs for a transient process', async () => {
        await workspace.runCli([
            'start',
            'log-transient',
            '--shell',
            'node ../../sampleServers/echoServer.js',
        ]);

        await workspace.runCli(['wait-for-log', 'log-transient', '--message', 'Echo server started']);

        const logsResult = await workspace.runCli(['logs', 'log-transient']);
        expect(logsResult.stdoutAsString()).toContain('Echo server started');
    });

    it('should kill a transient process', async () => {
        await workspace.runCli([
            'start',
            'kill-me',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);

        await workspace.runCli(['wait-for-log', 'kill-me', '--message', 'Test server started']);

        const killResult = await workspace.runCli(['kill', 'kill-me']);
        expect(killResult.stdoutAsString()).toContain('Killed');

        // Verify it's gone from list
        const listResult = await workspace.runCli(['list']);
        expect(listResult.stdoutAsString()).not.toContain('kill-me');
    });

    it('should restart a transient process with same shell from DB', async () => {
        await workspace.runCli([
            'start',
            'restart-me',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);

        await workspace.runCli(['wait-for-log', 'restart-me', '--message', 'Test server started']);

        const restartResult = await workspace.runCli(['restart', 'restart-me']);
        expect(restartResult.stdoutAsString()).toContain('Started');

        // Verify it's still running
        const listResult = await workspace.runCli(['list']);
        expect(listResult.stdoutAsString()).toContain('restart-me');
        expect(listResult.stdoutAsString()).toContain('RUNNING');
    });
});

describe('Validation', () => {
    it('should error when --root is provided without --shell', async () => {
        const result = await workspace.runCli(['start', 'bad-transient', '--root', 'src'], { ignoreExitCode: true });

        // Without --shell, it will try to find 'bad-transient' in config
        expect(result.failed()).toBe(true);
        expect(result.stderrAsString()).toContain('bad-transient');
    });

    it('should error when starting transient in directory without config file', async () => {
        const result = await workspace.runCli(
            ['start', 'no-config', '--shell', 'echo hello'],
            { cwd: '/tmp', ignoreExitCode: true }
        );

        expect(result.failed()).toBe(true);
        expect(result.stderrAsString()).toContain('.candle.json');
    });

    it('should error when --root escapes project directory', async () => {
        const result = await workspace.runCli([
            'start',
            'escape-transient',
            '--shell',
            'echo hello',
            '--root',
            '../../../escape',
        ], { ignoreExitCode: true });

        expect(result.failed()).toBe(true);
        expect(result.stderrAsString()).toContain('root');
    });
});

describe('Name Collision Behavior', () => {
    it('should allow transient with same name as config service', async () => {
        // 'test' is defined in .candle.json
        // Start it as transient with different shell
        const result = await workspace.runCli([
            'start',
            'test',
            '--shell',
            'node ../../sampleServers/echoServer.js',
        ]);

        expect(result.stdoutAsString()).toContain('Started');

        // Verify it's running with our shell (check logs for echoServer output)
        await workspace.runCli(['wait-for-log', 'test', '--message', 'Echo server started']);
        const logsResult = await workspace.runCli(['logs', 'test']);
        expect(logsResult.stdoutAsString()).toContain('Echo server started');
    });

    it('should kill existing transient when starting new one with same name', async () => {
        // Start first transient
        await workspace.runCli([
            'start',
            'same-name',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);

        await workspace.runCli(['wait-for-log', 'same-name', '--message', 'Test server started']);

        // Start second transient with same name but different shell
        const result = await workspace.runCli([
            'start',
            'same-name',
            '--shell',
            'node ../../sampleServers/echoServer.js',
        ]);

        expect(result.stdoutAsString()).toBeDefined();

        // Verify new one is running
        await workspace.runCli(['wait-for-log', 'same-name', '--message', 'Echo server started']);

        const logsResult = await workspace.runCli(['logs', 'same-name']);
        expect(logsResult.stdoutAsString()).toContain('Echo server started');
    });

    it('should not affect other transient when starting config service', async () => {
        // Start a transient
        await workspace.runCli([
            'start',
            'my-transient',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);
        await workspace.runCli(['wait-for-log', 'my-transient', '--message', 'Test server started']);

        // Start a config service (test uses testProcess.js which outputs "Test server started successfully")
        await workspace.runCli(['start', 'test']);
        await workspace.runCli(['wait-for-log', 'test', '--message', 'Test server started']);

        // Both should be running
        const listResult = await workspace.runCli(['list']);
        expect(listResult.stdoutAsString()).toContain('my-transient');
        expect(listResult.stdoutAsString()).toContain('test');
    });
});

describe('Config Drift Detection', () => {
    it('should not show warning when config matches DB', async () => {
        await workspace.runCli(['start', 'test']);
        await workspace.runCli(['wait-for-log', 'test', '--message', 'Test server started']);

        const listResult = await workspace.runCli(['list']);
        expect(listResult.stdoutAsString()).toContain('test');
        expect(listResult.stdoutAsString()).toContain('RUNNING');
        expect(listResult.stdoutAsString()).not.toContain('[config changed]');
    });

    it('should show warning when transient shadows config service', async () => {
        // Start 'test' as transient with different shell (echoServer instead of testProcess)
        await workspace.runCli([
            'start',
            'test',
            '--shell',
            'node ../../sampleServers/echoServer.js',
        ]);
        await workspace.runCli(['wait-for-log', 'test', '--message', 'Echo server started']);

        const listResult = await workspace.runCli(['list']);
        expect(listResult.stdoutAsString()).toContain('test');
        expect(listResult.stdoutAsString()).toContain('[config changed]');
    });

    it('should not show warning for pure transient (not in config)', async () => {
        await workspace.runCli([
            'start',
            'pure-transient',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);
        await workspace.runCli(['wait-for-log', 'pure-transient', '--message', 'Test server started']);

        const listResult = await workspace.runCli(['list']);
        expect(listResult.stdoutAsString()).toContain('pure-transient');
        // Check that the pure-transient line specifically doesn't have [config changed]
        const pureTransientLine = listResult.stdoutAsString().split('\n').find(line =>
            line.includes('pure-transient')
        );
        expect(pureTransientLine).toBeDefined();
        expect(pureTransientLine).not.toContain('[config changed]');
    });
});

describe('Database Schema', () => {
    it('should store shell in DB for config-based process', async () => {
        await workspace.runCli(['start', 'test']);
        await workspace.runCli(['wait-for-log', 'test', '--message', 'Test server started']);

        // Restart should work even though we're reading from DB
        const restartResult = await workspace.runCli(['restart', 'test']);
        expect(restartResult.stdoutAsString()).toContain('Started');
    });

    it('should store shell in DB for transient process', async () => {
        await workspace.runCli([
            'start',
            'db-transient',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);
        await workspace.runCli(['wait-for-log', 'db-transient', '--message', 'Test server started']);

        // Restart reads from DB
        await workspace.runCli(['restart', 'db-transient']);

        // Verify it still works
        await workspace.runCli(['wait-for-log', 'db-transient', '--message', 'Test server started']);
    });
});

describe('Edge Cases', () => {
    it('should handle shell command with spaces', async () => {
        const result = await workspace.runCli([
            'start',
            'spaced-shell',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);

        expect(result.stdoutAsString()).toContain('Started');
    });

    it('should handle process that exits immediately', async () => {
        const result = await workspace.runCli([
            'start',
            'quick-exit',
            '--shell',
            'echo "quick exit"',
        ]);

        // Process may have already exited, but start should succeed
        expect(result.stdoutAsString()).toBeDefined();
    });

    it('should handle name with dash', async () => {
        const result = await workspace.runCli([
            'start',
            'my-dashed-name',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);

        expect(result.stdoutAsString()).toContain('my-dashed-name');
    });
});
