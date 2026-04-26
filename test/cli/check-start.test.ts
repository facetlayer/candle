import { describe, it, expect, afterAll, beforeAll } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import * as path from 'path';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-check-start');

describe('CLI Check-Start Command', () => {
    beforeAll(() => workspace.ensureSubdir('test'));
    afterAll(() => workspace.cleanup());

    it('should start a service when not running', async () => {
        // Make sure it's not running
        await workspace.runCli(['kill', 'echo'], { ignoreExitCode: true });

        const result = await workspace.runCli(['check-start', 'echo']);

        expect(result.stdoutAsString()).toContain('Started');
        expect(result.stdoutAsString()).toContain('echo');
    });

    it('should skip starting when service is already running', async () => {
        // Start the service first
        await workspace.runCli(['start', 'echo']);
        await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

        // Now check-start should be a no-op
        const result = await workspace.runCli(['check-start', 'echo']);

        expect(result.stdoutAsString()).toContain('already running');
        expect(result.stdoutAsString()).not.toContain('Started');
    });

    it('should start service when DB has stale entry with dead PID (post-reboot)', async () => {
        // Use a fresh workspace so other tests don't interfere with the dead-PID row.
        const staleWorkspace = new TestWorkspace('cli-check-start-stale');
        // Provide a config so 'echo' is resolvable.
        const fs = await import('fs');
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

        // check-start should detect the dead PID and actually start the service,
        // NOT report 'already running'.
        const result = await staleWorkspace.runCli(['check-start', 'echo']);
        const output = result.stdoutAsString();
        expect(output).not.toContain('already running');
        expect(output).toContain('Started');

        // Cleanup
        await staleWorkspace.runCli(['kill-all'], { ignoreExitCode: true });
        await staleWorkspace.cleanup();
    });

    it('should work with multiple service names', async () => {
        // Start echo but not web
        await workspace.runCli(['kill'], { ignoreExitCode: true });
        await workspace.runCli(['start', 'echo']);
        await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

        // check-start both: echo should skip, web should start
        const result = await workspace.runCli(['check-start', 'echo', 'web']);

        const output = result.stdoutAsString();
        expect(output).toContain("already running");
        expect(output).toContain("Started");
        expect(output).toContain("web");
    });
});
