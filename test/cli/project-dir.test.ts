import { describe, it, expect, afterAll, beforeAll } from 'vitest';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-project-dir');

/**
 * A throwaway project directory outside the workspace, so tests can delete it
 * (or its config) without disturbing the workspace's own .candle.json.
 *
 * Every command runs with the workspace as its cwd, so nothing here depends on
 * the CLI being invoked from inside the project — which is the point of
 * --project-dir.
 */
function makeProject(name: string, services: unknown[]): string {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), `candle-${name}-`));
    // macOS puts temp dirs under a /var -> /private/var symlink; Candle records
    // the resolved path, so resolve here too or the --project-dir string will
    // not match the stored rows.
    const resolved = fs.realpathSync(dir);
    fs.writeFileSync(path.join(resolved, '.candle.json'), JSON.stringify({ services }, null, 2));
    return resolved;
}

const sleeper = (name: string) => ({ name, shell: 'while true; do echo tick; sleep 1; done' });

describe('CLI --project-dir', () => {
    let project: string;

    beforeAll(() => {
        project = makeProject('project-dir', [sleeper('ticker')]);
    });

    afterAll(async () => {
        await workspace.runCli(['kill', '--project-dir', project], { ignoreExitCode: true });
        fs.rmSync(project, { recursive: true, force: true });
        await workspace.cleanup();
    });

    describe('targeting another project', () => {
        it('starts, lists, logs and kills a service in the named project', async () => {
            await workspace.runCli(['start', '--project-dir', project, 'ticker']);

            const ps = await workspace.runCli(['ps', '--project-dir', project]);
            expect(ps.stdoutAsString()).toContain('ticker');
            expect(ps.stdoutAsString()).toContain('RUNNING');

            await workspace.runCli([
                'wait-for-log',
                '--project-dir',
                project,
                'ticker',
                '--message',
                'tick',
            ]);

            const logs = await workspace.runCli(['logs', '--project-dir', project, 'ticker']);
            expect(logs.stdoutAsString()).toContain('tick');

            const killed = await workspace.runCli(['kill', '--project-dir', project, 'ticker']);
            expect(killed.stdoutAsString()).toContain('Killed');
        });

        it('does not leak into the workspace project', async () => {
            await workspace.runCli(['start', '--project-dir', project, 'ticker']);

            // The workspace's own project defines 'echo', not 'ticker'.
            const ps = await workspace.runCli(['ps']);
            expect(ps.stdoutAsString()).not.toContain('ticker');

            await workspace.runCli(['kill', '--project-dir', project, 'ticker']);
        });

        it('accepts a relative path, resolved against the cwd', async () => {
            const relative = path.relative(workspace.dbDir, project);
            const ps = await workspace.runCli(['ps', '--project-dir', relative]);

            // Same project as the absolute form: its service is listed.
            expect(ps.stdoutAsString()).toContain('ticker');
        });
    });

    describe('projects that are gone', () => {
        it('kills processes from a deleted project directory', async () => {
            const doomed = makeProject('doomed', [sleeper('ghost')]);
            await workspace.runCli(['start', '--project-dir', doomed, 'ghost']);

            const pid = Number(
                /RUNNING\s+(\d+)/.exec(
                    (await workspace.runCli(['ps', '--project-dir', doomed])).stdoutAsString()
                )?.[1]
            );
            expect(pid).toBeGreaterThan(0);

            // The whole project disappears out from under the running process.
            fs.rmSync(doomed, { recursive: true, force: true });

            const killed = await workspace.runCli(['kill', '--project-dir', doomed, 'ghost']);
            expect(killed.stdoutAsString()).toContain('Killed');

            // And the process really is gone, not just the row.
            expect(() => process.kill(pid, 0)).toThrow();
        });

        it('kills every process in a deleted project when given no names', async () => {
            const doomed = makeProject('doomed-all', [sleeper('a'), sleeper('b')]);
            await workspace.runCli(['start', '--project-dir', doomed, 'a', 'b']);
            fs.rmSync(doomed, { recursive: true, force: true });

            const killed = await workspace.runCli(['kill', '--project-dir', doomed]);
            expect(killed.stdoutAsString()).toContain("Killed 'a'");
            expect(killed.stdoutAsString()).toContain("Killed 'b'");
        });

        it('kills processes whose config file was deleted', async () => {
            const stripped = makeProject('stripped', [sleeper('orphan')]);
            await workspace.runCli(['start', '--project-dir', stripped, 'orphan']);
            fs.rmSync(path.join(stripped, '.candle.json'));

            const killed = await workspace.runCli(['kill', '--project-dir', stripped, 'orphan']);
            expect(killed.stdoutAsString()).toContain('Killed');

            fs.rmSync(stripped, { recursive: true, force: true });
        });

        it('reports nothing running rather than failing on an unknown name', async () => {
            // No config to validate against, so an unknown name is not an error.
            const result = await workspace.runCli([
                'kill',
                '--project-dir',
                '/tmp/candle-does-not-exist',
                'whatever',
            ]);

            expect(result.failed()).toBe(false);
            expect(result.stdoutAsString()).toContain('No running processes');
        });
    });

    describe('commands that need the config', () => {
        it('refuses a project directory that has no config file', async () => {
            const bare = fs.mkdtempSync(path.join(os.tmpdir(), 'candle-bare-'));

            const result = await workspace.runCli(['start', '--project-dir', bare, 'echo'], {
                ignoreExitCode: true,
            });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain(bare);

            fs.rmSync(bare, { recursive: true, force: true });
        });

        it('does not fall back to an ancestor project', async () => {
            // A subdirectory of a real project is not itself a project. Resolving
            // services from the parent would key rows to one directory while
            // reading services from another.
            const child = path.join(project, 'subdir');
            fs.mkdirSync(child, { recursive: true });

            const result = await workspace.runCli(['ps', '--project-dir', child], {
                ignoreExitCode: true,
            });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain(
                `No .candle.json in ${child} (--project-dir doesn't search parent directories)`
            );
            expect(result.stderrAsString()).not.toContain('current directory');
        });
    });

    describe('unsupported commands', () => {
        it('rejects --project-dir on list-all, which is already system-wide', async () => {
            const result = await workspace.runCli(['list-all', '--project-dir', '/tmp'], {
                ignoreExitCode: true,
            });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('Unknown argument');
        });
    });
});
