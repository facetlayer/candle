import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-ps');

const ECHO_SHELL = 'node ../../sampleServers/echoServer.js';

/** The header row of the ps table. */
function headerLine(output: string): string {
    return output.split('\n')[0];
}

/** The table row for a service. */
function rowFor(output: string, serviceName: string): string {
    const row = output.split('\n').find(line => line.startsWith(`${serviceName} `));
    expect(row, `no row for '${serviceName}' in:\n${output}`).toBeDefined();
    return row!;
}

describe('CLI Ps Command', () => {
    afterAll(() => workspace.cleanup());

    describe('table output', () => {
        it('should print NAME/STATUS/PID/UPTIME columns in order', async () => {
            const result = await workspace.runCli(['ps']);
            const header = headerLine(result.stdoutAsString());

            expect(header).toContain('NAME');
            expect(header).toContain('STATUS');
            expect(header).toContain('PID');
            expect(header).toContain('UPTIME');

            expect(header.indexOf('NAME')).toBeLessThan(header.indexOf('STATUS'));
            expect(header.indexOf('STATUS')).toBeLessThan(header.indexOf('PID'));
            expect(header.indexOf('PID')).toBeLessThan(header.indexOf('UPTIME'));
        });

        it('should have a dashed separator row under the header', async () => {
            const result = await workspace.runCli(['ps']);

            expect(result.stdoutAsString().split('\n')[1]).toMatch(/^-+( +-+)+$/);
        });

        it('should omit the COMMAND and DIRECTORY columns', async () => {
            const result = await workspace.runCli(['ps']);
            const output = result.stdoutAsString();

            expect(output).not.toContain('COMMAND');
            expect(output).not.toContain('DIRECTORY');
            // The shell string and the project directory are not shown either.
            expect(output).not.toContain(ECHO_SHELL);
            expect(output).not.toContain(workspace.dbDir);
        });

        it('should show running processes with a pid and uptime', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const row = rowFor((await workspace.runCli(['ps'])).stdoutAsString(), 'echo');

            expect(row).toMatch(/^echo +RUNNING +\d+ +\d+[dhms]/);
        });

        it('should show a dash for pid and uptime when not running', async () => {
            await workspace.runCli(['kill'], { ignoreExitCode: true });

            const row = rowFor((await workspace.runCli(['ps'])).stdoutAsString(), 'web');

            expect(row).toMatch(/^web +not running +- +-/);
        });

        it('should mark drifted processes as [config changed]', async () => {
            await workspace.runCli([
                'start',
                'echo',
                '--shell',
                'node ../../sampleServers/testProcess.js',
            ]);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Test server started']);

            const row = rowFor((await workspace.runCli(['ps'])).stdoutAsString(), 'echo');

            expect(row).toContain('[config changed]');
            await workspace.runCli(['kill'], { ignoreExitCode: true });
        });
    });

    describe('status alias', () => {
        it('should show the same table as ps', async () => {
            const psResult = await workspace.runCli(['ps']);
            const statusResult = await workspace.runCli(['status']);

            expect(headerLine(statusResult.stdoutAsString())).toBe(
                headerLine(psResult.stdoutAsString())
            );
            expect(statusResult.stdoutAsString()).toContain('NAME');
            expect(statusResult.stdoutAsString()).not.toContain('COMMAND');
            expect(statusResult.stdoutAsString()).not.toContain('DIRECTORY');
        });

        it('should not print the multiline detail view that list uses', async () => {
            const result = await workspace.runCli(['status']);

            expect(result.stdoutAsString()).not.toContain('command:');
            expect(result.stdoutAsString()).not.toContain('directory:');
        });
    });

    describe('--json', () => {
        it('should emit the same JSON array as list --json', async () => {
            const psResult = await workspace.runCli(['ps', '--json']);
            const listResult = await workspace.runCli(['list', '--json']);

            const psProcesses = JSON.parse(psResult.stdoutAsString());
            const listProcesses = JSON.parse(listResult.stdoutAsString());

            expect(Array.isArray(psProcesses)).toBe(true);
            expect(psProcesses.map((p: any) => p.serviceName)).toEqual(
                listProcesses.map((p: any) => p.serviceName)
            );
            expect(psProcesses.map((p: any) => p.command)).toEqual(
                listProcesses.map((p: any) => p.command)
            );
        });

        it('should include the shell string as the command', async () => {
            const result = await workspace.runCli(['ps', '--json']);
            const processes = JSON.parse(result.stdoutAsString());

            const echo = processes.find((p: any) => p.serviceName === 'echo');
            expect(echo.command).toBe(ECHO_SHELL);
        });
    });

    describe('service name filter', () => {
        it('should show only the named service', async () => {
            const result = await workspace.runCli(['ps', 'echo-test']);
            const output = result.stdoutAsString();

            expect(output).toContain('echo-test');
            expect(output).not.toContain('web');
            expect(output.split('\n').filter(line => line.startsWith('echo ')).length).toBe(0);
        });

        it('should accept multiple names', async () => {
            const result = await workspace.runCli(['ps', 'web', 'echo']);
            const output = result.stdoutAsString();

            expect(output).toContain('web');
            expect(output).toContain('echo');
            expect(output).not.toContain('echo-test');
        });

        it('should filter --json output the same way', async () => {
            const result = await workspace.runCli(['status', 'web', '--json']);
            const processes = JSON.parse(result.stdoutAsString());

            expect(processes.length).toBe(1);
            expect(processes[0].serviceName).toBe('web');
        });

        it('should error for an unknown service name', async () => {
            const result = await workspace.runCli(['ps', 'no-such-service'], {
                ignoreExitCode: true,
            });

            expect(result.failed()).toBe(true);
            const output = result.stdoutAsString() + result.stderrAsString();
            expect(output).toContain('no-such-service');
        });
    });
});
