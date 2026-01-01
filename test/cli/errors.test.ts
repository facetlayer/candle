import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-errors');

describe('CLI Error Handling', () => {
    afterAll(() => workspace.cleanup());

    describe('unknown commands', () => {
        it('should error for completely unknown command', async () => {
            const result = await workspace.runCli(['totally-unknown-command'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        });

        it('should show unrecognized error message for unknown command', async () => {
            const result = await workspace.runCli(['foobar'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            const output = result.stdoutAsString() + result.stderrAsString();
            expect(output.toLowerCase()).toContain('unrecognized');
        });
    });

    describe('missing required arguments', () => {
        it('should error when add-service missing arguments', async () => {
            const result = await workspace.runCli(['add-service'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        });

        it('should error when get-doc missing argument', async () => {
            const result = await workspace.runCli(['get-doc'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        });

        it('should error when wait-for-log missing --message', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['wait-for-log', 'echo'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        });
    });

    describe('timeout arguments', () => {
        it('should timeout quickly with short timeout on non-running service', async () => {
            const result = await workspace.runCli(['wait-for-log', 'echo', '--message', 'test', '--timeout', '1'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        }, 5000);

        it('should handle negative timeout', async () => {
            const result = await workspace.runCli(['wait-for-log', 'echo', '--message', 'test', '--timeout', '-1'], { ignoreExitCode: true });

            // Negative timeout is treated as 0 or fails immediately
            expect(result.failed()).toBe(true);
        }, 5000);
    });

    describe('directory without config file', () => {
        it('should error when starting service without config', async () => {
            const result = await workspace.runCli(['start', 'something'], { cwd: '/tmp', ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('.candle');
        });

        it('should error for list in directory without config', async () => {
            const result = await workspace.runCli(['list'], { cwd: '/tmp', ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        });

        it('should error for transient in directory without config', async () => {
            const result = await workspace.runCli(['start', 'temp', '--shell', 'echo hello'], { cwd: '/tmp', ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        });
    });

    describe('unknown service names', () => {
        it('should error for unknown service in start', async () => {
            const result = await workspace.runCli(['start', 'nonexistent-service-xyz'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('nonexistent-service-xyz');
        });

        it('should error when killing unknown service', async () => {
            const result = await workspace.runCli(['kill', 'nonexistent-service-xyz'], { ignoreExitCode: true });

            // CLI exits 1 for unknown service
            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('No service');
        });

        it('should report failure message when restarting unknown service', async () => {
            const result = await workspace.runCli(['restart', 'nonexistent-service-xyz']);

            // CLI exits 0 but reports failure in stderr
            expect(result.stderrAsString()).toContain('Failed to restart');
        });
    });

    describe('transient process validation', () => {
        it('should error when --root provided without --shell', async () => {
            const result = await workspace.runCli(['start', 'bad', '--root', 'some-dir'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        });

        it('should error when --root escapes project directory', async () => {
            const result = await workspace.runCli(['start', 'escape', '--shell', 'echo hi', '--root', '../../../escape'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString().toLowerCase()).toContain('root');
        });
    });

    describe('error message quality', () => {
        it('should have descriptive error for missing config', async () => {
            const result = await workspace.runCli(['start', 'web'], { cwd: '/tmp', ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toMatch(/\.candle\.json|\.candle-setup\.json|config/i);
        });

        it('should have descriptive error for unknown service', async () => {
            const result = await workspace.runCli(['start', 'unknown-xyz'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('unknown-xyz');
        });
    });

    describe('exit codes', () => {
        it('should have zero exit code for success', async () => {
            await workspace.runCli(['--help']);
        });

        it('should have non-zero exit code for errors', async () => {
            const result = await workspace.runCli(['start', 'nonexistent'], { cwd: '/tmp', ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        });
    });

    describe('stderr vs stdout for errors', () => {
        it('should output errors to stderr', async () => {
            const result = await workspace.runCli(['start', 'nonexistent'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString().length).toBeGreaterThan(0);
        });

        it('should output help to stdout', async () => {
            const result = await workspace.runCli(['--help']);

            expect(result.stdoutAsString().length).toBeGreaterThan(0);
            expect(result.stderrAsString()).toBe('');
        });
    });

});
