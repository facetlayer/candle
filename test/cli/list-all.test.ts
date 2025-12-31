import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-list-all');
const cli = workspace.createCli();

describe('CLI List-All Command', () => {
    afterAll(() => workspace.cleanup());

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
    });

    describe('output format', () => {
        it('should show project directory for each process', async () => {
            await cli(['start', 'echo']);
            await cli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await cli(['list-all']);

            expect(result.code).toBe(0);
            // Should include path information
            expect(result.stdout).toContain('cli-list-all');
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
});
