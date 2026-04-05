import { describe, it, expect, afterAll } from 'vitest';
import Database from 'better-sqlite3';
import * as path from 'path';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-list');

describe('CLI List Command', () => {
    afterAll(() => workspace.cleanup());

    describe('basic list functionality', () => {
        it('should show empty list when no processes running', async () => {
            const result = await workspace.runCli(['list']);

            expect(result.stdoutAsString()).toContain('NAME');
            expect(result.stdoutAsString()).toContain('STATUS');
        });

        it('should show running process in list', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['list']);

            expect(result.stdoutAsString()).toContain('echo');
            expect(result.stdoutAsString()).toContain('RUNNING');
        });

        it('should show multiple running processes', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['start', 'echo-test']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['wait-for-log', 'echo-test', '--message', 'Echo server started']);

            const result = await workspace.runCli(['list']);

            expect(result.stdoutAsString()).toContain('echo');
            expect(result.stdoutAsString()).toContain('echo-test');
        });

        it('should show uptime for running processes', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['list']);

            expect(result.stdoutAsString()).toMatch(/\d+s|\d+m/);
        });
    });

    describe('ls alias', () => {
        it('should work with ls alias', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const listResult = await workspace.runCli(['list']);
            const lsResult = await workspace.runCli(['ls']);

            expect(listResult.stdoutAsString()).toContain('echo');
            expect(lsResult.stdoutAsString()).toContain('echo');
        });
    });

    describe('list output format', () => {
        it('should have table-like format with columns', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['list']);

            expect(result.stdoutAsString()).toContain('NAME');
            expect(result.stdoutAsString()).toContain('STATUS');
            expect(result.stdoutAsString()).toContain('UPTIME');
        });

        it('should show config changed warning for transient overrides', async () => {
            // Start 'echo' as transient with different shell
            await workspace.runCli(['start', 'echo', '--shell', 'node ../../sampleServers/testProcess.js']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Test server started']);

            const result = await workspace.runCli(['list']);

            expect(result.stdoutAsString()).toContain('[config changed]');
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
            const db = new Database(dbPath);
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
            const runningResult = await workspace.runCli(['list']);
            expect(runningResult.stdoutAsString()).toContain('echo');
            // Find the line with 'echo' specifically (not echo-test) and verify it's RUNNING
            const echoLineRunning = runningResult.stdoutAsString().split('\n').find(line =>
                line.includes('echo') && !line.includes('echo-test') && line.includes('RUNNING')
            );
            expect(echoLineRunning).toBeDefined();

            // Kill it
            await workspace.runCli(['kill', 'echo']);

            // After kill, 'echo' specifically should not show as RUNNING
            const afterKillResult = await workspace.runCli(['list']);
            const echoLineAfterKill = afterKillResult.stdoutAsString().split('\n').find(line =>
                line.includes('echo') && !line.includes('echo-test') && line.includes('RUNNING')
            );
            expect(echoLineAfterKill).toBeUndefined();
        });
    });
});
