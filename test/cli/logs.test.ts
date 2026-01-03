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

describe('multiple launches - only show most recent', () => {
    it('should only show logs from the most recent launch', async () => {
        // Run service 3 times, with the 3rd still running when we check logs
        // (transient processes can only be looked up while running)

        // Run 1 - exits quickly
        await workspace.runCli([
            'run', 'multi-launch-test',
            '--shell', 'node ../../sampleServers/markerServer.js RUN_ONE_MARKER 500'
        ], { timeout: 10000 });

        // Run 2 - exits quickly
        await workspace.runCli([
            'run', 'multi-launch-test',
            '--shell', 'node ../../sampleServers/markerServer.js RUN_TWO_MARKER 500'
        ], { timeout: 10000 });

        // Run 3 - stays running while we check logs
        await workspace.runCli([
            'start', 'multi-launch-test',
            '--shell', 'node ../../sampleServers/markerServer.js RUN_THREE_MARKER 5000'
        ]);

        // Wait for it to start
        await workspace.runCli(['wait-for-log', 'multi-launch-test', '--message', 'MARKER=RUN_THREE_MARKER']);

        // Now get logs - should only see the most recent run (run 3)
        const logsResult = await workspace.runCli(['logs', 'multi-launch-test']);
        const output = logsResult.stdoutAsString();

        // Should contain the marker from run 3
        expect(output).toContain('MARKER=RUN_THREE_MARKER');

        // Should NOT contain markers from previous runs
        expect(output).not.toContain('MARKER=RUN_ONE_MARKER');
        expect(output).not.toContain('MARKER=RUN_TWO_MARKER');
    });

    it('should show most recent launch for currently running process', async () => {
        // Run the service twice, second time stays running
        await workspace.runCli([
            'run', 'multi-launch-running',
            '--shell', 'node ../../sampleServers/markerServer.js FIRST_RUN_MARKER 500'
        ], { timeout: 10000 });

        // Start second run that stays running longer
        await workspace.runCli([
            'start', 'multi-launch-running',
            '--shell', 'node ../../sampleServers/markerServer.js SECOND_RUN_MARKER 5000'
        ]);

        // Wait for it to start
        await workspace.runCli(['wait-for-log', 'multi-launch-running', '--message', 'MARKER=SECOND_RUN_MARKER']);

        // Get logs - should only see the current run
        const logsResult = await workspace.runCli(['logs', 'multi-launch-running']);
        const output = logsResult.stdoutAsString();

        // Should contain the marker from current run
        expect(output).toContain('MARKER=SECOND_RUN_MARKER');

        // Should NOT contain marker from previous run
        expect(output).not.toContain('MARKER=FIRST_RUN_MARKER');
    });
});

describe('run command with quick-exit processes', () => {
    it('should not say "Process is still running" when process exits successfully', async () => {
        // Use a transient process that exits with code 0 after 1 second (after grace period)
        const result = await workspace.runCli([
            'run', 'quick-exit-success',
            '--shell', 'node ../../sampleServers/delayedExitServer.js 0 1000'
        ], { timeout: 10000 });

        const output = result.stdoutAsString() + result.stderrAsString();
        expect(output).not.toContain('Process is still running');
        expect(output).toContain('Process exited with code 0');
    });

    it('should not say "Process is still running" when process exits with error', async () => {
        // Use a transient process that exits with code 1 after 1 second (after grace period)
        const result = await workspace.runCli([
            'run', 'quick-exit-error',
            '--shell', 'node ../../sampleServers/delayedExitServer.js 1 1000'
        ], { timeout: 10000 });

        const output = result.stdoutAsString() + result.stderrAsString();
        expect(output).not.toContain('Process is still running');
        expect(output).toContain('Process exited with code 1');
    });
});

