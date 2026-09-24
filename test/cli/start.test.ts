import { describe, it, expect, afterAll, beforeAll } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import * as fs from 'fs';
import * as path from 'path';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-start');

describe('CLI Start Command', () => {
    beforeAll(() => workspace.ensureSubdir('test'));
    afterAll(() => workspace.cleanup());

    describe('starting config-defined services', () => {
        it('should start a service defined in config', async () => {
            const result = await workspace.runCli(['start', 'echo']);

            expect(result.stdoutAsString()).toContain('Started');
            expect(result.stdoutAsString()).toContain('echo');
        });

        it('should start multiple services at once', async () => {
            const result = await workspace.runCli(['start', 'echo', 'web']);

            expect(result.stdoutAsString()).toContain('echo');
            expect(result.stdoutAsString()).toContain('web');
        });

        it('should start all configured services when no name provided', async () => {
            const result = await workspace.runCli(['start']);

            // Should start both configured services (web and echo)
            expect(result.stdoutAsString()).toContain('Started');

            // Verify both are running
            const listResult = await workspace.runCli(['list']);
            expect(listResult.stdoutAsString()).toContain('web');
            expect(listResult.stdoutAsString()).toContain('echo');

            // Cleanup
            await workspace.runCli(['kill']);
        });

        it('should show error for unknown service name', async () => {
            const result = await workspace.runCli(['start', 'nonexistent-service'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('nonexistent-service');
        });

        it('should exit quickly after starting', async () => {
            const startTime = Date.now();
            await workspace.runCli(['start', 'echo']);
            const elapsed = Date.now() - startTime;

            // Should exit within a reasonable time (not waiting for process to complete)
            expect(elapsed).toBeLessThan(5000);
        });
    });

    describe('transient processes with --shell', () => {
        it('should start transient process with --shell flag', async () => {
            const result = await workspace.runCli(['start', 'my-transient', '--shell', 'node ../../sampleServers/testProcess.js']);

            expect(result.stdoutAsString()).toContain('Started');
            expect(result.stdoutAsString()).toContain('my-transient');
        });

        it('should start transient process with --shell and --root', async () => {
            const result = await workspace.runCli(['start', 'rooted', '--shell', 'node ../../../sampleServers/testProcess.js', '--root', 'test']);

            expect(result.stdoutAsString()).toContain('Started');
        });

        it('should error when --root is provided without --shell', async () => {
            const result = await workspace.runCli(['start', 'bad-config', '--root', 'test'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        });

        it('should error when --root escapes project directory', async () => {
            const result = await workspace.runCli(['start', 'escape', '--shell', 'echo hi', '--root', '../../../escape'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('root');
        });

        it('should allow transient to shadow config service', async () => {
            // 'echo' exists in config, but we start with different shell
            await workspace.runCli(['kill', 'echo'], { ignoreExitCode: true });
            const result = await workspace.runCli(['start', 'echo', '--shell', 'node ../../sampleServers/testProcess.js']);

            expect(result.stdoutAsString()).toContain('Started');

            // Verify it uses our shell by checking logs
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Test server started']);
            const logs = await workspace.runCli(['logs', 'echo']);
            expect(logs.stdoutAsString()).toContain('Test server started');
        });

        it('should refuse a different --shell for a running service and point at restart', async () => {
            await workspace.runCli(['restart', 'echo']);

            const result = await workspace.runCli(
                ['start', 'echo', '--shell', 'node ../../sampleServers/testProcess.js'],
                { ignoreExitCode: true }
            );

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('already running with a different command');
            expect(result.stderrAsString()).toContain("candle restart echo --shell");
        });

        it('should be a no-op for a running transient started with the same --shell', async () => {
            const shell = 'node ../../sampleServers/testProcess.js';
            await workspace.runCli(['start', 'same-transient', '--shell', shell]);

            const result = await workspace.runCli(['start', 'same-transient', '--shell', shell]);

            expect(result.stdoutAsString()).toContain("[Service 'same-transient' is already running");
            expect(result.stdoutAsString()).not.toContain('Started');
        });
    });

    describe('interactive vs non-interactive mode', () => {
        it('exits immediately and prints a logs hint in non-interactive mode', async () => {
            // Tests run the CLI with piped stdio, so start auto-detects
            // non-interactive mode: it exits after launch and points at `candle logs`.
            const result = await workspace.runCli(['start', 'echo']);

            expect(result.stdoutAsString()).toContain("Run 'candle logs echo' to see logs.");
        });

        it('streams new logs with --watch and detaches leaving the process running', async () => {
            // --watch forces interactive mode; --exit-after-ms makes it terminate on
            // its own so the test doesn't block.
            const result = await workspace.runCli(['start', 'echo', '--watch', '--exit-after-ms', '2500']);
            const output = result.stdoutAsString();

            // Watch-mode banner instead of the logs hint.
            expect(output).toContain('Press Ctrl+C to stop watching');
            expect(output).not.toContain("Run 'candle logs");

            // The new launch's output is streamed live.
            expect(output).toMatch(/Echo \d+:|Echo server started/);

            // Leaving watch mode does not stop the process.
            const list = await workspace.runCli(['list', '--json']);
            const processes = JSON.parse(list.stdoutAsString());
            const echo = processes.find((p: any) => p.serviceName === 'echo');
            expect(echo?.status).toBe('RUNNING');
        });

        it('exits immediately with --bg', async () => {
            const result = await workspace.runCli(['start', 'echo', '--bg']);

            expect(result.stdoutAsString()).toContain("Run 'candle logs echo' to see logs.");
        });

        it('errors when both --bg and --watch are given', async () => {
            const result = await workspace.runCli(['start', 'echo', '--bg', '--watch'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('--bg and --watch');
        });
    });

    describe('starting already running services', () => {
        it('should leave an already running service alone', async () => {
            await workspace.runCli(['restart', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            const before = JSON.parse((await workspace.runCli(['ps', 'echo', '--json'])).stdoutAsString());

            const result = await workspace.runCli(['start', 'echo']);

            expect(result.stdoutAsString()).toMatch(
                /\[Service 'echo' is already running \(pid \d+, up [^)]+\); use 'candle restart echo' to restart it\]/
            );
            expect(result.stdoutAsString()).not.toContain('Started');
            expect(result.stdoutAsString()).not.toContain('Killed');
            const after = JSON.parse((await workspace.runCli(['ps', 'echo', '--json'])).stdoutAsString());
            expect(after[0].pid).toBe(before[0].pid);
        });

        it('should start only the services that are not running', async () => {
            await workspace.runCli(['kill'], { ignoreExitCode: true });
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['start', 'echo', 'web']);

            const output = result.stdoutAsString();
            expect(output).toContain("[Service 'echo' is already running");
            expect(output).toContain("[Started process 'web']");
        });

        it('should note when a running service has an outdated command', async () => {
            const driftWorkspace = new TestWorkspace('cli-start-drift');
            const configPath = path.join(driftWorkspace.dbDir, '.candle.json');
            const writeConfig = (shell: string) =>
                fs.writeFileSync(configPath, JSON.stringify({ services: [{ name: 'svc', shell }] }));
            try {
                writeConfig('node ../../sampleServers/testProcess.js');
                await driftWorkspace.runCli(['start', 'svc']);
                writeConfig('node ../../sampleServers/testProcess.js --edited');

                const result = await driftWorkspace.runCli(['start', 'svc']);

                expect(result.stdoutAsString()).toContain('with an outdated command');
                expect(result.stdoutAsString()).toContain("use 'candle restart svc' to apply the .candle.json changes");
            } finally {
                await driftWorkspace.runCli(['kill-all'], { ignoreExitCode: true });
                await driftWorkspace.cleanup();
            }
        });

        it('should start service when DB has stale entry with dead PID (post-reboot)', async () => {
            // Use a fresh workspace so other tests don't interfere with the dead-PID row.
            const staleWorkspace = new TestWorkspace('cli-start-stale');
            fs.writeFileSync(
                path.join(staleWorkspace.dbDir, '.candle.json'),
                JSON.stringify({
                    services: [
                        { name: 'echo', shell: 'node ../../sampleServers/echoServer.js' },
                    ],
                })
            );

            // Initialize the database.
            await staleWorkspace.runCli(['list-all']);

            // Insert a stale row simulating post-reboot: killed_at=null but PID is dead.
            const dbPath = path.join(staleWorkspace.dbDir, 'candle.db');
            const db = new DatabaseSync(dbPath);
            const fakePid = 2147483000;
            db.exec(`insert into processes (command_name, project_dir, pid, log_collector_pid, start_time, shell)
                      values ('echo', '${staleWorkspace.dbDir}', ${fakePid}, ${fakePid + 1}, strftime('%s','now'), 'node ../../sampleServers/echoServer.js')`);
            db.close();

            // start should detect the dead PID and actually start the service,
            // NOT report 'already running'.
            const result = await staleWorkspace.runCli(['start', 'echo']);
            const output = result.stdoutAsString();
            expect(output).not.toContain('already running');
            expect(output).toContain('Started');

            await staleWorkspace.runCli(['kill-all'], { ignoreExitCode: true });
            await staleWorkspace.cleanup();
        });
    });

    describe('starting several services when one fails', () => {
        it('should still start the services after the failing one', async () => {
            const failWorkspace = new TestWorkspace('cli-start-partial-failure');
            fs.writeFileSync(
                path.join(failWorkspace.dbDir, '.candle.json'),
                JSON.stringify({
                    services: [
                        { name: 'first', shell: 'node ../../sampleServers/testProcess.js' },
                        { name: 'broken', shell: 'exit 3' },
                        { name: 'last', shell: 'node ../../sampleServers/testProcess.js' },
                    ],
                })
            );
            try {
                const result = await failWorkspace.runCli(['start'], { ignoreExitCode: true });

                expect(result.failed()).toBe(true);
                expect(result.stdoutAsString()).toContain("[Started process 'first']");
                expect(result.stdoutAsString()).toContain("[Started process 'last']");
                expect(result.stderrAsString()).toContain("Process 'broken' failed to start");
                expect(result.stderrAsString()).toContain('1 of 3 services failed to start: broken');

                const ps = JSON.parse((await failWorkspace.runCli(['ps', '--json'])).stdoutAsString());
                const status = Object.fromEntries(ps.map((p: any) => [p.serviceName, p.status]));
                expect(status.first).toBe('RUNNING');
                expect(status.last).toBe('RUNNING');
            } finally {
                await failWorkspace.runCli(['kill-all'], { ignoreExitCode: true });
                await failWorkspace.cleanup();
            }
        });
    });

    describe('directory without config', () => {
        it('should error when starting in directory without config', async () => {
            const result = await workspace.runCli(['start', 'something'], { cwd: '/tmp', ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('.candle.json');
        });

        it('should error for transient in directory without config', async () => {
            const result = await workspace.runCli(['start', 'temp', '--shell', 'echo hello'], { cwd: '/tmp', ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('.candle.json');
        });
    });

    describe('start output format', () => {
        it('should show process info after starting', async () => {
            const result = await workspace.runCli(['start', 'echo']);

            // Should indicate the process was started
            expect(result.stdoutAsString().toLowerCase()).toMatch(/start/);
        });

        it('should print the launch banner with the shell and root directory', async () => {
            await workspace.runCli(['kill', 'echo'], { ignoreExitCode: true });
            const result = await workspace.runCli(['start', 'echo']);
            const lines = result.stdoutAsString().split('\n');

            const banner = lines.findIndex(line => line.startsWith('[Started process'));
            expect(banner).toBeGreaterThanOrEqual(0);
            expect(lines[banner]).toBe(
                "[Started process 'echo'] $ node ../../sampleServers/echoServer.js"
            );
            expect(lines[banner + 1]).toBe(`[With root directory: ${workspace.dbDir}]`);
        });

        it('should show the transient shell in the launch banner', async () => {
            const shell = 'node ../../sampleServers/testProcess.js';
            const result = await workspace.runCli(['start', 'banner-transient', '--shell', shell]);

            expect(result.stdoutAsString().split('\n')[0]).toBe(
                `[Started process 'banner-transient'] $ ${shell}`
            );
        });

        it('should show the resolved root in the launch banner when --root is given', async () => {
            const result = await workspace.runCli([
                'start',
                'banner-rooted',
                '--shell',
                'node ../../../sampleServers/testProcess.js',
                '--root',
                'test',
            ]);

            expect(result.stdoutAsString()).toContain(
                `[With root directory: ${workspace.dbDir}/test]`
            );
        });

        it('should have minimal stderr on success', async () => {
            const result = await workspace.runCli(['start', 'echo']);

            // No errors on success
            expect(result.stderrAsString()).toBe('');
        });
    });

    describe('special characters in names', () => {
        it('should handle names with dashes', async () => {
            const result = await workspace.runCli(['start', 'my-dashed-name', '--shell', 'node ../../sampleServers/testProcess.js']);

            expect(result.stdoutAsString()).toContain('my-dashed-name');
        });

        it('should handle names with underscores', async () => {
            const result = await workspace.runCli(['start', 'my_underscore_name', '--shell', 'node ../../sampleServers/testProcess.js']);

            expect(result.stdoutAsString()).toContain('my_underscore_name');
        });
    });

});

describe('CLI Start with empty config', () => {
    const emptyWorkspace = new TestWorkspace('cli-start-empty');
    afterAll(() => emptyWorkspace.cleanup());

    it('should error when no services configured and no args', async () => {
        const result = await emptyWorkspace.runCli(['start'], { ignoreExitCode: true });

        expect(result.failed()).toBe(true);
        expect(result.stderrAsString()).toContain('No services configured');
    });
});
