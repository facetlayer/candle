import { afterAll, describe, expect, it } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-watch-restart');

afterAll(() => workspace.cleanup());

function countOccurrences(haystack: string, needle: string): number {
    return haystack.split(needle).length - 1;
}

describe('watch mode after a restart', () => {
    it('does not show logs from the previous instance', async () => {
        // The 'slow-stop' service shuts down slowly and noisily: on SIGTERM it prints a
        // marker line and only dies ~800ms later. That means the *previous* instance's
        // shutdown output and its process_exited row are both written to the log table
        // AFTER the new launch has been recorded, which is exactly the race that used
        // to leak stale lines into the watch output.
        await workspace.runCli(['start', 'slow-stop']);
        await workspace.runCli([
            'wait-for-log',
            'slow-stop',
            '--message',
            'Slow stop server started',
        ]);

        // `run` is an alias of `start`; --watch forces interactive watch mode.
        const result = await workspace.runCli([
            'run',
            'slow-stop',
            '--watch',
            '--exit-after-ms',
            '3000',
        ]);
        const output = result.stdoutAsString();

        // Sanity check: the new instance's own startup line is shown, exactly once.
        expect(countOccurrences(output, 'Slow stop server started')).toBe(1);

        // Nothing from the instance that was just killed may appear.
        expect(output).not.toContain('PREVIOUS-INSTANCE-SHUTDOWN-MARKER');
        expect(output).not.toContain('Process was stopped');
        expect(output).not.toContain('Process exited with code');
    }, 20000);
});
