import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-kill');

describe('CLI Kill Command', () => {
    afterAll(() => workspace.cleanup());

    describe('basic kill functionality', () => {
        it('should kill a running service', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['kill', 'echo']);

            expect(result.stdoutAsString()).toContain('Killed');
            expect(result.stdoutAsString()).toContain('echo');
        });

        it('should exit quickly after killing', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const startTime = Date.now();
            await workspace.runCli(['kill', 'echo']);
            const elapsed = Date.now() - startTime;

            expect(elapsed).toBeLessThan(5000);
        });

        it('should succeed with message when killing non-running service', async () => {
            const result = await workspace.runCli(['kill', 'echo']);

            // CLI exits 0 and reports no running processes (echo is a known service)
            expect(result.stdoutAsString()).toContain('No running processes');
        });

        it('should error when killing unknown service', async () => {
            const result = await workspace.runCli(['kill', 'nonexistent-service'], { ignoreExitCode: true });

            // CLI exits 1 for unknown service
            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('No service');
            expect(result.stderrAsString()).toContain('nonexistent-service');
        });
    });

    describe('stop alias', () => {
        it('should work with stop command', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            await workspace.runCli(['stop', 'echo']);
        });

        it('should behave same as kill', async () => {
            // Start and kill with 'kill'
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['kill', 'echo']);

            // Start and kill with 'stop'
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['stop', 'echo']);
        });
    });

    describe('killing transient processes', () => {
        it('should kill transient process', async () => {
            await workspace.runCli(['start', 'my-transient', '--shell', 'node ../../sampleServers/testProcess.js']);
            await workspace.runCli(['wait-for-log', 'my-transient', '--message', 'Test server started']);

            const result = await workspace.runCli(['kill', 'my-transient']);

            expect(result.stdoutAsString()).toContain('Killed');
        });
    });

    describe('kill verification', () => {
        it('should remove process from list after kill', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Verify running
            const beforeKill = await workspace.runCli(['list']);
            expect(beforeKill.stdoutAsString()).toContain('echo');
            expect(beforeKill.stdoutAsString()).toContain('RUNNING');

            // Kill
            await workspace.runCli(['kill', 'echo']);

            // Verify not in list
            const afterKill = await workspace.runCli(['list']);
            expect(afterKill.stdoutAsString()).not.toContain('RUNNING');
        });
    });

    describe('kill without name', () => {
        it('should report no running processes when none running', async () => {
            const result = await workspace.runCli(['kill']);

            expect(result.stdoutAsString()).toContain('No running processes');
        });
    });

    describe('kill output format', () => {
        it('should have clear success message', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['kill', 'echo']);

            expect(result.stdoutAsString()).toContain('Killed');
        });

        it('should have no stderr on success', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['kill', 'echo']);

            expect(result.stderrAsString()).toBe('');
        });
    });

    describe('killing multiple services', () => {
        it('should kill multiple services at once', async () => {
            // Start two services
            await workspace.runCli(['start', 'echo', 'echo2']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['wait-for-log', 'echo2', '--message', 'Echo server started']);

            // Verify both are running
            const beforeKill = await workspace.runCli(['list']);
            expect(beforeKill.stdoutAsString()).toContain('echo');
            expect(beforeKill.stdoutAsString()).toContain('echo2');

            // Kill both at once
            const result = await workspace.runCli(['kill', 'echo', 'echo2']);

            expect(result.stdoutAsString()).toContain('Killed');
            expect(result.stdoutAsString()).toContain('echo');
            expect(result.stdoutAsString()).toContain('echo2');

            // Verify both are gone
            const afterKill = await workspace.runCli(['list']);
            expect(afterKill.stdoutAsString()).not.toContain('RUNNING');
        });

        it('should handle mix of running and non-running services', async () => {
            // Start only one service
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Kill both (echo2 is not running)
            const result = await workspace.runCli(['kill', 'echo', 'echo2']);

            // echo should be killed
            expect(result.stdoutAsString()).toContain('Killed');
            expect(result.stdoutAsString()).toContain('echo');
            // echo2 should report no running processes
            expect(result.stdoutAsString()).toContain('No running processes');
        });

        it('should error when any service is unknown', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['kill', 'echo', 'nonexistent-service'], { ignoreExitCode: true });

            // CLI exits 1 for unknown service
            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('nonexistent-service');
        });

        it('should work with stop alias for multiple services', async () => {
            await workspace.runCli(['start', 'echo', 'echo2']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['wait-for-log', 'echo2', '--message', 'Echo server started']);

            const result = await workspace.runCli(['stop', 'echo', 'echo2']);

            expect(result.stdoutAsString()).toContain('Killed');
        });

        it('should kill multiple transient processes', async () => {
            await workspace.runCli(['start', 'transient1', '--shell', 'node ../../sampleServers/testProcess.js']);
            await workspace.runCli(['start', 'transient2', '--shell', 'node ../../sampleServers/testProcess.js']);
            await workspace.runCli(['wait-for-log', 'transient1', '--message', 'Test server started']);
            await workspace.runCli(['wait-for-log', 'transient2', '--message', 'Test server started']);

            const result = await workspace.runCli(['kill', 'transient1', 'transient2']);

            expect(result.stdoutAsString()).toContain('transient1');
            expect(result.stdoutAsString()).toContain('transient2');
        });
    });
});
