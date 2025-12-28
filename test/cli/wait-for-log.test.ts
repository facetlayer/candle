import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import { createCli, ensureCleanDbDir, getSampleServersDir } from './utils';

const TEST_NAME = 'cli-wait-for-log';

describe('CLI Wait-For-Log Command', () => {
    let dbDir: string;
    let cli: ReturnType<typeof createCli>;

    beforeAll(() => {
        dbDir = ensureCleanDbDir(TEST_NAME);
        cli = createCli(dbDir, getSampleServersDir());
    });

    afterEach(async () => {
        await cli(['kill-all']).catch(() => {});
    });

    describe('basic wait-for-log functionality', () => {
        it('should wait for specific log message', async () => {
            await cli(['start', 'echo']);

            const result = await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            expect(result.code).toBe(0);
        });

        it('should return quickly when message already appeared', async () => {
            await cli(['start', 'echo']);
            // Wait for it to start first
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Now wait again - should return immediately
            const startTime = Date.now();
            const result = await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            const elapsed = Date.now() - startTime;

            expect(result.code).toBe(0);
            expect(elapsed).toBeLessThan(2000);
        });
    });

    describe('--message option', () => {
        it('should require --message option', async () => {
            await cli(['start', 'echo']);

            const result = await cli(['wait-for-log', 'echo']);

            // Should error without --message
            expect(result.code).not.toBe(0);
        });

        it('should match partial message', async () => {
            await cli(['start', 'echo']);

            // Echo server outputs "Echo server started"
            const result = await cli(['wait-for-log', 'echo', '--message', 'server started']);

            expect(result.code).toBe(0);
        });

        it('should match case-sensitive', async () => {
            await cli(['start', 'echo']);

            // Should match exact case
            const result = await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            expect(result.code).toBe(0);
        });
    });

    describe('--timeout option', () => {
        it('should use default timeout', async () => {
            await cli(['start', 'echo']);

            const result = await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            expect(result.code).toBe(0);
        });

        it('should respect custom timeout', async () => {
            await cli(['start', 'delayed-logger']);

            // Delayed logger outputs "Server ready" after 4 seconds
            const result = await cli(['wait-for-log', 'delayed-logger', '--message', 'Server ready', '--timeout', '10']);

            expect(result.code).toBe(0);
        }, 15000);

        it('should timeout when message not found', async () => {
            await cli(['start', 'echo']);

            // Wait for message that will never appear with short timeout
            const result = await cli([
                'wait-for-log',
                'echo',
                '--message',
                'this message will never appear xyz123',
                '--timeout',
                '2',
            ]);

            expect(result.code).not.toBe(0);
        }, 10000);
    });

    describe('wait-for-log with non-running service', () => {
        it('should handle non-running service', async () => {
            const result = await cli([
                'wait-for-log',
                'nonexistent-service',
                '--message',
                'hello',
                '--timeout',
                '1',
            ]);

            expect(result.code).not.toBe(0);
        }, 5000);
    });

    describe('wait-for-log with transient processes', () => {
        it('should work with transient process', async () => {
            await cli(['start', 'my-transient', '--shell', 'node echoServer.js']);

            const result = await cli(['wait-for-log', 'my-transient', '--message', 'Echo server started']);

            expect(result.code).toBe(0);
        });
    });

    describe('wait-for-log output format', () => {
        it('should have minimal output on success', async () => {
            await cli(['start', 'echo']);

            const result = await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            expect(result.code).toBe(0);
            // Output should be minimal
            expect(result.stderr).toBe('');
        });

        it('should have error message on timeout', async () => {
            await cli(['start', 'echo']);

            const result = await cli([
                'wait-for-log',
                'echo',
                '--message',
                'impossible message xyz',
                '--timeout',
                '1',
            ]);

            expect(result.code).not.toBe(0);
            // Should have some indication of timeout
            const output = result.stdout + result.stderr;
            expect(output.toLowerCase()).toMatch(/timeout|not found|failed/);
        }, 5000);
    });

    describe('wait-for-log exit behavior', () => {
        it('should exit immediately when message found', async () => {
            await cli(['start', 'echo']);
            // Give it time to start
            await new Promise((resolve) => setTimeout(resolve, 1500));

            const startTime = Date.now();
            const result = await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            const elapsed = Date.now() - startTime;

            expect(result.code).toBe(0);
            // Should be very quick since message already exists
            expect(elapsed).toBeLessThan(2000);
        });
    });
});
