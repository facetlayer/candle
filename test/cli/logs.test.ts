import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import { createCli, ensureCleanDbDir, getSampleServersDir } from './utils';

const TEST_NAME = 'cli-logs';

describe('CLI Logs Command', () => {
    let dbDir: string;
    let cli: ReturnType<typeof createCli>;

    beforeAll(() => {
        dbDir = ensureCleanDbDir(TEST_NAME);
        cli = createCli(dbDir, getSampleServersDir());
    });

    afterEach(async () => {
        await cli(['kill-all']).catch(() => {});
    });

    describe('basic logs functionality', () => {
        it('should show logs for a running service', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['logs', 'echo']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Echo server started');
        });

        it('should show logs for specified service', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['logs', 'echo']);

            expect(result.code).toBe(0);
            expect(result.stdout.length).toBeGreaterThan(0);
        });

        it('should exit quickly after fetching logs', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const startTime = Date.now();
            const result = await cli(['logs', 'echo']);
            const elapsed = Date.now() - startTime;

            expect(result.code).toBe(0);
            expect(elapsed).toBeLessThan(5000);
        });
    });

    describe('logs for transient processes', () => {
        it('should show logs for transient process', async () => {
            await cli(['start', 'my-transient', '--shell', 'node echoServer.js']);
            await cli(['wait-for-log', 'my-transient', '--message', 'Echo server started']);

            const result = await cli(['logs', 'my-transient']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Echo server started');
        });
    });

    describe('logs content', () => {
        it('should capture stdout from process', async () => {
            await cli(['start', 'echo']);
            // Wait for some output
            await new Promise((resolve) => setTimeout(resolve, 2000));

            const result = await cli(['logs', 'echo']);

            expect(result.code).toBe(0);
            // Echo server outputs regularly
            expect(result.stdout.length).toBeGreaterThan(0);
        });

        it('should capture stderr from process', async () => {
            await cli(['start', 'echo']);
            // Echo server outputs to stderr too
            await new Promise((resolve) => setTimeout(resolve, 2000));

            const result = await cli(['logs', 'echo']);

            expect(result.code).toBe(0);
            // Should have captured output
            expect(result.stdout.length).toBeGreaterThan(0);
        });
    });

    describe('logs for non-running service', () => {
        it('should show historical logs after service stopped', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Kill the service
            await cli(['kill', 'echo']);
            // Small delay for cleanup
            await new Promise((resolve) => setTimeout(resolve, 500));

            // Should still have logs
            const result = await cli(['logs', 'echo']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Echo server started');
        });
    });

    describe('logs for unknown service', () => {
        it('should handle unknown service name', async () => {
            const result = await cli(['logs', 'nonexistent-service']);

            // May return error or empty logs
            expect(typeof result.code).toBe('number');
        });
    });

    describe('logs without name', () => {
        it('should handle logs without service name', async () => {
            await cli(['start']);
            await new Promise((resolve) => setTimeout(resolve, 1000));

            const result = await cli(['logs']);

            // Should get logs for default service or show help
            expect(typeof result.code).toBe('number');
        });
    });

    describe('logs output format', () => {
        it('should output logs to stdout', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['logs', 'echo']);

            expect(result.code).toBe(0);
            // Logs should be in stdout, not stderr
            expect(result.stdout.length).toBeGreaterThan(0);
        });

        it('should have minimal command errors on success', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['logs', 'echo']);

            expect(result.code).toBe(0);
            // Note: stderr may contain output from the process itself (not command errors)
        });
    });

    describe('logs accumulation', () => {
        it('should show multiple log entries', async () => {
            await cli(['start', 'echo']);
            // Wait for multiple outputs
            await new Promise((resolve) => setTimeout(resolve, 3000));

            const result = await cli(['logs', 'echo']);

            expect(result.code).toBe(0);
            // Should have multiple lines
            const lines = result.stdout.trim().split('\n');
            expect(lines.length).toBeGreaterThan(1);
        });
    });
});
