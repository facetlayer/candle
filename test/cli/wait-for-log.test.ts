import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-wait-for-log');

describe('CLI Wait-For-Log Command', () => {
    afterAll(() => workspace.cleanup());

    describe('basic wait-for-log functionality', () => {
        it('should wait for specific log message', async () => {
            await workspace.runCli(['start', 'echo']);

            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
        });

        it('should return quickly when message already appeared', async () => {
            await workspace.runCli(['start', 'echo']);
            // Wait for it to start first
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Now wait again - should return immediately
            const startTime = Date.now();
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            const elapsed = Date.now() - startTime;

            expect(elapsed).toBeLessThan(2000);
        });
    });

    describe('--message option', () => {
        it('should require --message option', async () => {
            await workspace.runCli(['start', 'echo']);

            const result = await workspace.runCli(['wait-for-log', 'echo'], { ignoreExitCode: true });

            // Should error without --message
            expect(result.failed()).toBe(true);
        });

        it('should match partial message', async () => {
            await workspace.runCli(['start', 'echo']);

            // Echo server outputs "Echo server started"
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'server started']);
        });

        it('should match case-sensitive', async () => {
            await workspace.runCli(['start', 'echo']);

            // Should match exact case
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
        });
    });

    describe('--timeout option', () => {
        it('should use default timeout', async () => {
            await workspace.runCli(['start', 'echo']);

            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
        });

        it('should respect custom timeout', async () => {
            await workspace.runCli(['start', 'delayed-logger']);

            // Delayed logger outputs "Server ready" after 4 seconds
            await workspace.runCli(['wait-for-log', 'delayed-logger', '--message', 'Server ready', '--timeout', '10']);
        }, 15000);

        it('should timeout when message not found', async () => {
            await workspace.runCli(['start', 'echo']);

            // Wait for message that will never appear with short timeout
            const result = await workspace.runCli([
                'wait-for-log',
                'echo',
                '--message',
                'this message will never appear xyz123',
                '--timeout',
                '2',
            ], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        }, 10000);
    });

    describe('wait-for-log with non-running service', () => {
        it('should handle non-running service', async () => {
            const result = await workspace.runCli([
                'wait-for-log',
                'nonexistent-service',
                '--message',
                'hello',
                '--timeout',
                '1',
            ], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        }, 5000);
    });

    describe('wait-for-log with transient processes', () => {
        it('should work with transient process', async () => {
            await workspace.runCli(['start', 'my-transient', '--shell', 'node ../../sampleServers/echoServer.js']);

            await workspace.runCli(['wait-for-log', 'my-transient', '--message', 'Echo server started']);
        });
    });

    describe('wait-for-log output format', () => {
        it('should have minimal output on success', async () => {
            await workspace.runCli(['start', 'echo']);

            const result = await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Output should be minimal
            expect(result.stderrAsString()).toBe('');
        });

        it('should have error message on timeout', async () => {
            await workspace.runCli(['start', 'echo']);

            const result = await workspace.runCli([
                'wait-for-log',
                'echo',
                '--message',
                'impossible message xyz',
                '--timeout',
                '1',
            ], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            // Should have some indication of timeout
            const output = result.stdoutAsString() + result.stderrAsString();
            expect(output.toLowerCase()).toMatch(/timeout|not found|failed/);
        }, 5000);
    });

    describe('wait-for-log exit behavior', () => {
        it('should exit immediately when message found', async () => {
            await workspace.runCli(['start', 'echo']);
            // Give it time to start
            await new Promise((resolve) => setTimeout(resolve, 1500));

            const startTime = Date.now();
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            const elapsed = Date.now() - startTime;

            // Should be very quick since message already exists
            expect(elapsed).toBeLessThan(2000);
        });
    });
});
