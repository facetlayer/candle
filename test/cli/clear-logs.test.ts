import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import { createCli, ensureCleanDbDir, getSampleServersDir } from './utils';

const TEST_NAME = 'cli-clear-logs';

describe('CLI Clear-Logs Command', () => {
    let dbDir: string;
    let cli: ReturnType<typeof createCli>;

    beforeAll(() => {
        dbDir = ensureCleanDbDir(TEST_NAME);
        cli = createCli(dbDir, getSampleServersDir());
    });

    afterEach(async () => {
        await cli(['kill-all']).catch(() => {});
    });

    describe('basic clear-logs functionality', () => {
        it('should clear logs for a service', async () => {
            // Start and generate logs
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Verify logs exist
            const beforeClear = await cli(['logs', 'echo']);
            expect(beforeClear.stdout).toContain('Echo server started');

            // Kill service then clear logs
            await cli(['kill', 'echo']);

            // Clear logs
            const result = await cli(['clear-logs', 'echo']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Logs cleared successfully');

            // Verify logs are cleared
            const afterClear = await cli(['logs', 'echo']);
            expect(afterClear.stdout).not.toContain('Echo server started');
        });

        it('should exit quickly', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await cli(['kill', 'echo']);

            const startTime = Date.now();
            const result = await cli(['clear-logs', 'echo']);
            const elapsed = Date.now() - startTime;

            expect(result.code).toBe(0);
            expect(elapsed).toBeLessThan(5000);
        });
    });

    describe('clear-logs for transient processes', () => {
        it('should clear logs for transient process', async () => {
            await cli(['start', 'my-transient', '--shell', 'node echoServer.js']);
            await cli(['wait-for-log', 'my-transient', '--message', 'Echo server started']);
            await cli(['kill', 'my-transient']);

            const result = await cli(['clear-logs', 'my-transient']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Logs cleared successfully');
        });
    });

    describe('clear-logs without service name', () => {
        it('should clear all logs in directory', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await cli(['kill', 'echo']);

            const result = await cli(['clear-logs']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Logs cleared successfully');
        });
    });

    describe('clear-logs for unknown service', () => {
        it('should succeed with no logs to clear message', async () => {
            const result = await cli(['clear-logs', 'nonexistent-service']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('No logs found to clear');
        });
    });

    describe('clear-logs output format', () => {
        it('should have confirmation message', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await cli(['kill', 'echo']);

            const result = await cli(['clear-logs', 'echo']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Logs cleared successfully');
        });

        it('should have no stderr on success', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await cli(['kill', 'echo']);

            const result = await cli(['clear-logs', 'echo']);

            expect(result.code).toBe(0);
            expect(result.stderr).toBe('');
        });
    });

    describe('clear-logs while service running', () => {
        it('should clear logs even when service is running', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['clear-logs', 'echo']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Logs cleared successfully');
        });
    });
});
