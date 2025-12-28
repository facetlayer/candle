import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import { createCli, ensureCleanDbDir, getSampleServersDir } from './utils';

const TEST_NAME = 'cli-kill';

describe('CLI Kill Command', () => {
    let dbDir: string;
    let cli: ReturnType<typeof createCli>;

    beforeAll(() => {
        dbDir = ensureCleanDbDir(TEST_NAME);
        cli = createCli(dbDir, getSampleServersDir());
    });

    afterEach(async () => {
        await cli(['kill-all']).catch(() => {});
    });

    describe('basic kill functionality', () => {
        it('should kill a running service', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['kill', 'echo']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Killed');
            expect(result.stdout).toContain('echo');
        });

        it('should exit quickly after killing', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const startTime = Date.now();
            const result = await cli(['kill', 'echo']);
            const elapsed = Date.now() - startTime;

            expect(result.code).toBe(0);
            expect(elapsed).toBeLessThan(5000);
        });

        it('should succeed with message when killing non-running service', async () => {
            const result = await cli(['kill', 'echo']);

            // CLI exits 0 and reports no running processes (echo is a known service)
            expect(result.code).toBe(0);
            expect(result.stdout).toContain('No running processes');
        });

        it('should error when killing unknown service', async () => {
            const result = await cli(['kill', 'nonexistent-service']);

            // CLI exits 1 for unknown service
            expect(result.code).toBe(1);
            expect(result.stderr).toContain('No service');
            expect(result.stderr).toContain('nonexistent-service');
        });
    });

    describe('stop alias', () => {
        it('should work with stop command', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['stop', 'echo']);

            expect(result.code).toBe(0);
        });

        it('should behave same as kill', async () => {
            // Start and kill with 'kill'
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            const killResult = await cli(['kill', 'echo']);

            // Start and kill with 'stop'
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            const stopResult = await cli(['stop', 'echo']);

            expect(killResult.code).toBe(0);
            expect(stopResult.code).toBe(0);
        });
    });

    describe('killing transient processes', () => {
        it('should kill transient process', async () => {
            await cli(['start', 'my-transient', '--shell', 'node testProcess.js']);
            await cli(['wait-for-log', 'my-transient', '--message', 'Test server started']);

            const result = await cli(['kill', 'my-transient']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Killed');
        });
    });

    describe('kill verification', () => {
        it('should remove process from list after kill', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Verify running
            const beforeKill = await cli(['list']);
            expect(beforeKill.stdout).toContain('echo');
            expect(beforeKill.stdout).toContain('RUNNING');

            // Kill
            await cli(['kill', 'echo']);

            // Verify not in list
            const afterKill = await cli(['list']);
            expect(afterKill.stdout).not.toContain('RUNNING');
        });
    });

    describe('kill without name', () => {
        it('should report no running processes when none running', async () => {
            const result = await cli(['kill']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('No running processes');
        });
    });

    describe('kill output format', () => {
        it('should have clear success message', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['kill', 'echo']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Killed');
        });

        it('should have no stderr on success', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['kill', 'echo']);

            expect(result.code).toBe(0);
            expect(result.stderr).toBe('');
        });
    });

    describe('killing multiple services', () => {
        it('should kill multiple services at once', async () => {
            // Start two services
            await cli(['start', 'echo', 'echo2']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await cli(['wait-for-log', 'echo2', '--message', 'Echo server started']);

            // Verify both are running
            const beforeKill = await cli(['list']);
            expect(beforeKill.stdout).toContain('echo');
            expect(beforeKill.stdout).toContain('echo2');

            // Kill both at once
            const result = await cli(['kill', 'echo', 'echo2']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Killed');
            expect(result.stdout).toContain('echo');
            expect(result.stdout).toContain('echo2');

            // Verify both are gone
            const afterKill = await cli(['list']);
            expect(afterKill.stdout).not.toContain('RUNNING');
        });

        it('should handle mix of running and non-running services', async () => {
            // Start only one service
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Kill both (echo2 is not running)
            const result = await cli(['kill', 'echo', 'echo2']);

            expect(result.code).toBe(0);
            // echo should be killed
            expect(result.stdout).toContain('Killed');
            expect(result.stdout).toContain('echo');
            // echo2 should report no running processes
            expect(result.stdout).toContain('No running processes');
        });

        it('should error when any service is unknown', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['kill', 'echo', 'nonexistent-service']);

            // CLI exits 1 for unknown service
            expect(result.code).toBe(1);
            expect(result.stderr).toContain('nonexistent-service');
        });

        it('should work with stop alias for multiple services', async () => {
            await cli(['start', 'echo', 'echo2']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await cli(['wait-for-log', 'echo2', '--message', 'Echo server started']);

            const result = await cli(['stop', 'echo', 'echo2']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Killed');
        });

        it('should kill multiple transient processes', async () => {
            await cli(['start', 'transient1', '--shell', 'node testProcess.js']);
            await cli(['start', 'transient2', '--shell', 'node testProcess.js']);
            await cli(['wait-for-log', 'transient1', '--message', 'Test server started']);
            await cli(['wait-for-log', 'transient2', '--message', 'Test server started']);

            const result = await cli(['kill', 'transient1', 'transient2']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('transient1');
            expect(result.stdout).toContain('transient2');
        });
    });
});
