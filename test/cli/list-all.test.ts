import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-list-all');

describe('CLI List-All Command', () => {
    afterAll(() => workspace.cleanup());

    describe('basic list-all functionality', () => {
        it('should show empty list when no processes running anywhere', async () => {
            await workspace.runCli(['list-all']);
        });

        it('should show processes from current directory', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['list-all']);

            expect(result.stdoutAsString()).toContain('echo');
        });
    });

    describe('output format', () => {
        it('should show project directory for each process', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['list-all']);

            // Should include path information
            expect(result.stdoutAsString()).toContain('cli-list-all');
        });

        it('should have table-like format', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['list-all']);

            expect(result.stdoutAsString()).toContain('NAME');
            expect(result.stdoutAsString()).toContain('STATUS');
        });
    });
});
