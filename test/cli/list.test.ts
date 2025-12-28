import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import { createCli, ensureCleanDbDir, getFixtureDir, getSampleServersDir } from './utils';

const TEST_NAME = 'cli-list';

describe('CLI List Command', () => {
    let dbDir: string;
    let cli: ReturnType<typeof createCli>;

    beforeAll(() => {
        dbDir = ensureCleanDbDir(TEST_NAME);
        cli = createCli(dbDir, getSampleServersDir());
    });

    afterEach(async () => {
        await cli(['kill-all']).catch(() => {});
    });

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

    describe('directory scoping', () => {
        it('should only show processes for current directory', async () => {
            // Start in sampleServers directory
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // List in basic fixture should not show echo
            const basicCli = createCli(dbDir, getFixtureDir('basic'));
            const result = await basicCli(['list']);

            expect(result.code).toBe(0);
            expect(result.stdout).not.toContain('echo');
        });
    });

    describe('edge cases', () => {
        it('should handle list when database is empty', async () => {
            const freshDbDir = ensureCleanDbDir('cli-list-fresh');
            const freshCli = createCli(freshDbDir, getSampleServersDir());

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
