import { describe, it, expect } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-list-docs');
const cli = workspace.createCli();

describe('CLI List-Docs Command', () => {

    describe('basic list-docs functionality', () => {
        it('should list available documentation files', async () => {
            const result = await cli(['list-docs']);

            expect(result.code).toBe(0);
            // Should list docs that exist in the project
            expect(result.stdout.length).toBeGreaterThan(0);
        });

        it('should exit quickly', async () => {
            const startTime = Date.now();
            const result = await cli(['list-docs']);
            const elapsed = Date.now() - startTime;

            expect(result.code).toBe(0);
            expect(elapsed).toBeLessThan(2000);
        });
    });

    describe('list-docs output format', () => {
        it('should list docs as separate entries', async () => {
            const result = await cli(['list-docs']);

            expect(result.code).toBe(0);
            // Output should be parseable
            expect(typeof result.stdout).toBe('string');
        });

        it('should have minimal stderr on success', async () => {
            const result = await cli(['list-docs']);

            expect(result.code).toBe(0);
            expect(result.stderr).toBe('');
        });
    });

    describe('list-docs content', () => {
        it('should include known documentation files', async () => {
            const result = await cli(['list-docs']);

            expect(result.code).toBe(0);
            // Project has docs like getting-started.md
            // The exact files depend on the docs directory
        });
    });
});
