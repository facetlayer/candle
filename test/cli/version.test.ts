import { describe, it, expect, beforeAll } from 'vitest';
import { createCli, ensureCleanDbDir, getFixtureDir } from './utils';

const TEST_NAME = 'cli-version';
const dbDir = ensureCleanDbDir(TEST_NAME);
const cli = createCli(dbDir, getFixtureDir('basic'));

describe('CLI Version Command', () => {
    beforeAll(() => {
        ensureCleanDbDir(TEST_NAME);
    });

    describe('--version flag', () => {
        it('should display version number', async () => {
            const result = await cli(['--version']);

            expect(result.code).toBe(0);
            expect(result.stdout).toMatch(/\d+\.\d+\.\d+/);
        });

        it('should output only the version number', async () => {
            const result = await cli(['--version']);

            expect(result.code).toBe(0);
            const lines = result.stdout.trim().split('\n');
            expect(lines.length).toBe(1);
            expect(lines[0]).toMatch(/^\d+\.\d+\.\d+/);
        });

        it('should have no stderr output', async () => {
            const result = await cli(['--version']);

            expect(result.stderr).toBe('');
        });
    });

    describe('-v shorthand', () => {
        it('should work with -v flag', async () => {
            const result = await cli(['-v']);

            expect(result.code).toBe(0);
            expect(result.stdout).toMatch(/\d+\.\d+\.\d+/);
        });
    });

    describe('version consistency', () => {
        it('should return consistent version across calls', async () => {
            const result1 = await cli(['--version']);
            const result2 = await cli(['--version']);

            expect(result1.stdout.trim()).toBe(result2.stdout.trim());
        });

        it('should match package.json version', async () => {
            const result = await cli(['--version']);
            const versionFromCli = result.stdout.trim();

            const pkg = require('../../package.json');
            expect(versionFromCli).toBe(pkg.version);
        });
    });
});
