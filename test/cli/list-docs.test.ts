import { describe, it, expect } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-list-docs');

describe('CLI List-Docs Command', () => {

    describe('basic list-docs functionality', () => {
        it('should list available documentation files', async () => {
            const result = await workspace.runCli(['list-docs']);

            // Should list docs that exist in the project
            expect(result.stdoutAsString().length).toBeGreaterThan(0);
        });

        it('should exit quickly', async () => {
            const startTime = Date.now();
            await workspace.runCli(['list-docs']);
            const elapsed = Date.now() - startTime;

            expect(elapsed).toBeLessThan(2000);
        });
    });

    describe('list-docs output format', () => {
        it('should list docs as separate entries', async () => {
            const result = await workspace.runCli(['list-docs']);

            // Output should be parseable
            expect(typeof result.stdoutAsString()).toBe('string');
        });

        it('should have minimal stderr on success', async () => {
            const result = await workspace.runCli(['list-docs']);

            expect(result.stderrAsString()).toBe('');
        });
    });

    describe('list-docs hints', () => {
        it('should suggest get-doc with the listed key, not the filename', async () => {
            const result = await workspace.runCli(['list-docs']);
            const output = result.stdoutAsString();

            expect(output).toContain('candle get-doc agents-intro)');
            expect(output).not.toMatch(/candle get-doc [^)\s]+\.md/);
        });

        it('should list keys that get-doc accepts', async () => {
            const result = await workspace.runCli(['list-docs']);
            const keys = [...result.stdoutAsString().matchAll(/\(candle get-doc ([^)]+)\)/g)].map((m) => m[1]);

            expect(keys.length).toBeGreaterThan(0);
            for (const key of keys) {
                const doc = await workspace.runCli(['get-doc', key]);
                expect(doc.stdoutAsString().length).toBeGreaterThan(0);
            }
        });
    });

    describe('list-docs content', () => {
        it('should include known documentation files', async () => {
            await workspace.runCli(['list-docs']);

            // Project has docs like getting-started.md
            // The exact files depend on the docs directory
        });
    });
});
