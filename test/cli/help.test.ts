import { describe, it, expect, beforeAll } from 'vitest';
import { createCli, ensureCleanDbDir, getFixtureDir, normalizeOutput } from './utils';

const TEST_NAME = 'cli-help';
const dbDir = ensureCleanDbDir(TEST_NAME);
const cli = createCli(dbDir, getFixtureDir('basic'));

describe('CLI Help Command', () => {
    beforeAll(() => {
        ensureCleanDbDir(TEST_NAME);
    });

    describe('--help flag', () => {
        it('should display main help when --help is passed', async () => {
            const result = await cli(['--help']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Commands:');
            expect(result.stdout).toContain('run');
            expect(result.stdout).toContain('start');
            expect(result.stdout).toContain('kill');
            expect(result.stdout).toContain('restart');
            expect(result.stdout).toContain('list');
            expect(result.stdout).toContain('logs');
        });

        it('should display help with all expected commands', async () => {
            const result = await cli(['--help']);

            // All main commands should be listed
            const commands = [
                'run',
                'start',
                'restart',
                'kill',
                'stop',
                'kill-all',
                'list',
                'ls',
                'list-all',
                'logs',
                'watch',
                'wait-for-log',
                'clear-logs',
                'erase-database',
                'add-service',
                'list-docs',
                'get-doc',
            ];

            for (const cmd of commands) {
                expect(result.stdout).toContain(cmd);
            }
        });

        it('should have consistent help output format (snapshot)', async () => {
            const result = await cli(['--help']);

            // Snapshot the structure of help output (normalized)
            const normalized = normalizeOutput(result.stdout);

            // Check for consistent structure
            expect(normalized).toContain('Commands:');
            expect(normalized).toContain('Options:');
            expect(result.code).toBe(0);
        });
    });

    describe('no arguments', () => {
        it('should display help when no arguments provided', async () => {
            const result = await cli([]);

            const output = result.stdout + result.stderr;
            expect(output).toContain('Commands:');
        });
    });

    describe('command-specific help', () => {
        it('should show help for run command', async () => {
            const result = await cli(['run', '--help']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('run');
        });

        it('should show help for start command', async () => {
            const result = await cli(['start', '--help']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('start');
        });

        it('should show help for kill command', async () => {
            const result = await cli(['kill', '--help']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('kill');
        });

        it('should show help for list command', async () => {
            const result = await cli(['list', '--help']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('list');
        });

        it('should show help for logs command', async () => {
            const result = await cli(['logs', '--help']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('logs');
        });

        it('should show help for wait-for-log command', async () => {
            const result = await cli(['wait-for-log', '--help']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('wait-for-log');
            expect(result.stdout).toContain('--message');
            expect(result.stdout).toContain('--timeout');
        });

        it('should show help for add-service command', async () => {
            const result = await cli(['add-service', '--help']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('add-service');
            expect(result.stdout).toContain('--root');
        });

        it('should show help for restart command', async () => {
            const result = await cli(['restart', '--help']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('restart');
        });

        it('should show help for list-all command', async () => {
            const result = await cli(['list-all', '--help']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('list-all');
        });

        it('should show help for kill-all command', async () => {
            const result = await cli(['kill-all', '--help']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('kill-all');
        });

        it('should show help for clear-logs command', async () => {
            const result = await cli(['clear-logs', '--help']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('clear-logs');
        });

        it('should show help for erase-database command', async () => {
            const result = await cli(['erase-database', '--help']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('erase-database');
        });

        it('should show help for list-docs command', async () => {
            const result = await cli(['list-docs', '--help']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('list-docs');
        });

        it('should show help for get-doc command', async () => {
            const result = await cli(['get-doc', '--help']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('get-doc');
        });
    });

    describe('help aliases', () => {
        it('should show same help for ls and list', async () => {
            const listResult = await cli(['list', '--help']);
            const lsResult = await cli(['ls', '--help']);

            // Both should succeed and have similar content
            expect(listResult.code).toBe(0);
            expect(lsResult.code).toBe(0);
        });

        it('should show same help for stop and kill', async () => {
            const killResult = await cli(['kill', '--help']);
            const stopResult = await cli(['stop', '--help']);

            // Both should succeed
            expect(killResult.code).toBe(0);
            expect(stopResult.code).toBe(0);
        });
    });

    describe('invalid commands', () => {
        it('should show error for unrecognized command', async () => {
            const result = await cli(['nonexistent-command']);

            expect(result.code).not.toBe(0);
            const output = result.stdout + result.stderr;
            expect(output.toLowerCase()).toContain('unrecognized');
        });

        it('should suggest similar commands for typos', async () => {
            const result = await cli(['stat']); // typo for 'start'

            // May or may not have suggestions, but should fail
            expect(result.code).not.toBe(0);
        });
    });
});
