import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-restart');
const cli = workspace.createCli();

describe('CLI Restart Command', () => {
    afterAll(() => workspace.cleanup());

    describe('basic restart functionality', () => {
        it('should restart a running service', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['restart', 'echo']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Started');
        });

        it('should exit quickly after restart', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const startTime = Date.now();
            const result = await cli(['restart', 'echo']);
            const elapsed = Date.now() - startTime;

            expect(result.code).toBe(0);
            expect(elapsed).toBeLessThan(5000);
        });

        it('should keep service running after restart', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            await cli(['restart', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const list = await cli(['list']);
            expect(list.stdout).toContain('echo');
            expect(list.stdout).toContain('RUNNING');
        });
    });

    describe('restart without running service', () => {
        it('should start the service when not already running', async () => {
            const result = await cli(['restart', 'echo']);

            // CLI starts the service (not an error)
            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Started');
        });
    });

    describe('restart transient processes', () => {
        it('should restart transient process with same shell', async () => {
            await cli(['start', 'my-transient', '--shell', 'node testProcess.js']);
            await cli(['wait-for-log', 'my-transient', '--message', 'Test server started']);

            const result = await cli(['restart', 'my-transient']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Started');

            // Verify it still uses same shell
            await cli(['wait-for-log', 'my-transient', '--message', 'Test server started']);
        });
    });

    describe('restart preserves configuration', () => {
        it('should use stored shell command from DB', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Restart should use same command
            await cli(['restart', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const logs = await cli(['logs', 'echo']);
            expect(logs.stdout).toContain('Echo server started');
        });
    });

    describe('restart output format', () => {
        it('should have clear success message', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['restart', 'echo']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Started');
        });

        it('should have no stderr on success', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['restart', 'echo']);

            expect(result.code).toBe(0);
            expect(result.stderr).toBe('');
        });
    });

    describe('restart without name', () => {
        it('should restart default service when no name provided', async () => {
            // Start echo instead of web to avoid port conflicts
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Restart with no name restarts the first running service
            const result = await cli(['restart']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Started');
        });
    });

    describe('restart unknown service', () => {
        it('should report failure message for unknown service', async () => {
            const result = await cli(['restart', 'nonexistent-service']);

            // CLI exits 0 but reports failure in stderr
            expect(result.code).toBe(0);
            expect(result.stderr).toContain('Failed to restart');
            expect(result.stderr).toContain('No service');
        });
    });

    describe('start over existing process', () => {
        it('should not print duplicate kill message when starting over existing process', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const startResult = await cli(['start', 'echo']);

            const startOutput = startResult.stdout + startResult.stderr;
            const killedCount = (startOutput.match(/Killed/g) || []).length;

            expect(killedCount).toBeLessThanOrEqual(1);
        });
    });
});
