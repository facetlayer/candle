import * as path from 'path';
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

// `ps` / `list` status and JSON schema, and the directory `list` / `list-all` report.
const workspace = new TestWorkspace('cli-service-status');

const TRANSIENT_SHELL = 'node ../../../sampleServers/testProcess.js';

interface ListRow {
    serviceName: string;
    command: string;
    workingDir: string;
    uptime: string;
    pid: number | null;
    status: string;
    configChanged: boolean;
    exitCode: number | null;
}

function rowFor(output: string, serviceName: string): string {
    const row = output.split('\n').find(line => line.startsWith(`${serviceName} `));
    expect(row, `no row for '${serviceName}' in:\n${output}`).toBeDefined();
    return row!;
}

async function listJson(args: string[]): Promise<ListRow[]> {
    const result = await workspace.runCli([...args, '--json']);
    return JSON.parse(result.stdoutAsString());
}

function jsonRowFor(rows: ListRow[], serviceName: string): ListRow {
    const row = rows.find(r => r.serviceName === serviceName);
    expect(row, `no JSON row for '${serviceName}'`).toBeDefined();
    return row!;
}

describe('service status and directories', () => {
    beforeAll(async () => {
        workspace.ensureSubdir('sub');
        // Both exit ~1s after starting, past the start grace period.
        await workspace.runCli(['start', 'crasher', 'clean-exit']);
        await workspace.runCli(['wait-for-log', 'crasher', '--message', 'exiting with code 3']);
        await workspace.runCli(['wait-for-log', 'clean-exit', '--message', 'exiting with code 0']);
        await new Promise(resolve => setTimeout(resolve, 500));
    });

    afterAll(() => workspace.cleanup());

    describe('crashed services', () => {
        it('ps shows EXITED (<code>) when the latest run exited non-zero', async () => {
            const result = await workspace.runCli(['ps']);
            expect(rowFor(result.stdoutAsString(), 'crasher')).toContain('EXITED (3)');
        });

        it('ps shows "not running" for a clean exit and for a never-started service', async () => {
            const output = (await workspace.runCli(['ps'])).stdoutAsString();
            expect(rowFor(output, 'clean-exit')).toContain('not running');
            expect(rowFor(output, 'idle')).toContain('not running');
        });

        it('list shows EXITED (<code>) too', async () => {
            const result = await workspace.runCli(['list', 'crasher']);
            expect(result.stdoutAsString().split('\n')[0]).toBe('crasher  EXITED (3)');
        });

        it('--json carries the exit code', async () => {
            const rows = await listJson(['ps']);
            const crasher = jsonRowFor(rows, 'crasher');
            expect(crasher.status).toBe('EXITED (3)');
            expect(crasher.exitCode).toBe(3);
            expect(jsonRowFor(rows, 'clean-exit').exitCode).toBeNull();
            expect(jsonRowFor(rows, 'idle').exitCode).toBeNull();
        });
    });

    describe('failed starts', () => {
        beforeAll(async () => {
            for (const name of ['missing-root', 'self-signal', 'start-exit']) {
                const result = await workspace.runCli(['start', name], { ignoreExitCode: true });
                expect(result.failed(), `start ${name} should fail`).toBe(true);
            }
        });

        it('ps shows FAILED for a start that failed without an exit code', async () => {
            const output = (await workspace.runCli(['ps'])).stdoutAsString();
            expect(rowFor(output, 'missing-root')).toContain('FAILED');
            expect(rowFor(output, 'self-signal')).toContain('FAILED');
        });

        it('a start that exited non-zero keeps EXITED (<code>)', async () => {
            const output = (await workspace.runCli(['ps'])).stdoutAsString();
            expect(rowFor(output, 'start-exit')).toContain('EXITED (4)');
        });

        it('list shows FAILED too', async () => {
            const result = await workspace.runCli(['list', 'missing-root']);
            expect(result.stdoutAsString().split('\n')[0]).toBe('missing-root  FAILED');
        });

        it('--json has status FAILED and a null exitCode', async () => {
            const rows = await listJson(['ps']);
            for (const name of ['missing-root', 'self-signal']) {
                const row = jsonRowFor(rows, name);
                expect(row.status).toBe('FAILED');
                expect(row.exitCode).toBeNull();
                expect(row.pid).toBeNull();
            }
            expect(jsonRowFor(rows, 'start-exit').exitCode).toBe(4);
        });

        it('logs explains why the start failed', async () => {
            const result = await workspace.runCli(['logs', 'missing-root']);
            expect(result.stdoutAsString()).toContain('root directory does not exist');
        });

        it('a deliberate kill, even during startup, is not FAILED', async () => {
            const starting = workspace.runCli(['start', 'killed-early'], { ignoreExitCode: true });
            // Kill as soon as the monitor has registered the process.
            for (let i = 0; i < 100; i++) {
                const row = jsonRowFor(await listJson(['ps']), 'killed-early');
                if (row.pid !== null) break;
                await new Promise(resolve => setTimeout(resolve, 20));
            }
            await workspace.runCli(['kill', 'killed-early'], { ignoreExitCode: true });
            await starting;
            await new Promise(resolve => setTimeout(resolve, 300));

            const row = jsonRowFor(await listJson(['ps']), 'killed-early');
            expect(row.status).toBe('not running');
            expect(row.exitCode).toBeNull();
        });
    });

    describe('list --json schema', () => {
        it('a stopped service has pid null and every key present', async () => {
            const idle = jsonRowFor(await listJson(['list']), 'idle');
            expect(idle.pid).toBeNull();
            expect(idle.configChanged).toBe(false);
            expect(Object.keys(idle).sort()).toEqual(
                ['command', 'configChanged', 'exitCode', 'pid', 'serviceName', 'status', 'uptime', 'workingDir'],
            );
        });

        it('a running service has the same keys', async () => {
            await workspace.runCli(['start', 'idle']);
            try {
                const idle = jsonRowFor(await listJson(['list']), 'idle');
                expect(idle.status).toBe('RUNNING');
                expect(typeof idle.pid).toBe('number');
                expect(idle.configChanged).toBe(false);
                expect(idle.exitCode).toBeNull();
                expect(Object.keys(idle).sort()).toEqual(
                    ['command', 'configChanged', 'exitCode', 'pid', 'serviceName', 'status', 'uptime', 'workingDir'],
                );
            } finally {
                await workspace.runCli(['kill', 'idle']);
            }
        });
    });

    describe('directory reporting', () => {
        const subDir = path.join(workspace.dbDir, 'sub');

        beforeAll(async () => {
            await workspace.runCli(['start', 'rooted']);
            await workspace.runCli(['start', 'tr', '--shell', TRANSIENT_SHELL, '--root', 'sub']);
        });

        afterAll(async () => {
            await workspace.runCli(['kill', 'rooted', 'tr'], { ignoreExitCode: true });
        });

        it('list shows a configured root service in its root directory', async () => {
            expect(jsonRowFor(await listJson(['list']), 'rooted').workingDir).toBe(subDir);
        });

        it('list shows a transient service started with --root in that directory', async () => {
            expect(jsonRowFor(await listJson(['list']), 'tr').workingDir).toBe(subDir);
            const text = (await workspace.runCli(['list', 'tr'])).stdoutAsString();
            expect(text).toContain(`directory: ${subDir}`);
        });

        it('list-all agrees with list, in JSON and in the DIRECTORY column', async () => {
            const rows = (await listJson(['list-all'])).filter(r => r.workingDir.startsWith(workspace.dbDir));
            expect(jsonRowFor(rows, 'rooted').workingDir).toBe(subDir);
            expect(jsonRowFor(rows, 'tr').workingDir).toBe(subDir);

            const table = (await workspace.runCli(['list-all'])).stdoutAsString();
            const rootedRow = table.split('\n').find(line => line.startsWith('rooted ') && line.includes(workspace.dbDir));
            expect(rootedRow).toBeDefined();
            expect(rootedRow!.trimEnd().endsWith(subDir)).toBe(true);
        });
    });
});
