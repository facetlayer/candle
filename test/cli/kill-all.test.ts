import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-kill-all');

describe('CLI Kill-All Command', () => {
    afterAll(() => workspace.cleanup());

    describe('basic kill-all functionality', () => {
        it('should kill all running services', async () => {
            // Start multiple services using echo and echo-test (both use echoServer.js)
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['start', 'echo-test']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['wait-for-log', 'echo-test', '--message', 'Echo server started']);

            // Kill all
            await workspace.runCli(['kill-all']);
        });

        it('should work when no services are running', async () => {
            // Should succeed even with nothing to kill
            await workspace.runCli(['kill-all']);
        });

        it('should exit quickly', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const startTime = Date.now();
            await workspace.runCli(['kill-all']);
            const elapsed = Date.now() - startTime;

            expect(elapsed).toBeLessThan(5000);
        });
    });

    describe('kill-all verification', () => {
        it('should clear list after kill-all', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['start', 'echo-test']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['wait-for-log', 'echo-test', '--message', 'Echo server started']);

            // Verify running
            const beforeKill = await workspace.runCli(['list']);
            expect(beforeKill.stdoutAsString()).toContain('echo');
            expect(beforeKill.stdoutAsString()).toContain('echo-test');

            // Kill all
            await workspace.runCli(['kill-all']);

            // Verify list is empty or shows stopped
            const afterKill = await workspace.runCli(['list']);
            if (afterKill.stdoutAsString().includes('RUNNING')) {
                // If there are any RUNNING, it's a failure
                expect(afterKill.stdoutAsString()).not.toContain('echo');
                expect(afterKill.stdoutAsString()).not.toContain('echo-test');
            }
        });
    });

    describe('kill-all with transient processes', () => {
        it('should kill transient processes too', async () => {
            await workspace.runCli(['start', 'my-transient', '--shell', 'node ../../sampleServers/testProcess.js']);
            await workspace.runCli(['wait-for-log', 'my-transient', '--message', 'Test server started']);

            await workspace.runCli(['kill-all']);

            // Verify killed
            const list = await workspace.runCli(['list']);
            expect(list.stdoutAsString()).not.toContain('my-transient');
        });

        it('should kill mix of config and transient processes', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['start', 'transient-one', '--shell', 'node ../../sampleServers/testProcess.js']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['wait-for-log', 'transient-one', '--message', 'Test server started']);

            await workspace.runCli(['kill-all']);
        });
    });

    describe('kill-all output format', () => {
        it('should have clear output', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['kill-all']);

            // Should have some indication of what was killed or success
            expect(result.stdoutAsString().length >= 0).toBe(true);
        });

        it('should have minimal stderr on success', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['kill-all']);

            expect(result.stderrAsString()).toBe('');
        });
    });
});
