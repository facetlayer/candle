import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-list');
const cli = workspace.createCli();

describe('CLI List Command', () => {
    afterAll(() => workspace.cleanup());

    describe('basic list functionality', () => {
        it('should show empty list when no processes running', async () => {
            const result = await cli(['list']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('NAME');
            expect(result.stdout).toContain('STATUS');
        });

        it('should show running process in list', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['list']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('echo');
            expect(result.stdout).toContain('RUNNING');
        });

        it('should show multiple running processes', async () => {
            await cli(['start', 'echo']);
            await cli(['start', 'echo-test']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await cli(['wait-for-log', 'echo-test', '--message', 'Echo server started']);

            const result = await cli(['list']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('echo');
            expect(result.stdout).toContain('echo-test');
        });

        it('should show uptime for running processes', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['list']);

            expect(result.code).toBe(0);
            expect(result.stdout).toMatch(/\d+s|\d+m/);
        });
    });

    describe('ls alias', () => {
        it('should work with ls alias', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const listResult = await cli(['list']);
            const lsResult = await cli(['ls']);

            expect(listResult.code).toBe(0);
            expect(lsResult.code).toBe(0);

            expect(listResult.stdout).toContain('echo');
            expect(lsResult.stdout).toContain('echo');
        });
    });

    describe('list output format', () => {
        it('should have table-like format with columns', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['list']);

            expect(result.stdout).toContain('NAME');
            expect(result.stdout).toContain('STATUS');
            expect(result.stdout).toContain('UPTIME');
        });

        it('should show config changed warning for transient overrides', async () => {
            // Start 'echo' as transient with different shell
            await cli(['start', 'echo', '--shell', 'node testProcess.js']);
            await cli(['wait-for-log', 'echo', '--message', 'Test server started']);

            const result = await cli(['list']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('[config changed]');
        });
    });

    describe('edge cases', () => {
        it('should handle list when database is empty', async () => {
            const freshWorkspace = new TestWorkspace('cli-list-fresh');
            const freshCli = freshWorkspace.createCli();

            const result = await freshCli(['list']);

            expect(result.code).toBe(0);
        });

        it('should not show killed process as RUNNING', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Verify it's running
            const runningResult = await cli(['list']);
            expect(runningResult.stdout).toContain('echo');
            expect(runningResult.stdout).toContain('RUNNING');

            // Kill it
            await cli(['kill', 'echo']);

            // After kill, should not show as RUNNING
            const afterKillResult = await cli(['list']);
            expect(afterKillResult.code).toBe(0);
            expect(afterKillResult.stdout).not.toContain('RUNNING');
        });
    });
});
