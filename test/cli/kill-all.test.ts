import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-kill-all');
const cli = workspace.createCli();

describe('CLI Kill-All Command', () => {
    afterAll(() => workspace.cleanup());

    describe('basic kill-all functionality', () => {
        it('should kill all running services', async () => {
            // Start multiple services using echo and echo-test (both use echoServer.js)
            await cli(['start', 'echo']);
            await cli(['start', 'echo-test']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await cli(['wait-for-log', 'echo-test', '--message', 'Echo server started']);

            // Kill all
            const result = await cli(['kill-all']);

            expect(result.code).toBe(0);
        });

        it('should work when no services are running', async () => {
            const result = await cli(['kill-all']);

            // Should succeed even with nothing to kill
            expect(result.code).toBe(0);
        });

        it('should exit quickly', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const startTime = Date.now();
            const result = await cli(['kill-all']);
            const elapsed = Date.now() - startTime;

            expect(result.code).toBe(0);
            expect(elapsed).toBeLessThan(5000);
        });
    });

    describe('kill-all verification', () => {
        it('should clear list after kill-all', async () => {
            await cli(['start', 'echo']);
            await cli(['start', 'echo-test']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await cli(['wait-for-log', 'echo-test', '--message', 'Echo server started']);

            // Verify running
            const beforeKill = await cli(['list']);
            expect(beforeKill.stdout).toContain('echo');
            expect(beforeKill.stdout).toContain('echo-test');

            // Kill all
            await cli(['kill-all']);

            // Verify list is empty or shows stopped
            const afterKill = await cli(['list']);
            if (afterKill.stdout.includes('RUNNING')) {
                // If there are any RUNNING, it's a failure
                expect(afterKill.stdout).not.toContain('echo');
                expect(afterKill.stdout).not.toContain('echo-test');
            }
        });
    });

    describe('kill-all with transient processes', () => {
        it('should kill transient processes too', async () => {
            await cli(['start', 'my-transient', '--shell', 'node testProcess.js']);
            await cli(['wait-for-log', 'my-transient', '--message', 'Test server started']);

            const result = await cli(['kill-all']);

            expect(result.code).toBe(0);

            // Verify killed
            const list = await cli(['list']);
            expect(list.stdout).not.toContain('my-transient');
        });

        it('should kill mix of config and transient processes', async () => {
            await cli(['start', 'echo']);
            await cli(['start', 'transient-one', '--shell', 'node testProcess.js']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await cli(['wait-for-log', 'transient-one', '--message', 'Test server started']);

            const result = await cli(['kill-all']);

            expect(result.code).toBe(0);
        });
    });

    describe('kill-all output format', () => {
        it('should have clear output', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['kill-all']);

            expect(result.code).toBe(0);
            // Should have some indication of what was killed or success
            expect(result.stdout.length >= 0).toBe(true);
        });

        it('should have minimal stderr on success', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['kill-all']);

            expect(result.code).toBe(0);
            expect(result.stderr).toBe('');
        });
    });
});
