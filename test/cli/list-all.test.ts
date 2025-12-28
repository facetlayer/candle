import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import { createCli, ensureCleanDbDir, getFixtureDir, getSampleServersDir } from './utils';

const TEST_NAME = 'cli-list-all';

describe('CLI List-All Command', () => {
    let dbDir: string;
    let cli: ReturnType<typeof createCli>;
    let basicCli: ReturnType<typeof createCli>;

    beforeAll(() => {
        dbDir = ensureCleanDbDir(TEST_NAME);
        cli = createCli(dbDir, getSampleServersDir());
        basicCli = createCli(dbDir, getFixtureDir('basic'));
    });

    afterEach(async () => {
        await cli(['kill-all']).catch(() => {});
    });

    describe('basic list-all functionality', () => {
        it('should show empty list when no processes running anywhere', async () => {
            const result = await cli(['list-all']);

            expect(result.code).toBe(0);
        });

        it('should show processes from current directory', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['list-all']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('echo');
        });

        it('should show processes from all directories', async () => {
            // Start in sampleServers
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Start in basic fixture
            await basicCli(['start', 'web']);
            await basicCli(['wait-for-log', 'web', '--message', 'Server started']);

            // list-all from either directory should show both
            const result = await cli(['list-all']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('echo');
            expect(result.stdout).toContain('web');
        });
    });

    describe('output format', () => {
        it('should show project directory for each process', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['list-all']);

            expect(result.code).toBe(0);
            // Should include path information
            expect(result.stdout).toContain('sampleServers');
        });

        it('should have table-like format', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['list-all']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('NAME');
            expect(result.stdout).toContain('STATUS');
        });
    });

    describe('cross-directory visibility', () => {
        it('should show all processes regardless of cwd', async () => {
            // Start echo in sampleServers
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // list-all from basic fixture should still show echo
            const result = await basicCli(['list-all']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('echo');
        });
    });
});
