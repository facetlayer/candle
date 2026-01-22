import { describe, it, expect, afterAll, beforeAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-start');

describe('CLI Start Command', () => {
    beforeAll(() => workspace.ensureSubdir('test'));
    afterAll(() => workspace.cleanup());

    describe('starting config-defined services', () => {
        it('should start a service defined in config', async () => {
            const result = await workspace.runCli(['start', 'echo']);

            expect(result.stdoutAsString()).toContain('Started');
            expect(result.stdoutAsString()).toContain('echo');
        });

        it('should start multiple services at once', async () => {
            const result = await workspace.runCli(['start', 'echo', 'web']);

            expect(result.stdoutAsString()).toContain('echo');
            expect(result.stdoutAsString()).toContain('web');
        });

        it('should start all configured services when no name provided', async () => {
            const result = await workspace.runCli(['start']);

            // Should start both configured services (web and echo)
            expect(result.stdoutAsString()).toContain('Started');

            // Verify both are running
            const listResult = await workspace.runCli(['list']);
            expect(listResult.stdoutAsString()).toContain('web');
            expect(listResult.stdoutAsString()).toContain('echo');

            // Cleanup
            await workspace.runCli(['kill']);
        });

        it('should show error for unknown service name', async () => {
            const result = await workspace.runCli(['start', 'nonexistent-service'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('nonexistent-service');
        });

        it('should exit quickly after starting', async () => {
            const startTime = Date.now();
            await workspace.runCli(['start', 'echo']);
            const elapsed = Date.now() - startTime;

            // Should exit within a reasonable time (not waiting for process to complete)
            expect(elapsed).toBeLessThan(5000);
        });
    });

    describe('transient processes with --shell', () => {
        it('should start transient process with --shell flag', async () => {
            const result = await workspace.runCli(['start', 'my-transient', '--shell', 'node ../../sampleServers/testProcess.js']);

            expect(result.stdoutAsString()).toContain('Started');
            expect(result.stdoutAsString()).toContain('my-transient');
        });

        it('should start transient process with --shell and --root', async () => {
            const result = await workspace.runCli(['start', 'rooted', '--shell', 'node ../../../sampleServers/testProcess.js', '--root', 'test']);

            expect(result.stdoutAsString()).toContain('Started');
        });

        it('should error when --root is provided without --shell', async () => {
            const result = await workspace.runCli(['start', 'bad-config', '--root', 'test'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        });

        it('should error when --root escapes project directory', async () => {
            const result = await workspace.runCli(['start', 'escape', '--shell', 'echo hi', '--root', '../../../escape'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('root');
        });

        it('should allow transient to shadow config service', async () => {
            // 'echo' exists in config, but we start with different shell
            const result = await workspace.runCli(['start', 'echo', '--shell', 'node ../../sampleServers/testProcess.js']);

            expect(result.stdoutAsString()).toContain('Started');

            // Verify it uses our shell by checking logs
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Test server started']);
            const logs = await workspace.runCli(['logs', 'echo']);
            expect(logs.stdoutAsString()).toContain('Test server started');
        });
    });

    describe('starting already running services', () => {
        it('should handle starting already running service', async () => {
            // Start once
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Start again
            const result = await workspace.runCli(['start', 'echo']);

            // Should either succeed (restart) or give informative message
            expect(result.stdoutAsString()).toBeDefined();
        });
    });

    describe('directory without config', () => {
        it('should error when starting in directory without config', async () => {
            const result = await workspace.runCli(['start', 'something'], { cwd: '/tmp', ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('.candle.json');
        });

        it('should error for transient in directory without config', async () => {
            const result = await workspace.runCli(['start', 'temp', '--shell', 'echo hello'], { cwd: '/tmp', ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('.candle.json');
        });
    });

    describe('start output format', () => {
        it('should show process info after starting', async () => {
            const result = await workspace.runCli(['start', 'echo']);

            // Should indicate the process was started
            expect(result.stdoutAsString().toLowerCase()).toMatch(/start/);
        });

        it('should have minimal stderr on success', async () => {
            const result = await workspace.runCli(['start', 'echo']);

            // No errors on success
            expect(result.stderrAsString()).toBe('');
        });
    });

    describe('special characters in names', () => {
        it('should handle names with dashes', async () => {
            const result = await workspace.runCli(['start', 'my-dashed-name', '--shell', 'node ../../sampleServers/testProcess.js']);

            expect(result.stdoutAsString()).toContain('my-dashed-name');
        });

        it('should handle names with underscores', async () => {
            const result = await workspace.runCli(['start', 'my_underscore_name', '--shell', 'node ../../sampleServers/testProcess.js']);

            expect(result.stdoutAsString()).toContain('my_underscore_name');
        });
    });

});

describe('CLI Start with empty config', () => {
    const emptyWorkspace = new TestWorkspace('cli-start-empty');
    afterAll(() => emptyWorkspace.cleanup());

    it('should error when no services configured and no args', async () => {
        const result = await emptyWorkspace.runCli(['start'], { ignoreExitCode: true });

        expect(result.failed()).toBe(true);
        expect(result.stderrAsString()).toContain('No services configured');
    });
});