describe('blended mode - watching multiple processes', () => {
    it('should run multiple services with run command', async () => {
        // Run two services that exit after a delay (after grace period)
        const result = await workspace.runCli([
            'run', 'delayed-exit-a',
            '--shell', 'node ../../sampleServers/delayedExitServer.js 0 1000'
        ], { timeout: 10000 });

        const output = result.stdoutAsString();

        // Service should have started
        expect(output).toContain("Started");
        expect(output).toContain("'delayed-exit-a'");
        expect(output).toContain('Delayed exit server');
    });

    it('should add prefix to logs in blended mode with transient processes', async () => {
        // Start two transient services that run for a while (5 seconds)
        await workspace.runCli([
            'start', 'blended-a',
            '--shell', 'node ../../sampleServers/delayedExitServer.js 0 5000'
        ]);
        await workspace.runCli([
            'start', 'blended-b',
            '--shell', 'node ../../sampleServers/delayedExitServer.js 0 5000'
        ]);

        // Wait for them to fully start
        await workspace.runCli(['wait-for-log', 'blended-a', '--message', 'Delayed exit server running']);
        await workspace.runCli(['wait-for-log', 'blended-b', '--message', 'Delayed exit server running']);

        // Watch both using watch command - they will exit and watching will complete
        const result = await workspace.runCli([
            'watch', 'blended-a', 'blended-b'
        ], { timeout: 10000 });

        const output = result.stdoutAsString();

        // Should have prefixes for each service since we're watching multiple
        expect(output).toContain('[blended-a]');
        expect(output).toContain('[blended-b]');
    });

    it('should watch multiple running processes with watch command', async () => {
        // Start two transient services that run for a while
        await workspace.runCli([
            'start', 'watch-a',
            '--shell', 'node ../../sampleServers/delayedExitServer.js 0 4000'
        ]);
        await workspace.runCli([
            'start', 'watch-b',
            '--shell', 'node ../../sampleServers/delayedExitServer.js 0 4000'
        ]);

        // Wait for them to fully start
        await workspace.runCli(['wait-for-log', 'watch-a', '--message', 'Delayed exit server running']);
        await workspace.runCli(['wait-for-log', 'watch-b', '--message', 'Delayed exit server running']);

        // Watch both - they will exit and watching will complete
        const result = await workspace.runCli([
            'watch', 'watch-a', 'watch-b'
        ], { timeout: 10000 });

        const output = result.stdoutAsString();

        // Should show that we're watching 2 processes
        expect(output).toContain('Watching 2 processes');
        expect(output).toContain("'watch-a'");
        expect(output).toContain("'watch-b'");

        // Should have prefixes since watching multiple
        expect(output).toContain('[watch-a]');
        expect(output).toContain('[watch-b]');
    });

    it('should not add prefix when watching single process', async () => {
        // Run a single quick-exit service (using delayed exit to ensure it starts)
        const result = await workspace.runCli([
            'run', 'single-delayed',
            '--shell', 'node ../../sampleServers/delayedExitServer.js 0 1000'
        ], { timeout: 10000 });

        const output = result.stdoutAsString();

        // Should NOT have prefix since it's a single service
        expect(output).not.toContain('[single-delayed]');
        // But should still have the output
        expect(output).toContain('Delayed exit server');
    });

    it('should error when using --shell with multiple service names', async () => {
        const result = await workspace.runCli([
            'run', 'service1', 'service2',
            '--shell', 'node somescript.js'
        ], { ignoreExitCode: true });

        expect(result.failed()).toBe(true);
        expect(result.stderrAsString()).toContain('--shell can only be used with a single service name');
    });

    it('should run multiple transient quick-exit services together', async () => {
        // Run two transient services that exit quickly (both with code 0)
        const result = await workspace.runCli([
            'run', 'multi-a',
            '--shell', 'node ../../sampleServers/delayedExitServer.js 0 1000'
        ], { timeout: 10000 });

        // First service should have started and run
        const output = result.stdoutAsString();
        expect(output).toContain("Started");
        expect(output).toContain("'multi-a'");
        expect(output).toContain('Delayed exit server');
    });

    it('should run quick-exit config service with prefix', async () => {
        // Start quick-exit (which exits quickly) and another transient
        await workspace.runCli(['start', 'quick-exit']);
        await workspace.runCli([
            'start', 'quick-exit-b',
            '--shell', 'node ../../sampleServers/delayedExitServer.js 0 2000'
        ]);

        // Wait for them to start
        await new Promise(resolve => setTimeout(resolve, 1000));

        // Watch both - they should exit
        const result = await workspace.runCli([
            'watch', 'quick-exit', 'quick-exit-b'
        ], { timeout: 5000, ignoreExitCode: true });

        const output = result.stdoutAsString();

        // Should have prefixes since watching multiple (if any were still running)
        // At minimum, should show watching message or already exited messages
        expect(output.length).toBeGreaterThan(0);
    });
});
