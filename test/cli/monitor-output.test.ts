import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-monitor-output');

describe('Monitor output capture', () => {
    beforeAll(() => {
        fs.writeFileSync(
            path.join(workspace.dbDir, '.candle.json'),
            JSON.stringify(
                {
                    services: [
                        {
                            name: 'burst',
                            // Prints a burst and exits non-zero right away, so its
                            // last lines race the exit event inside the monitor.
                            shell: 'i=0; while [ $i -lt 600 ]; do echo line$i; i=$((i+1)); done; echo final-error-line >&2; exit 3',
                        },
                        {
                            name: 'leaky',
                            // The shell exits at once but leaves a background
                            // child running: the service is that child now.
                            shell: 'sleep 60 & echo leaky-done; exit 0',
                        },
                        {
                            name: 'brief',
                            // The same, with a child that ends by itself.
                            shell: 'sleep 2 & echo brief-done; exit 0',
                        },
                        {
                            name: 'leaky-fail',
                            // A failed start must not leave its child behind.
                            shell: 'sleep 6161 & echo leaky-fail-done; exit 3',
                        },
                    ],
                },
                null,
                2
            ) + '\n'
        );
    });

    afterAll(() => workspace.cleanup());

    it('keeps the last output of a process that exits right after writing it', async () => {
        await workspace.runCli(['start', 'burst'], { ignoreExitCode: true });

        const out = (await workspace.runCli(['logs', 'burst', '--count', '1000'])).stdoutAsString();
        expect(out).toContain('line599');
        expect(out).toContain('final-error-line');
    });

    it('keeps a service running while a background child outlives its shell', async () => {
        await workspace.runCli(['start', 'leaky']);
        await workspace.runCli(['wait-for-log', 'leaky', '--message', 'leaky-done']);
        // Give the shell time to exit; the child is all that is left.
        await new Promise((r) => setTimeout(r, 1000));

        const ps = (await workspace.runCli(['ps', 'leaky'])).stdoutAsString();
        expect(ps).toContain('RUNNING');
        const logs = (await workspace.runCli(['logs', 'leaky'])).stdoutAsString();
        expect(logs).not.toContain('exited');

        // `kill` reaches the child even though the shell's PID is gone.
        const started = Date.now();
        const kill = (await workspace.runCli(['kill', 'leaky'])).stdoutAsString();
        expect(kill).toContain("Killed 'leaky'");
        expect(Date.now() - started).toBeLessThan(4000);

        let out = '';
        while (Date.now() - started < 5000) {
            out = (await workspace.runCli(['logs', 'leaky'])).stdoutAsString();
            if (out.includes('Process was stopped')) break;
            await new Promise((r) => setTimeout(r, 100));
        }
        expect(out).toContain('Process was stopped');
        expect((await workspace.runCli(['ps', 'leaky'])).stdoutAsString()).not.toContain('RUNNING');
    });

    it('records the exit once the background child ends', async () => {
        const started = Date.now();
        await workspace.runCli(['start', 'brief']);
        let out = '';
        while (Date.now() - started < 8000) {
            out = (await workspace.runCli(['logs', 'brief'])).stdoutAsString();
            if (out.includes('exited')) break;
            await new Promise((r) => setTimeout(r, 100));
        }
        expect(out).toContain('brief-done');
        expect(out).toContain('Process exited with code 0');
        // Not before the child's two seconds were up.
        expect(Date.now() - started).toBeGreaterThan(1500);
    });

    it('stops a background child when the start fails', async () => {
        const result = await workspace.runCli(['start', 'leaky-fail'], { ignoreExitCode: true });
        expect(result.stderrAsString()).toContain('failed to start');
        expect((await workspace.runCli(['list-all'])).stdoutAsString()).not.toContain('leaky-fail');

        // The monitor exits once nothing of the service is left; it would
        // otherwise live as long as the child.
        const { execSync } = await import('child_process');
        const leftover = () =>
            execSync('ps -ax -o command').toString().split('\n').includes('sleep 6161');
        const started = Date.now();
        while (leftover() && Date.now() - started < 8000) {
            await new Promise((r) => setTimeout(r, 100));
        }
        expect(leftover()).toBe(false);
    });

    // Python block-buffers stdout when it's a pipe, which would hide a
    // service's output from `logs` and `wait-for-log`.
    it('runs services with PYTHONUNBUFFERED=1 by default', async () => {
        await workspace.runCli(['start', 'unbuf-default', '--shell', 'echo "unbuffered=[$PYTHONUNBUFFERED]"; sleep 30']);
        await workspace.runCli(['wait-for-log', 'unbuf-default', '--message', 'unbuffered=[1]', '--timeout', '5']);
    });

    it('keeps a PYTHONUNBUFFERED the caller already set, even an empty one', async () => {
        await workspace.runCli(['start', 'unbuf-empty', '--shell', 'echo "unbuffered=[$PYTHONUNBUFFERED]"; sleep 30'], {
            env: { PYTHONUNBUFFERED: '' },
        });
        await workspace.runCli(['wait-for-log', 'unbuf-empty', '--message', 'unbuffered=[]', '--timeout', '5']);
    });
});
