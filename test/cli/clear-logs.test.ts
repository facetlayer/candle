import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-clear-logs');

describe('CLI Clear-Logs Command', () => {
    afterAll(() => workspace.cleanup());

    describe('basic clear-logs functionality', () => {
        it('should clear logs for a service', async () => {
            // Start and generate logs
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Verify logs exist
            const beforeClear = await workspace.runCli(['logs', 'echo']);
            expect(beforeClear.stdoutAsString()).toContain('Echo server started');

            // Kill service then clear logs
            await workspace.runCli(['kill', 'echo']);

            // Clear logs
            const result = await workspace.runCli(['clear-logs', 'echo']);

            expect(result.stdoutAsString()).toContain('Logs cleared successfully');

            // Verify logs are cleared
            const afterClear = await workspace.runCli(['logs', 'echo']);
            expect(afterClear.stdoutAsString()).not.toContain('Echo server started');
        });

        it('should exit quickly', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['kill', 'echo']);

            const startTime = Date.now();
            await workspace.runCli(['clear-logs', 'echo']);
            const elapsed = Date.now() - startTime;

            expect(elapsed).toBeLessThan(5000);
        });
    });

    describe('clear-logs for transient processes', () => {
        it('should clear logs for transient process', async () => {
            await workspace.runCli(['start', 'my-transient', '--shell', 'node ../../sampleServers/echoServer.js']);
            await workspace.runCli(['wait-for-log', 'my-transient', '--message', 'Echo server started']);
            await workspace.runCli(['kill', 'my-transient']);

            const result = await workspace.runCli(['clear-logs', 'my-transient']);

            expect(result.stdoutAsString()).toContain('Logs cleared successfully');
        });
    });

    describe('clear-logs without service name', () => {
        it('should clear all logs in directory', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['kill', 'echo']);

            const result = await workspace.runCli(['clear-logs']);

            expect(result.stdoutAsString()).toContain('Logs cleared successfully');
        });
    });

    describe('clear-logs for unknown service', () => {
        it('should succeed with no logs to clear message', async () => {
            const result = await workspace.runCli(['clear-logs', 'nonexistent-service']);

            expect(result.stdoutAsString()).toContain('No logs found to clear');
        });
    });

    describe('clear-logs output format', () => {
        it('should have confirmation message', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['kill', 'echo']);

            const result = await workspace.runCli(['clear-logs', 'echo']);

            expect(result.stdoutAsString()).toContain('Logs cleared successfully');
        });

        it('should have no stderr on success', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['kill', 'echo']);

            const result = await workspace.runCli(['clear-logs', 'echo']);

            expect(result.stderrAsString()).toBe('');
        });
    });

    describe('clear-logs while service running', () => {
        it('should clear logs even when service is running', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['clear-logs', 'echo']);

            expect(result.stdoutAsString()).toContain('Logs cleared successfully');
        });
    });
});
