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
        it('should error for a service that is not configured', async () => {
            const result = await workspace.runCli([
                'wait-for-log',
                'nonexistent-service',
                '--message',
                'hello',
                '--timeout',
                '1',
            ], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain("No service 'nonexistent-service' configured");
        }, 5000);

        it('should fail at once when the service is not running', async () => {
            await workspace.runCli(['kill', 'web']);

            const startTime = Date.now();
            const result = await workspace.runCli([
                'wait-for-log', 'web', '--message', 'never printed',
            ], { ignoreExitCode: true });
            const elapsed = Date.now() - startTime;

            expect(result.failed()).toBe(true);
            // The default timeout is 30s; a stopped service must not wait it out.
            expect(elapsed).toBeLessThan(5000);
            expect(result.stderrAsString()).toContain("Error: Service 'web' is not running");
        }, 15000);

        it('should fail at once when the latest run already exited', async () => {
            await workspace.runCli([
                'start', 'exits-fast', '--shell', 'node ../../sampleServers/delayedExitServer.js 1 300',
            ], { ignoreExitCode: true });
            // Let the process exit.
            await new Promise((resolve) => setTimeout(resolve, 1500));

            const startTime = Date.now();
            const result = await workspace.runCli([
                'wait-for-log', 'exits-fast', '--message', 'never printed',
            ], { ignoreExitCode: true });
            const elapsed = Date.now() - startTime;

            expect(result.failed()).toBe(true);
            expect(elapsed).toBeLessThan(5000);
            expect(result.stderrAsString()).toContain("Error: Service 'exits-fast' is not running");
        }, 15000);

        it('should still find a message a finished run printed', async () => {
            await workspace.runCli([
                'start', 'exits-fast-2', '--shell', 'node ../../sampleServers/delayedExitServer.js 0 300',
            ], { ignoreExitCode: true });
            await new Promise((resolve) => setTimeout(resolve, 1500));

            const result = await workspace.runCli(['wait-for-log', 'exits-fast-2', '--message', 'exiting with code 0']);
            expect(result.stdoutAsString()).toContain('Found message');
        }, 15000);
    });

    describe('wait-for-log timeout output', () => {
        it('should show only the tail of the latest run, plus a logs hint', async () => {
            // A previous run whose output must not be shown.
            await workspace.runCli(['start', 'tail-test', '--shell', 'node ../../sampleServers/burstServer.js 5 oldrun']);
            await workspace.runCli(['wait-for-log', 'tail-test', '--message', 'oldrun done']);
            await workspace.runCli(['kill', 'tail-test']);

            await workspace.runCli(['start', 'tail-test', '--shell', 'node ../../sampleServers/burstServer.js 60 newrun']);
            await workspace.runCli(['wait-for-log', 'tail-test', '--message', 'newrun done']);

            const result = await workspace.runCli([
                'wait-for-log', 'tail-test', '--message', 'never printed', '--timeout', '1',
            ], { ignoreExitCode: true });
            const output = result.stdoutAsString();

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('Error: Timed out');
            expect(output).not.toContain('oldrun');
            const lines = output.split('\n').filter((l) => l.startsWith('newrun'));
            expect(lines.length).toBe(20);
            expect(lines[lines.length - 1]).toBe('newrun done');
            expect(output).toContain("Run 'candle logs tail-test' to see more.");

            await workspace.runCli(['kill', 'tail-test']);
        }, 20000);
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
        }, 15000);
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
