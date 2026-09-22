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
                            // Exits at once but leaves a background child holding
                            // stdout open; the monitor must not wait on it forever.
                            shell: 'sleep 6 & echo leaky-done; exit 0',
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

    it('records the exit within a bounded time when a grandchild holds the pipe open', async () => {
        const started = Date.now();
        await workspace.runCli(['start', 'leaky'], { ignoreExitCode: true });
        let out = '';
        while (Date.now() - started < 5000) {
            out = (await workspace.runCli(['logs', 'leaky'])).stdoutAsString();
            if (out.includes('exited')) break;
            await new Promise((r) => setTimeout(r, 100));
        }
        expect(out).toContain('leaky-done');
        expect(out).toContain('exited');
        // Well before the background `sleep 6` would release the pipe.
        expect(Date.now() - started).toBeLessThan(5000);
    });
});
