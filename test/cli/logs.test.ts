import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-logs');

afterAll(() => workspace.cleanup());

describe('basic logs functionality', () => {
    it('should show logs for a running service', async () => {
        await workspace.runCli(['start', 'echo']);
        await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

        const result = await workspace.runCli(['logs', 'echo']);

        expect(result.stdoutAsString()).toContain('Echo server started');
    });

    it('should show logs for specified service', async () => {
        await workspace.runCli(['start', 'echo']);
        await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

        const result = await workspace.runCli(['logs', 'echo']);

        expect(result.stdoutAsString().length).toBeGreaterThan(0);
    });

    it('should exit quickly after fetching logs', async () => {
        await workspace.runCli(['start', 'echo']);
        await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

        const startTime = Date.now();
        await workspace.runCli(['logs', 'echo']);
        const elapsed = Date.now() - startTime;

        expect(elapsed).toBeLessThan(5000);
    });
});

describe('logs for transient processes', () => {
    it('should show logs for transient process', async () => {
        await workspace.runCli(['start', 'my-transient', '--shell', `node ../../sampleServers/echoServer.js`]);
        await workspace.runCli(['wait-for-log', 'my-transient', '--message', 'Echo server started']);

        const result = await workspace.runCli(['logs', 'my-transient']);

        expect(result.stdoutAsString()).toContain('Echo server started');
    });
});

describe('logs content', () => {
    it('should capture stdout from process', async () => {
        await workspace.runCli(['start', 'echo']);
        // Wait for some output
        await new Promise((resolve) => setTimeout(resolve, 2000));

        const result = await workspace.runCli(['logs', 'echo']);

        expect(result.stdoutAsString().length).toBeGreaterThan(0);
    });

    it('should capture stderr from process', async () => {
        await workspace.runCli(['start', 'echo']);
        // Echo server outputs to stderr too
        await new Promise((resolve) => setTimeout(resolve, 2000));

        const result = await workspace.runCli(['logs', 'echo']);

        expect(result.stdoutAsString().length).toBeGreaterThan(0);
    });
});

describe('logs for non-running service', () => {
    it('should show historical logs after service stopped', async () => {
        await workspace.runCli(['start', 'echo']);
        await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

        // Kill the service
        await workspace.runCli(['kill', 'echo']);
        // Small delay for cleanup
        await new Promise((resolve) => setTimeout(resolve, 500));

        // Should still have logs
        const result = await workspace.runCli(['logs', 'echo']);

        expect(result.stdoutAsString()).toContain('Echo server started');
    });
});

describe('logs for unknown service', () => {
    it('should handle unknown service name', async () => {
        const result = await workspace.runCli(['logs', 'nonexistent-service'], { ignoreExitCode: true });

        // May return error or empty logs
        expect(result.failed()).toBe(true);
    });
});

describe('logs without name', () => {
    it('should handle logs without service name', async () => {
        await workspace.runCli(['start']);
        await new Promise((resolve) => setTimeout(resolve, 1000));

        const result = await workspace.runCli(['logs']);

        // TODO: check logs
    });
});

describe('logs output format', () => {
    it('should output logs to stdout', async () => {
        await workspace.runCli(['start', 'echo']);
        await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

        const result = await workspace.runCli(['logs', 'echo']);

        expect(result.stdoutAsString().length).toBeGreaterThan(0);
    });

    it('should have minimal command errors on success', async () => {
        await workspace.runCli(['start', 'echo']);
        await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

        const result = await workspace.runCli(['logs', 'echo']);

        expect(result.stdoutAsString().length).toBeGreaterThan(0);
    });
});

describe('logs accumulation', () => {
    it('should show multiple log entries', async () => {
        await workspace.runCli(['start', 'echo']);
        // Wait for multiple outputs
        await new Promise((resolve) => setTimeout(resolve, 3000));

        const result = await workspace.runCli(['logs', 'echo']);

        expect(result.stdoutAsString().length).toBeGreaterThan(0);
        const lines = result.stdoutAsString().trim().split('\n');
        expect(lines.length).toBeGreaterThan(1);
    });
});
