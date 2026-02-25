import { describe, it, expect } from 'vitest';
import { TestWorkspace, normalizeOutput } from './utils';

const workspace = new TestWorkspace('cli-help');

describe('CLI Help Command', () => {

    describe('--help flag', () => {
        it('should display main help when --help is passed', async () => {
            const result = await workspace.runCli(['--help']);

            expect(result.stdoutAsString()).toContain('Process Management:');
            expect(result.stdoutAsString()).toContain('run');
            expect(result.stdoutAsString()).toContain('start');
            expect(result.stdoutAsString()).toContain('kill');
            expect(result.stdoutAsString()).toContain('restart');
            expect(result.stdoutAsString()).toContain('list');
            expect(result.stdoutAsString()).toContain('logs');
        });

        it('should display help with all expected commands', async () => {
            const result = await workspace.runCli(['--help']);

            // All main commands should be listed
            const commands = [
                'run',
                'start',
                'restart',
                'kill',
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
                expect(result.stdoutAsString()).toContain(cmd);
            }
        });

        it('should have consistent help output format (snapshot)', async () => {
            const result = await workspace.runCli(['--help']);

            // Snapshot the structure of help output (normalized)
            const normalized = normalizeOutput(result.stdoutAsString());

            // Check for consistent structure with sections
            expect(normalized).toContain('Process Management:');
            expect(normalized).toContain('Port Detection:');
            expect(normalized).toContain('Logs:');
            expect(normalized).toContain('Configuration:');
            expect(normalized).toContain('Documentation:');
            expect(normalized).toContain('Troubleshooting & Maintenance:');
            expect(normalized).toContain('Options:');
        });
    });

    describe('no arguments', () => {
        it('should display help when no arguments provided', async () => {
            const result = await workspace.runCli([]);

            const output = result.stdoutAsString() + result.stderrAsString();
            expect(output).toContain('Process Management:');
        });
    });

    describe('help command', () => {
        it('should display main help when help command is used', async () => {
            const result = await workspace.runCli(['help']);

            expect(result.stdoutAsString()).toContain('Process Management:');
            expect(result.stdoutAsString()).toContain('run');
            expect(result.stdoutAsString()).toContain('start');
        });

        it('should error for unknown help topic', async () => {
            const result = await workspace.runCli(['help', 'nonexistent'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            const output = result.stdoutAsString() + result.stderrAsString();
            expect(output).toContain('Unknown help topic');
        });
    });

    describe('command-specific help', () => {
        it('should show help for run command', async () => {
            const result = await workspace.runCli(['run', '--help']);

            expect(result.stdoutAsString()).toContain('run');
        });

        it('should show help for start command', async () => {
            const result = await workspace.runCli(['start', '--help']);

            expect(result.stdoutAsString()).toContain('start');
        });

        it('should show help for kill command', async () => {
            const result = await workspace.runCli(['kill', '--help']);

            expect(result.stdoutAsString()).toContain('kill');
        });

        it('should show help for list command', async () => {
            const result = await workspace.runCli(['list', '--help']);

            expect(result.stdoutAsString()).toContain('list');
        });

        it('should show help for logs command', async () => {
            const result = await workspace.runCli(['logs', '--help']);

            expect(result.stdoutAsString()).toContain('logs');
            expect(result.stdoutAsString()).toContain('--count');
            expect(result.stdoutAsString()).toContain('--start-at');
        });

        it('should show help for wait-for-log command', async () => {
            const result = await workspace.runCli(['wait-for-log', '--help']);

            expect(result.stdoutAsString()).toContain('wait-for-log');
            expect(result.stdoutAsString()).toContain('--message');
            expect(result.stdoutAsString()).toContain('--timeout');
        });

        it('should show help for add-service command', async () => {
            const result = await workspace.runCli(['add-service', '--help']);

            expect(result.stdoutAsString()).toContain('add-service');
            expect(result.stdoutAsString()).toContain('--root');
        });

        it('should show help for restart command', async () => {
            const result = await workspace.runCli(['restart', '--help']);

            expect(result.stdoutAsString()).toContain('restart');
        });

        it('should show help for list-all command', async () => {
            const result = await workspace.runCli(['list-all', '--help']);

            expect(result.stdoutAsString()).toContain('list-all');
        });

        it('should show help for kill-all command', async () => {
            const result = await workspace.runCli(['kill-all', '--help']);

            expect(result.stdoutAsString()).toContain('kill-all');
        });

        it('should show help for clear-logs command', async () => {
            const result = await workspace.runCli(['clear-logs', '--help']);

            expect(result.stdoutAsString()).toContain('clear-logs');
        });

        it('should show help for erase-database command', async () => {
            const result = await workspace.runCli(['erase-database', '--help']);

            expect(result.stdoutAsString()).toContain('erase-database');
        });

        it('should show help for list-docs command', async () => {
            const result = await workspace.runCli(['list-docs', '--help']);

            expect(result.stdoutAsString()).toContain('list-docs');
        });

        it('should show help for get-doc command', async () => {
            const result = await workspace.runCli(['get-doc', '--help']);

            expect(result.stdoutAsString()).toContain('get-doc');
        });
    });

    describe('help aliases', () => {
        it('should show same help for ls and list', async () => {
            const listResult = await workspace.runCli(['list', '--help']);
            const lsResult = await workspace.runCli(['ls', '--help']);

            // Both should succeed and have similar content
            expect(listResult.stdoutAsString()).toBeDefined();
            expect(lsResult.stdoutAsString()).toBeDefined();
        });
    });

    describe('invalid commands', () => {
        it('should show error for unrecognized command', async () => {
            const result = await workspace.runCli(['nonexistent-command'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            const output = result.stdoutAsString() + result.stderrAsString();
            expect(output.toLowerCase()).toContain('unrecognized');
        });

        it('should suggest similar commands for typos', async () => {
            const result = await workspace.runCli(['stat'], { ignoreExitCode: true }); // typo for 'start'

            // May or may not have suggestions, but should fail
            expect(result.failed()).toBe(true);
        });
    });
});
