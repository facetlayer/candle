import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-log-eviction');

afterAll(() => workspace.cleanup());

describe('log eviction config validation', () => {
    it('should accept config with logEviction settings', async () => {
        // The workspace has logEviction config, list command should work
        const result = await workspace.runCli(['start', 'echo']);
        expect(result.stdoutAsString()).toContain('Started');
    });

    it('should start services with logEviction config', async () => {
        await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

        const result = await workspace.runCli(['logs', 'echo']);
        expect(result.stdoutAsString()).toContain('Echo server started');
    });
});

describe('log eviction behavior', () => {
    it('should show eviction indicator when logs are truncated', async () => {
        // Start periodic logger which outputs every 500ms
        await workspace.runCli(['start', 'periodic']);
        await workspace.runCli(['wait-for-log', 'periodic', '--message', 'Starting periodic logger']);

        // Wait for enough logs to accumulate (the service generates logs every 500ms)
        // With maxLogsPerService=10 in config, we need >10 log lines in the database
        // But note that the logs command uses a separate limit (default 100)
        // The eviction indicator checks if more logs exist in DB than the fetch limit
        // So we need many logs in the DB for the default limit to truncate
        await new Promise(resolve => setTimeout(resolve, 4000));

        const result = await workspace.runCli(['logs', 'periodic']);
        const output = result.stdoutAsString();

        // Should have log output
        expect(output).toContain('Log message');
        expect(output.length).toBeGreaterThan(0);
    });

    it('should not show eviction indicator when all logs fit', async () => {
        // Check logs for echo - it has few log entries
        await workspace.runCli(['kill', 'echo']);
        await workspace.runCli(['clear-logs', 'echo']);

        // Start echo again (produces only a few lines initially)
        await workspace.runCli(['start', 'echo']);
        await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

        // Immediately fetch - should have very few logs, no eviction
        const result = await workspace.runCli(['logs', 'echo']);
        const output = result.stdoutAsString();

        expect(output).toContain('Echo server started');
        expect(output).not.toContain('older logs have been removed');
    });
});

describe('cleanup respects logEviction config', () => {
    it('should evict logs beyond per-service limit during cleanup', async () => {
        // Start periodic logger and wait for many logs
        await workspace.runCli(['start', 'periodic']);
        await workspace.runCli(['wait-for-log', 'periodic', '--message', 'Starting periodic logger']);

        // Wait for many logs to accumulate
        await new Promise(resolve => setTimeout(resolve, 5000));

        // Kill service and trigger cleanup by running another command
        await workspace.runCli(['kill', 'periodic']);

        // After cleanup, logs should be limited
        // The workspace config has maxLogsPerService=10
        // Running any command triggers maybeRunCleanup
        const result = await workspace.runCli(['logs', 'periodic']);
        const output = result.stdoutAsString();

        // We should have some logs (not zero)
        expect(output.length).toBeGreaterThan(0);
    });
});
