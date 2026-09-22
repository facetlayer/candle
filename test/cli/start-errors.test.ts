import * as fs from 'fs';
import * as path from 'path';
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

// Start / restart failure reporting.
const workspace = new TestWorkspace('cli-start-errors');

describe('start and restart errors', () => {
    beforeAll(() => {
        workspace.ensureSubdir('sub');
    });

    afterAll(() => workspace.cleanup());

    describe('--root with a configured service', () => {
        it('is rejected instead of silently ignored', async () => {
            const result = await workspace.runCli(['start', 'web', '--root', 'sub'], { ignoreExitCode: true });
            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('--root only applies');
            expect(result.stderrAsString()).toContain('--shell');
            expect(result.stdoutAsString()).not.toContain('Started');
        });

        it('still reports an unknown name as unknown', async () => {
            const result = await workspace.runCli(['start', 'nope', '--root', 'sub'], { ignoreExitCode: true });
            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain("No service 'nope' configured");
        });
    });

    describe('failed start messages', () => {
        it('names the missing root directory', async () => {
            const result = await workspace.runCli(['start', 'missing-root'], { ignoreExitCode: true });
            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain(
                `root directory does not exist: ${path.join(workspace.dbDir, 'does-not-exist')}`,
            );
            expect(result.stderrAsString()).not.toContain('os error 2');
        });

        it('names the missing executable', async () => {
            const result = await workspace.runCli(['start', 'missing-exe'], { ignoreExitCode: true });
            const stderr = result.stderrAsString();
            expect(result.failed()).toBe(true);
            expect(stderr).toContain("Process 'missing-exe' failed to start. Recent logs:\n");
            expect(stderr).toContain('nosuch-binary-xyz-123');
            expect(stderr).not.toContain('root directory');
            // No trailing space after the label, and no blank line after it.
            expect(stderr).not.toMatch(/Recent logs: /);
            expect(stderr).not.toMatch(/Recent logs:\n\n/);
        });
    });

    describe('restart', () => {
        it('does not print the stale-entry cleanup message', async () => {
            await workspace.runCli(['start', 'web']);
            const result = await workspace.runCli(['restart', 'web']);
            expect(result.stdoutAsString()).toContain("[Started process 'web']");
            expect(result.stdoutAsString() + result.stderrAsString()).not.toContain('Cleaning up stale');
        });

        it('exits non-zero when the start inside it fails', async () => {
            await workspace.runCli(['start', 'rooted']);
            const subDir = path.join(workspace.dbDir, 'sub');
            const movedDir = path.join(workspace.dbDir, 'sub-moved');
            fs.renameSync(subDir, movedDir);
            try {
                const result = await workspace.runCli(['restart', 'rooted'], { ignoreExitCode: true });
                expect(result.failed()).toBe(true);
                expect(result.stderrAsString()).toContain('Failed to restart');
                expect(result.stderrAsString()).toContain('root directory does not exist');
            } finally {
                fs.renameSync(movedDir, subDir);
            }
        });
    });
});
