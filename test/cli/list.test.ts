import { describe, it, expect, afterAll } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import * as path from 'path';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-list');

// The shell strings configured in test/workspaces/cli-list/.candle.json
const ECHO_SHELL = 'node ../../sampleServers/echoServer.js';
const WEB_SHELL = 'node ../../sampleServers/testProcess.js';

/** The multiline detail entry for a service, i.e. its header line plus the two indented lines. */
function entryFor(output: string, serviceName: string): string[] {
    const lines = output.split('\n');
    const start = lines.findIndex(line => line.startsWith(`${serviceName}  `));
    expect(start, `no entry for '${serviceName}' in:\n${output}`).toBeGreaterThanOrEqual(0);
    return lines.slice(start, start + 3);
}

describe('CLI List Command', () => {
    afterAll(() => workspace.cleanup());

    describe('basic list functionality', () => {
        it('should list every configured service with its details', async () => {
            const result = await workspace.runCli(['list']);
            const output = result.stdoutAsString();

            for (const name of ['web', 'echo', 'echo-test']) {
                expect(output).toContain(name);
            }
            expect(output).toContain('command:');
            expect(output).toContain('directory:');
        });

        it('should show running process in list', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const [header, command, directory] = entryFor(
                (await workspace.runCli(['list'])).stdoutAsString(),
                'echo'
            );

            expect(header).toMatch(/^echo {2}RUNNING {2}pid \d+ {2}uptime \S/);
            expect(command).toBe(`  command:   ${ECHO_SHELL}`);
            expect(directory).toBe(`  directory: ${workspace.dbDir}`);
        });

        it('should show multiple running processes', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['start', 'echo-test']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['wait-for-log', 'echo-test', '--message', 'Echo server started']);

            const result = await workspace.runCli(['list']);

            expect(entryFor(result.stdoutAsString(), 'echo')[0]).toContain('RUNNING');
            expect(entryFor(result.stdoutAsString(), 'echo-test')[0]).toContain('RUNNING');
        });

        it('should show uptime for running processes', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['list']);

            expect(entryFor(result.stdoutAsString(), 'echo')[0]).toMatch(/uptime (\d+[dhms] ?)+/);
        });

        it('should omit pid and uptime for services that are not running', async () => {
            await workspace.runCli(['kill'], { ignoreExitCode: true });

            const [header, command] = entryFor(
                (await workspace.runCli(['list'])).stdoutAsString(),
                'web'
            );

            expect(header).toBe('web  not running');
            expect(header).not.toContain('pid');
            expect(header).not.toContain('uptime');
            expect(command).toBe(`  command:   ${WEB_SHELL}`);
        });
    });

    describe('command field shows the shell string', () => {
        it('should show the configured shell, not the service name, when not running', async () => {
            await workspace.runCli(['kill'], { ignoreExitCode: true });

            const output = (await workspace.runCli(['list'])).stdoutAsString();

            expect(output).toContain(`command:   ${ECHO_SHELL}`);
            expect(output).not.toContain('command:   echo');
        });

        it('should show the launched shell for a running process', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const [, command] = entryFor(
                (await workspace.runCli(['list'])).stdoutAsString(),
                'echo'
            );

            expect(command).toBe(`  command:   ${ECHO_SHELL}`);
        });

        it('should show the transient shell for a process started with --shell', async () => {
            await workspace.runCli(['start', 'echo', '--shell', WEB_SHELL]);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Test server started']);

            const [, command] = entryFor(
                (await workspace.runCli(['list'])).stdoutAsString(),
                'echo'
            );

            expect(command).toBe(`  command:   ${WEB_SHELL}`);
        });

        it('should expose the shell string as the command in --json', async () => {
            const result = await workspace.runCli(['list', '--json']);
            const processes = JSON.parse(result.stdoutAsString());

            const echoTest = processes.find((p: any) => p.serviceName === 'echo-test');
            expect(echoTest.command).toBe(ECHO_SHELL);
            expect(echoTest.command).not.toBe('echo-test');
        });
    });

    describe('service name filter', () => {
        it('should list only the named service', async () => {
            const result = await workspace.runCli(['list', 'echo-test']);
            const output = result.stdoutAsString();

            expect(output).toContain('echo-test');
            expect(output).not.toContain('web');
            // 'echo' only appears as part of 'echo-test'
            expect(output.split('\n').filter(line => line.startsWith('echo  ')).length).toBe(0);
        });

        it('should accept multiple names', async () => {
            const result = await workspace.runCli(['list', 'web', 'echo']);
            const output = result.stdoutAsString();

            expect(output).toContain('web');
            expect(output).toContain('echo');
            expect(output).not.toContain('echo-test');
        });

        it('should filter --json output the same way', async () => {
            const result = await workspace.runCli(['list', 'web', '--json']);
            const processes = JSON.parse(result.stdoutAsString());

            expect(processes.length).toBe(1);
            expect(processes[0].serviceName).toBe('web');
        });

        it('should error for an unknown service name', async () => {
            const result = await workspace.runCli(['list', 'no-such-service'], {
                ignoreExitCode: true,
            });

            expect(result.failed()).toBe(true);
            const output = result.stdoutAsString() + result.stderrAsString();
            expect(output).toContain('no-such-service');
        });
    });

    describe('ls alias', () => {
        it('should work with ls alias', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const listResult = await workspace.runCli(['list']);
            const lsResult = await workspace.runCli(['ls']);

            expect(lsResult.stdoutAsString()).toContain('command:');
            expect(listResult.stdoutAsString()).toContain('echo');
            expect(lsResult.stdoutAsString()).toContain('echo');
        });
    });

    describe('list output format', () => {
        it('should use the multiline detail view, not a table', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const output = (await workspace.runCli(['list'])).stdoutAsString();

            // The table headers belong to 'candle ps' now.
            expect(output).not.toContain('NAME');
            expect(output).not.toContain('STATUS');
            expect(output).not.toContain('UPTIME');

            // Each entry is a header line plus two indented detail lines.
            const lines = output.split('\n').filter(line => line.length > 0);
            expect(lines.length % 3).toBe(0);
            for (let i = 0; i < lines.length; i += 3) {
                expect(lines[i]).not.toMatch(/^ /);
                expect(lines[i + 1]).toMatch(/^ {2}command: {3}/);
                expect(lines[i + 2]).toMatch(/^ {2}directory: /);
            }
        });

        it('should not truncate long command strings', async () => {
            const longShell = `node ../../sampleServers/testProcess.js --a-very-long-argument-that-would-be-truncated-in-a-table-view`;
            await workspace.runCli(['start', 'long-command', '--shell', longShell]);

            const [, command] = entryFor(
                (await workspace.runCli(['list'])).stdoutAsString(),
                'long-command'
            );

            expect(command).toBe(`  command:   ${longShell}`);
            expect(command).not.toContain('...');
        });

        it('should show config changed warning for transient overrides', async () => {
            // Start 'echo' as transient with different shell
            await workspace.runCli(['start', 'echo', '--shell', WEB_SHELL]);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Test server started']);

            const [header] = entryFor(
                (await workspace.runCli(['list'])).stdoutAsString(),
                'echo'
            );

            expect(header).toContain('[config changed]');
            expect(header).toMatch(/^echo {2}RUNNING \[config changed\] {2}pid \d+/);
        });
    });

    describe('edge cases', () => {
        it('should handle list when database is empty', async () => {
            const freshWorkspace = new TestWorkspace('cli-list-fresh');

            await freshWorkspace.runCli(['list']);
        });

        it('should not show stale process entry as RUNNING after reboot', async () => {
            // Use a fresh workspace to avoid interference from other tests
            const staleWorkspace = new TestWorkspace('cli-list-stale');

            // Simulate a post-reboot scenario: insert a DB entry with a PID that
            // doesn't exist (as would happen after a reboot kills all processes).
            // First, run any command to initialize the database.
            await staleWorkspace.runCli(['list-all']);

            const dbPath = path.join(staleWorkspace.dbDir, 'candle.db');
            const db = new DatabaseSync(dbPath);
            const fakePid = 2147483000; // PID that almost certainly doesn't exist
            db.exec(`insert into processes (command_name, project_dir, pid, log_collector_pid, start_time, shell)
                      values ('echo', '${staleWorkspace.dbDir}', ${fakePid}, ${fakePid + 1}, strftime('%s','now'), 'node test.js')`);
            db.close();

            // candle ls --all should detect the dead PIDs and NOT show the stale entry
            const result = await staleWorkspace.runCli(['list-all']);
            const staleRunning = result.stdoutAsString().split('\n').find(
                line => line.includes('echo') && line.includes('RUNNING')
            );
            expect(staleRunning).toBeUndefined();
        });

        it('should not show killed process as RUNNING', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Verify it's running
            expect(
                entryFor((await workspace.runCli(['list'])).stdoutAsString(), 'echo')[0]
            ).toContain('RUNNING');

            // Kill it
            await workspace.runCli(['kill', 'echo']);

            // After kill, 'echo' specifically should not show as RUNNING
            expect(
                entryFor((await workspace.runCli(['list'])).stdoutAsString(), 'echo')[0]
            ).toContain('not running');
        });
    });
});
