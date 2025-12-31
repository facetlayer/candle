import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './TestWorkspace';

const workspace = new TestWorkspace('transient-processes');
const cli = workspace.createCli();

afterAll(() => workspace.cleanup());

describe('Basic Operations', () => {
    it('should start a transient process with --shell', async () => {
        const result = await cli([
            'start',
            'my-transient',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);

        expect(result.code).toBe(0);
        expect(result.stdout).toContain('Started');
        expect(result.stdout).toContain('my-transient');

        // Verify it appears in list
        const listResult = await cli(['list']);
        expect(listResult.stdout).toContain('my-transient');
        expect(listResult.stdout).toContain('RUNNING');
    });

    it('should start a transient process with --shell and --root', async () => {
        // Use 'test' subdirectory which exists in sampleServers
        const result = await cli([
            'start',
            'rooted-transient',
            '--shell',
            'node ../../sampleServers/testProcess.js',
            '--root',
            'test',
        ]);

        expect(result.code).toBe(0);
        expect(result.stdout).toContain('Started');
    });

    it('should show logs for a transient process', async () => {
        const startResult = await cli([
            'start',
            'log-transient',
            '--shell',
            'node ../../sampleServers/echoServer.js',
        ]);
        expect(startResult.code).toBe(0);

        const waitResult = await cli(['wait-for-log', 'log-transient', '--message', 'Echo server started']);
        expect(waitResult.code).toBe(0);

        const logsResult = await cli(['logs', 'log-transient']);
        expect(logsResult.stdout).toContain('Echo server started');
    });

    it('should kill a transient process', async () => {
        const startResult = await cli([
            'start',
            'kill-me',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);
        expect(startResult.code).toBe(0);

        const waitResult = await cli(['wait-for-log', 'kill-me', '--message', 'Test server started']);
        expect(waitResult.code).toBe(0);

        const killResult = await cli(['kill', 'kill-me']);
        expect(killResult.code).toBe(0);
        expect(killResult.stdout).toContain('Killed');

        // Verify it's gone from list
        const listResult = await cli(['list']);
        expect(listResult.stdout).not.toContain('kill-me');
    });

    it('should restart a transient process with same shell from DB', async () => {
        const startResult = await cli([
            'start',
            'restart-me',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);
        expect(startResult.code).toBe(0);

        const waitResult = await cli(['wait-for-log', 'restart-me', '--message', 'Test server started']);
        expect(waitResult.code).toBe(0);

        const restartResult = await cli(['restart', 'restart-me']);
        expect(restartResult.code).toBe(0);
        expect(restartResult.stdout).toContain('Started');

        // Verify it's still running
        const listResult = await cli(['list']);
        expect(listResult.stdout).toContain('restart-me');
        expect(listResult.stdout).toContain('RUNNING');
    });
});

describe('Validation', () => {
    it('should error when --root is provided without --shell', async () => {
        const result = await cli(['start', 'bad-transient', '--root', 'src']);

        // Without --shell, it will try to find 'bad-transient' in config
        expect(result.code).not.toBe(0);
        expect(result.stderr).toContain('bad-transient');
    });

    it('should error when starting transient in directory without config file', async () => {
        const result = await cli(
            ['start', 'no-config', '--shell', 'echo hello'],
            { cwd: '/tmp' }
        );

        expect(result.code).not.toBe(0);
        expect(result.stderr).toContain('.candle.json');
    });

    it('should error when --root escapes project directory', async () => {
        const result = await cli([
            'start',
            'escape-transient',
            '--shell',
            'echo hello',
            '--root',
            '../../../escape',
        ]);

        expect(result.code).not.toBe(0);
        expect(result.stderr).toContain('root');
    });
});

describe('Name Collision Behavior', () => {
    it('should allow transient with same name as config service', async () => {
        // 'test' is defined in .candle.json
        // Start it as transient with different shell
        const result = await cli([
            'start',
            'test',
            '--shell',
            'node ../../sampleServers/echoServer.js',
        ]);

        expect(result.code).toBe(0);
        expect(result.stdout).toContain('Started');

        // Verify it's running with our shell (check logs for echoServer output)
        await cli(['wait-for-log', 'test', '--message', 'Echo server started']);
        const logsResult = await cli(['logs', 'test']);
        expect(logsResult.stdout).toContain('Echo server started');
    });

    it('should kill existing transient when starting new one with same name', async () => {
        // Start first transient
        const startResult1 = await cli([
            'start',
            'same-name',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);
        expect(startResult1.code).toBe(0);

        const waitResult1 = await cli(['wait-for-log', 'same-name', '--message', 'Test server started']);
        expect(waitResult1.code).toBe(0);

        // Start second transient with same name but different shell
        const result = await cli([
            'start',
            'same-name',
            '--shell',
            'node ../../sampleServers/echoServer.js',
        ]);

        expect(result.code).toBe(0);

        // Verify new one is running
        const waitResult2 = await cli(['wait-for-log', 'same-name', '--message', 'Echo server started']);
        expect(waitResult2.code).toBe(0);

        const logsResult = await cli(['logs', 'same-name']);
        expect(logsResult.stdout).toContain('Echo server started');
    });

    it('should not affect other transient when starting config service', async () => {
        // Start a transient
        await cli([
            'start',
            'my-transient',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);
        await cli(['wait-for-log', 'my-transient', '--message', 'Test server started']);

        // Start a config service
        await cli(['start', 'test']);
        await cli(['wait-for-log', 'test', '--message', 'Echo server started']);

        // Both should be running
        const listResult = await cli(['list']);
        expect(listResult.stdout).toContain('my-transient');
        expect(listResult.stdout).toContain('test');
    });
});

describe('Config Drift Detection', () => {
    it('should not show warning when config matches DB', async () => {
        await cli(['start', 'test']);
        await cli(['wait-for-log', 'test', '--message', 'Echo server started']);

        const listResult = await cli(['list']);
        expect(listResult.stdout).toContain('test');
        expect(listResult.stdout).toContain('RUNNING');
        expect(listResult.stdout).not.toContain('[config changed]');
    });

    it('should show warning when transient shadows config service', async () => {
        // Start 'test' as transient with different shell
        await cli([
            'start',
            'test',
            '--shell',
            'node ../sampleServers/testProcess.js',
        ]);
        await cli(['wait-for-log', 'test', '--message', 'Test server started']);

        const listResult = await cli(['list']);
        expect(listResult.stdout).toContain('test');
        expect(listResult.stdout).toContain('[config changed]');
    });

    it('should not show warning for pure transient (not in config)', async () => {
        await cli([
            'start',
            'pure-transient',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);
        await cli(['wait-for-log', 'pure-transient', '--message', 'Test server started']);

        const listResult = await cli(['list']);
        expect(listResult.stdout).toContain('pure-transient');
        expect(listResult.stdout).not.toContain('[config changed]');
    });
});

describe('Database Schema', () => {
    it('should store shell in DB for config-based process', async () => {
        await cli(['start', 'test']);
        await cli(['wait-for-log', 'test', '--message', 'Echo server started']);

        // Restart should work even though we're reading from DB
        const restartResult = await cli(['restart', 'test']);
        expect(restartResult.code).toBe(0);
        expect(restartResult.stdout).toContain('Started');
    });

    it('should store shell in DB for transient process', async () => {
        await cli([
            'start',
            'db-transient',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);
        await cli(['wait-for-log', 'db-transient', '--message', 'Test server started']);

        // Restart reads from DB
        const restartResult = await cli(['restart', 'db-transient']);
        expect(restartResult.code).toBe(0);

        // Verify it still works
        await cli(['wait-for-log', 'db-transient', '--message', 'Test server started']);
    });
});

describe('Edge Cases', () => {
    it('should handle shell command with spaces', async () => {
        const result = await cli([
            'start',
            'spaced-shell',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);

        expect(result.code).toBe(0);
        expect(result.stdout).toContain('Started');
    });

    it('should handle process that exits immediately', async () => {
        const result = await cli([
            'start',
            'quick-exit',
            '--shell',
            'echo "quick exit"',
        ]);

        // Process may have already exited, but start should succeed
        expect(result.code).toBe(0);
    });

    it('should handle name with dash', async () => {
        const result = await cli([
            'start',
            'my-dashed-name',
            '--shell',
            'node ../../sampleServers/testProcess.js',
        ]);

        expect(result.code).toBe(0);
        expect(result.stdout).toContain('my-dashed-name');
    });
});
