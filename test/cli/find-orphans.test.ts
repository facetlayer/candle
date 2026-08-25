import { describe, it, expect, afterEach, afterAll } from 'vitest';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-find-orphans');

const sleeper = (name: string) => ({ name, shell: 'while true; do echo tick; sleep 1; done' });

/** A throwaway project outside the workspace, so tests can dismantle it. */
function makeProject(services: unknown[]): string {
    const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'candle-orphan-')));
    fs.writeFileSync(path.join(dir, '.candle.json'), JSON.stringify({ services }, null, 2));
    return dir;
}

/** Projects to tear down after each test, however it ended. */
let started: string[] = [];

async function startProject(services: { name: string; shell: string }[]): Promise<string> {
    const dir = makeProject(services);
    started.push(dir);
    await workspace.runCli(['start', '--project-dir', dir, ...services.map((s) => s.name)]);
    return dir;
}

describe('CLI Find-Orphans Command', () => {
    afterEach(async () => {
        for (const dir of started) {
            await workspace.runCli(['kill', '--project-dir', dir], { ignoreExitCode: true });
            fs.rmSync(dir, { recursive: true, force: true });
        }
        started = [];
    });

    afterAll(() => workspace.cleanup());

    describe('with nothing orphaned', () => {
        it('reports that there are no orphans', async () => {
            const result = await workspace.runCli(['find-orphans']);
            expect(result.stdoutAsString()).toContain('No orphaned processes found');
        });

        it('does not report a healthy running process', async () => {
            await startProject([sleeper('healthy')]);

            const result = await workspace.runCli(['find-orphans']);
            expect(result.stdoutAsString()).not.toContain('healthy');
            expect(result.stdoutAsString()).toContain('No orphaned processes found');
        });
    });

    describe('detecting orphans', () => {
        it('reports a process whose project directory was deleted', async () => {
            const dir = await startProject([sleeper('gone-dir')]);
            fs.rmSync(dir, { recursive: true, force: true });

            const result = await workspace.runCli(['find-orphans']);
            const out = result.stdoutAsString();

            expect(out).toContain('gone-dir');
            expect(out).toContain(dir);
            expect(out).toContain('project directory no longer exists');
        });

        it('reports a process whose config file was deleted', async () => {
            const dir = await startProject([sleeper('gone-config')]);
            fs.rmSync(path.join(dir, '.candle.json'));

            const result = await workspace.runCli(['find-orphans']);
            const out = result.stdoutAsString();

            expect(out).toContain('gone-config');
            expect(out).toContain('no candle config file in project directory');
        });

        it('reports a process whose service was removed from the config', async () => {
            const dir = await startProject([sleeper('dropped')]);
            fs.writeFileSync(path.join(dir, '.candle.json'), JSON.stringify({ services: [] }));

            const result = await workspace.runCli(['find-orphans']);
            const out = result.stdoutAsString();

            expect(out).toContain('dropped');
            expect(out).toContain('service is no longer listed in the config file');
        });

        it('reports only the orphaned service when a sibling is still configured', async () => {
            const dir = await startProject([sleeper('kept'), sleeper('removed')]);
            fs.writeFileSync(
                path.join(dir, '.candle.json'),
                JSON.stringify({ services: [sleeper('kept')] })
            );

            const out = (await workspace.runCli(['find-orphans'])).stdoutAsString();

            expect(out).toContain('removed');
            expect(out).toContain('Found 1 orphaned process:');
            // 'kept' is still in the config, so it is not an orphan.
            expect(out).not.toContain('kept (PID');
        });

        it('suggests the cleanup command', async () => {
            const dir = await startProject([sleeper('needs-cleanup')]);
            fs.rmSync(dir, { recursive: true, force: true });

            const out = (await workspace.runCli(['find-orphans'])).stdoutAsString();
            expect(out).toContain('candle kill --project-dir');
        });
    });

    describe('--json output', () => {
        it('emits structured orphan records', async () => {
            const dir = await startProject([sleeper('json-orphan')]);
            fs.rmSync(dir, { recursive: true, force: true });

            const result = await workspace.runCli(['find-orphans', '--json']);
            const parsed = JSON.parse(result.stdoutAsString());

            const orphan = parsed.orphans.find((o: any) => o.serviceName === 'json-orphan');
            expect(orphan).toBeDefined();
            expect(orphan.projectDir).toBe(dir);
            expect(orphan.reason).toBe('missingProjectDir');
            expect(orphan.pid).toBeGreaterThan(0);
        });

        it('emits an empty list when nothing is orphaned', async () => {
            const result = await workspace.runCli(['find-orphans', '--json']);
            expect(JSON.parse(result.stdoutAsString()).orphans).toEqual([]);
        });
    });

    describe('cleaning up what it finds', () => {
        it('the reported process can be killed with the suggested command', async () => {
            const dir = await startProject([sleeper('cleanup-me')]);
            fs.rmSync(dir, { recursive: true, force: true });

            const found = JSON.parse(
                (await workspace.runCli(['find-orphans', '--json'])).stdoutAsString()
            ).orphans.find((o: any) => o.serviceName === 'cleanup-me');

            await workspace.runCli([
                'kill',
                '--project-dir',
                found.projectDir,
                found.serviceName,
            ]);

            // The process is gone, and so is the orphan report.
            expect(() => process.kill(found.pid, 0)).toThrow();
            const after = (await workspace.runCli(['find-orphans'])).stdoutAsString();
            expect(after).not.toContain('cleanup-me');
        });
    });

    describe('command surface', () => {
        it('is a recognized command', async () => {
            const result = await workspace.runCli(['find-orphans']);
            expect(result.stderrAsString()).not.toContain('Unrecognized command');
        });

        it('shows help', async () => {
            const result = await workspace.runCli(['find-orphans', '--help']);
            expect(result.stdoutAsString()).toContain('find-orphans');
            expect(result.stdoutAsString()).toContain('--project-dir');
        });

        it('is listed in the grouped help', async () => {
            const result = await workspace.runCli(['help']);
            expect(result.stdoutAsString()).toContain('find-orphans');
        });
    });
});
