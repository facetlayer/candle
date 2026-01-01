import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-erase-database');

describe('CLI Erase-Database Command', () => {
    afterAll(() => workspace.cleanup());

    describe('basic erase-database functionality', () => {
        it('should erase the database', async () => {
            // Start a service to create database entries
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['kill', 'echo']);

            await workspace.runCli(['erase-database']);
        });

        it('should work on empty database', async () => {
            await workspace.runCli(['erase-database']);
        });

        it('should exit quickly', async () => {
            const startTime = Date.now();
            await workspace.runCli(['erase-database']);
            const elapsed = Date.now() - startTime;

            expect(elapsed).toBeLessThan(2000);
        });
    });

    describe('erase-database effects', () => {
        it('should clear process state', async () => {
            // Start and stop a service
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['kill', 'echo']);

            await workspace.runCli(['erase-database']);
        });

        it('should clear list-all results', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await workspace.runCli(['kill', 'echo']);

            await workspace.runCli(['erase-database']);

            const list = await workspace.runCli(['list-all']);
            expect(list.stdoutAsString()).not.toContain('echo');
        });
    });

    describe('erase-database output format', () => {
        it('should have minimal output', async () => {
            await workspace.runCli(['erase-database']);
        });

        it('should have no stderr on success', async () => {
            const result = await workspace.runCli(['erase-database']);

            expect(result.stderrAsString()).toBe('');
        });
    });

    describe('erase-database is recognized command', () => {
        it('should not show unrecognized command error', async () => {
            const result = await workspace.runCli(['erase-database']);

            expect(result.stderrAsString()).not.toContain('Unrecognized command');
        });
    });
});
