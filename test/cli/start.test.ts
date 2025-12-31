import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-start');
const cli = workspace.createCli();

describe('CLI Start Command', () => {
    afterAll(() => workspace.cleanup());

    describe('starting config-defined services', () => {
        it('should start a service defined in config', async () => {
            const result = await cli(['start', 'echo']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Started');
            expect(result.stdout).toContain('echo');
        });

        it('should start multiple services at once', async () => {
            const result = await cli(['start', 'echo', 'web']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('echo');
            expect(result.stdout).toContain('web');
        });

        it('should start default service when no name provided', async () => {
            const result = await cli(['start']);

            // Should start first service (web) or error if no default
            expect(result.code).toBe(0);
        });

        it('should show error for unknown service name', async () => {
            const result = await cli(['start', 'nonexistent-service']);

            expect(result.code).not.toBe(0);
            expect(result.stderr).toContain('nonexistent-service');
        });

        it('should exit quickly after starting', async () => {
            const startTime = Date.now();
            const result = await cli(['start', 'echo']);
            const elapsed = Date.now() - startTime;

            expect(result.code).toBe(0);
            // Should exit within a reasonable time (not waiting for process to complete)
            expect(elapsed).toBeLessThan(5000);
        });
    });

    describe('transient processes with --shell', () => {
        it('should start transient process with --shell flag', async () => {
            const result = await cli(['start', 'my-transient', '--shell', 'node testProcess.js']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Started');
            expect(result.stdout).toContain('my-transient');
        });

        it('should start transient process with --shell and --root', async () => {
            const result = await cli(['start', 'rooted', '--shell', 'node ../testProcess.js', '--root', 'test']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Started');
        });

        it('should error when --root is provided without --shell', async () => {
            const result = await cli(['start', 'bad-config', '--root', 'test']);

            expect(result.code).not.toBe(0);
        });

        it('should error when --root escapes project directory', async () => {
            const result = await cli(['start', 'escape', '--shell', 'echo hi', '--root', '../../../escape']);

            expect(result.code).not.toBe(0);
            expect(result.stderr).toContain('root');
        });

        it('should allow transient to shadow config service', async () => {
            // 'echo' exists in config, but we start with different shell
            const result = await cli(['start', 'echo', '--shell', 'node testProcess.js']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Started');

            // Verify it uses our shell by checking logs
            await cli(['wait-for-log', 'echo', '--message', 'Test server started']);
            const logs = await cli(['logs', 'echo']);
            expect(logs.stdout).toContain('Test server started');
        });
    });

    describe('starting already running services', () => {
        it('should handle starting already running service', async () => {
            // Start once
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Start again
            const result = await cli(['start', 'echo']);

            // Should either succeed (restart) or give informative message
            expect(result.code).toBe(0);
        });
    });

    describe('directory without config', () => {
        it('should error when starting in directory without config', async () => {
            const result = await cli(['start', 'something'], { cwd: '/tmp' });

            expect(result.code).not.toBe(0);
            expect(result.stderr).toContain('.candle.json');
        });

        it('should error for transient in directory without config', async () => {
            const result = await cli(['start', 'temp', '--shell', 'echo hello'], { cwd: '/tmp' });

            expect(result.code).not.toBe(0);
            expect(result.stderr).toContain('.candle.json');
        });
    });

    describe('start output format', () => {
        it('should show process info after starting', async () => {
            const result = await cli(['start', 'echo']);

            expect(result.code).toBe(0);
            // Should indicate the process was started
            expect(result.stdout.toLowerCase()).toMatch(/start/);
        });

        it('should have minimal stderr on success', async () => {
            const result = await cli(['start', 'echo']);

            expect(result.code).toBe(0);
            // No errors on success
            expect(result.stderr).toBe('');
        });
    });

    describe('special characters in names', () => {
        it('should handle names with dashes', async () => {
            const result = await cli(['start', 'my-dashed-name', '--shell', 'node testProcess.js']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('my-dashed-name');
        });

        it('should handle names with underscores', async () => {
            const result = await cli(['start', 'my_underscore_name', '--shell', 'node testProcess.js']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('my_underscore_name');
        });
    });

});
