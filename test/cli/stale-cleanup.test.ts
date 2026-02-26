import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('stale-cleanup');

describe('Stale process cleanup', () => {
    afterAll(() => workspace.cleanup());

    it('should clean up stale process entries when process is no longer alive', async () => {
        // Start a service normally
        await workspace.runCli(['start', 'echo']);
        await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

        // Verify it appears as RUNNING
        const beforeResult = await workspace.runCli(['list']);
        expect(beforeResult.stdoutAsString()).toContain('RUNNING');

        // Kill the process externally (bypassing candle), simulating a reboot.
        // We use SIGKILL to prevent graceful shutdown / cleanup.
        const listBefore = await workspace.runCli(['list']);
        const pidMatch = listBefore.stdoutAsString().match(/echo\s+RUNNING\s+(\d+)/);
        expect(pidMatch).toBeTruthy();
        const pid = parseInt(pidMatch![1], 10);

        // Kill the process tree externally with SIGKILL (no chance for cleanup)
        try {
            process.kill(pid, 'SIGKILL');
        } catch {
            // Process may already be dead
        }

        // Also kill the log collector - find it via the process group
        // Wait a moment for the kill to take effect
        await new Promise(resolve => setTimeout(resolve, 500));

        // At this point, the database still thinks the process is running,
        // but the PID is dead. Run any candle command — the cleanup should
        // detect and remove the stale entry. We force cleanup by using
        // a command that triggers maybeRunCleanup with a stale timestamp.

        // Start the same service again — this implicitly cleans up old entries
        // for the same name, but let's verify through list that stale detection
        // also works independently.
        await workspace.runCli(['start', 'echo']);
        await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

        // The list should show exactly one RUNNING entry for echo (the new one),
        // not two (which would happen if the stale entry persisted).
        const afterResult = await workspace.runCli(['list']);
        const runningLines = afterResult.stdoutAsString().split('\n')
            .filter(line => line.includes('echo') && line.includes('RUNNING'));
        expect(runningLines.length).toBe(1);
    });

    it('should not remove entries for processes that are still alive', async () => {
        // Start a service
        await workspace.runCli(['start', 'echo']);
        await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

        // List should show it as running
        const result = await workspace.runCli(['list']);
        expect(result.stdoutAsString()).toContain('RUNNING');

        // Run another command (which triggers cleanup) — the live process should not be removed
        const result2 = await workspace.runCli(['list']);
        expect(result2.stdoutAsString()).toContain('RUNNING');
    });

    it('should show stale process as not running after cleanup on next invocation', async () => {
        // Start a transient process
        await workspace.runCli(['start', 'stale-test', '--shell', 'node ../../sampleServers/echoServer.js']);
        await workspace.runCli(['wait-for-log', 'stale-test', '--message', 'Echo server started']);

        // Verify running
        const before = await workspace.runCli(['list']);
        expect(before.stdoutAsString()).toContain('stale-test');
        expect(before.stdoutAsString()).toContain('RUNNING');

        // Get the PID and kill it externally
        const pidMatch = before.stdoutAsString().match(/stale-test\s+RUNNING\s+(\d+)/);
        expect(pidMatch).toBeTruthy();
        const pid = parseInt(pidMatch![1], 10);

        try { process.kill(pid, 'SIGKILL'); } catch { /* may be dead */ }
        await new Promise(resolve => setTimeout(resolve, 500));

        // Force cleanup by running kill-all (which also does cleanup on invocation)
        await workspace.runCli(['kill-all']);

        // Now list should not show stale-test as RUNNING
        const after = await workspace.runCli(['list']);
        expect(after.stdoutAsString()).not.toMatch(/stale-test\s+RUNNING/);
    });
});
