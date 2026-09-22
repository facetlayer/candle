import { describe, it, expect, afterAll } from 'vitest';
import { execFileSync } from 'node:child_process';
import { TestWorkspace } from './utils';

// Guarantees a process manager has to keep: kill really kills, one start
// means one instance, erase-database won't strand running processes, and
// `logs --count N` prints N lines.
const workspace = new TestWorkspace('cli-process-safety');

function isAlive(pid: number): boolean {
    try {
        process.kill(pid, 0);
        return true;
    } catch {
        return false;
    }
}

async function pidOf(service: string): Promise<number> {
    const result = await workspace.runCli(['ps', service, '--json']);
    const [entry] = JSON.parse(result.stdoutAsString());
    return entry.pid;
}

describe('process safety', () => {
    afterAll(() => workspace.cleanup());

    describe('kill', () => {
        it('escalates to SIGKILL when a service ignores SIGTERM', async () => {
            await workspace.runCli(['start', 'stubborn']);
            await workspace.runCli(['wait-for-log', 'stubborn', '--message', 'ready']);
            const pid = await pidOf('stubborn');
            expect(pid).toBeGreaterThan(0);

            const result = await workspace.runCli(['kill', 'stubborn']);

            expect(result.stderrAsString()).toContain('sent SIGKILL');
            expect(result.stdoutAsString()).toContain("Killed 'stubborn'");
            expect(isAlive(pid)).toBe(false);
        }, 20000);

        it('SIGKILLs a child that ignores SIGTERM after its parent exits', async () => {
            await workspace.runCli(['start', 'stubborn-child']);
            await workspace.runCli(['wait-for-log', 'stubborn-child', '--message', 'ready']);
            const pid = await pidOf('stubborn-child');
            const childPid = Number(execFileSync('pgrep', ['-P', String(pid)]).toString().trim().split('\n')[0]);
            expect(childPid).toBeGreaterThan(0);

            const result = await workspace.runCli(['kill', 'stubborn-child']);

            expect(result.stderrAsString()).toContain('sent SIGKILL');
            expect(isAlive(pid)).toBe(false);
            expect(isAlive(childPid)).toBe(false);
        }, 20000);
    });

    describe('concurrent start', () => {
        it('leaves exactly one instance when started five times at once', async () => {
            await Promise.all(
                Array.from({ length: 5 }, () => workspace.runCli(['start', 'slow'])),
            );

            const list = await workspace.runCli(['list-all', '--json']);
            const running = JSON.parse(list.stdoutAsString()).filter(
                (e: any) => e.serviceName === 'slow' && e.status === 'RUNNING',
            );
            expect(running).toHaveLength(1);

            await workspace.runCli(['kill', 'slow']);
        }, 60000);

        it('check-start launches once when raced', async () => {
            const results = await Promise.all(
                Array.from({ length: 5 }, () => workspace.runCli(['check-start', 'slow'])),
            );
            const launched = results.filter((r) => r.stdoutAsString().includes("[Started process 'slow']"));
            expect(launched).toHaveLength(1);

            await workspace.runCli(['kill', 'slow']);
        }, 60000);
    });

    describe('logs --count', () => {
        it('prints exactly N lines and says when it cut some off', async () => {
            await workspace.runCli(['start', 'counter']);
            await workspace.runCli(['wait-for-log', 'counter', '--message', 'line 10']);

            const three = await workspace.runCli(['logs', 'counter', '--count', '3']);
            expect(three.stdoutAsString().split('\n').filter(Boolean)).toEqual([
                '-- showing the last 3 lines; use --count to see more --',
                'line 8',
                'line 9',
                'line 10',
            ]);

            const all = await workspace.runCli(['logs', 'counter', '--count', '10']);
            const lines = all.stdoutAsString().split('\n').filter(Boolean);
            expect(lines).toHaveLength(10);
            expect(all.stdoutAsString()).not.toContain('use --count');

            await workspace.runCli(['kill', 'counter']);
        }, 20000);
    });

    describe('erase-database', () => {
        it('refuses while a service is running', async () => {
            await workspace.runCli(['start', 'slow']);

            const result = await workspace.runCli(['erase-database'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('Refusing to erase the database');
            expect(result.stderrAsString()).toContain('slow (pid');
            expect(result.stderrAsString()).toContain('--force');

            const ps = await workspace.runCli(['ps', 'slow']);
            expect(ps.stdoutAsString()).toContain('RUNNING');
        });

        it('erases once services are stopped', async () => {
            await workspace.runCli(['kill', 'slow']);
            const result = await workspace.runCli(['erase-database']);
            expect(result.stdoutAsString()).toContain('Database erased');
        });

        it('--force erases anyway', async () => {
            await workspace.runCli(['start', 'slow']);
            const pid = await pidOf('slow');

            try {
                const result = await workspace.runCli(['erase-database', '--force']);
                expect(result.stdoutAsString()).toContain('Database erased');
                expect(isAlive(pid)).toBe(true);
            } finally {
                // --force orphans it by design; clean up by hand. Its monitor
                // exits once the shell does.
                process.kill(pid, 'SIGKILL');
            }
        });
    });
});
