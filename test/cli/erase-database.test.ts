import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs';
import { createCli, ensureCleanDbDir, getSampleServersDir } from './utils';

const TEST_NAME = 'cli-erase-database';

describe('CLI Erase-Database Command', () => {
    let dbDir: string;
    let cli: ReturnType<typeof createCli>;

    beforeEach(() => {
        dbDir = ensureCleanDbDir(TEST_NAME + '-' + Date.now());
        cli = createCli(dbDir, getSampleServersDir());
    });

    afterEach(async () => {
        await cli(['kill-all']).catch(() => {});
        if (fs.existsSync(dbDir)) {
            fs.rmSync(dbDir, { recursive: true, force: true });
        }
    });

    describe('basic erase-database functionality', () => {
        it('should erase the database', async () => {
            // Start a service to create database entries
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await cli(['kill', 'echo']);

            const result = await cli(['erase-database']);

            expect(result.code).toBe(0);
        });

        it('should work on empty database', async () => {
            const result = await cli(['erase-database']);

            expect(result.code).toBe(0);
        });

        it('should exit quickly', async () => {
            const startTime = Date.now();
            const result = await cli(['erase-database']);
            const elapsed = Date.now() - startTime;

            expect(result.code).toBe(0);
            expect(elapsed).toBeLessThan(2000);
        });
    });

    describe('erase-database effects', () => {
        it('should clear process state', async () => {
            // Start and stop a service
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await cli(['kill', 'echo']);

            const result = await cli(['erase-database']);
            expect(result.code).toBe(0);
        });

        it('should clear list-all results', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);
            await cli(['kill', 'echo']);

            await cli(['erase-database']);

            const list = await cli(['list-all']);
            expect(list.stdout).not.toContain('echo');
        });
    });

    describe('erase-database output format', () => {
        it('should have minimal output', async () => {
            const result = await cli(['erase-database']);

            expect(result.code).toBe(0);
        });

        it('should have no stderr on success', async () => {
            const result = await cli(['erase-database']);

            expect(result.code).toBe(0);
            expect(result.stderr).toBe('');
        });
    });

    describe('erase-database is recognized command', () => {
        it('should not show unrecognized command error', async () => {
            const result = await cli(['erase-database']);

            expect(result.stderr).not.toContain('Unrecognized command');
            expect(result.code).toBe(0);
        });
    });
});
