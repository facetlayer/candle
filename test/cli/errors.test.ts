import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-errors');
const cli = workspace.createCli();

describe('CLI Error Handling', () => {
    afterAll(() => workspace.cleanup());

    describe('unknown commands', () => {
        it('should error for completely unknown command', async () => {
            const result = await cli(['totally-unknown-command']);

            expect(result.code).not.toBe(0);
        });

        it('should show unrecognized error message for unknown command', async () => {
            const result = await cli(['foobar']);

            expect(result.code).not.toBe(0);
            const output = result.stdout + result.stderr;
            expect(output.toLowerCase()).toContain('unrecognized');
        });
    });

    describe('missing required arguments', () => {
        it('should error when add-service missing arguments', async () => {
            const result = await cli(['add-service']);

            expect(result.code).not.toBe(0);
        });

        it('should error when get-doc missing argument', async () => {
            const result = await cli(['get-doc']);

            expect(result.code).not.toBe(0);
        });

        it('should error when wait-for-log missing --message', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['wait-for-log', 'echo']);

            expect(result.code).not.toBe(0);
        });
    });

    describe('timeout arguments', () => {
        it('should timeout quickly with short timeout on non-running service', async () => {
            const result = await cli(['wait-for-log', 'echo', '--message', 'test', '--timeout', '1']);

            expect(result.code).not.toBe(0);
        }, 5000);

        it('should handle negative timeout', async () => {
            const result = await cli(['wait-for-log', 'echo', '--message', 'test', '--timeout', '-1']);

            // Negative timeout is treated as 0 or fails immediately
            expect(result.code).not.toBe(0);
        }, 5000);
    });

    describe('directory without config file', () => {
        it('should error when starting service without config', async () => {
            const result = await cli(['start', 'something'], { cwd: '/tmp' });

            expect(result.code).not.toBe(0);
            expect(result.stderr).toContain('.candle');
        });

        it('should error for list in directory without config', async () => {
            const result = await cli(['list'], { cwd: '/tmp' });

            expect(result.code).not.toBe(0);
        });

        it('should error for transient in directory without config', async () => {
            const result = await cli(['start', 'temp', '--shell', 'echo hello'], { cwd: '/tmp' });

            expect(result.code).not.toBe(0);
        });
    });

    describe('unknown service names', () => {
        it('should error for unknown service in start', async () => {
            const result = await cli(['start', 'nonexistent-service-xyz']);

            expect(result.code).not.toBe(0);
            expect(result.stderr).toContain('nonexistent-service-xyz');
        });

        it('should error when killing unknown service', async () => {
            const result = await cli(['kill', 'nonexistent-service-xyz']);

            // CLI exits 1 for unknown service
            expect(result.code).toBe(1);
            expect(result.stderr).toContain('No service');
        });

        it('should report failure message when restarting unknown service', async () => {
            const result = await cli(['restart', 'nonexistent-service-xyz']);

            // CLI exits 0 but reports failure in stderr
            expect(result.code).toBe(0);
            expect(result.stderr).toContain('Failed to restart');
        });
    });

    describe('transient process validation', () => {
        it('should error when --root provided without --shell', async () => {
            const result = await cli(['start', 'bad', '--root', 'some-dir']);

            expect(result.code).not.toBe(0);
        });

        it('should error when --root escapes project directory', async () => {
            const result = await cli(['start', 'escape', '--shell', 'echo hi', '--root', '../../../escape']);

            expect(result.code).not.toBe(0);
            expect(result.stderr.toLowerCase()).toContain('root');
        });
    });

    describe('error message quality', () => {
        it('should have descriptive error for missing config', async () => {
            const result = await cli(['start', 'web'], { cwd: '/tmp' });

            expect(result.code).not.toBe(0);
            expect(result.stderr).toMatch(/\.candle\.json|\.candle-setup\.json|config/i);
        });

        it('should have descriptive error for unknown service', async () => {
            const result = await cli(['start', 'unknown-xyz']);

            expect(result.code).not.toBe(0);
            expect(result.stderr).toContain('unknown-xyz');
        });
    });

    describe('exit codes', () => {
        it('should have zero exit code for success', async () => {
            const result = await cli(['--help']);

            expect(result.code).toBe(0);
        });

        it('should have non-zero exit code for errors', async () => {
            const result = await cli(['start', 'nonexistent'], { cwd: '/tmp' });

            expect(result.code).not.toBe(0);
        });
    });

    describe('stderr vs stdout for errors', () => {
        it('should output errors to stderr', async () => {
            const result = await cli(['start', 'nonexistent']);

            expect(result.code).not.toBe(0);
            expect(result.stderr.length).toBeGreaterThan(0);
        });

        it('should output help to stdout', async () => {
            const result = await cli(['--help']);

            expect(result.code).toBe(0);
            expect(result.stdout.length).toBeGreaterThan(0);
            expect(result.stderr).toBe('');
        });
    });

});
