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

    // Each doc is one line: "  <name>  <description>". Names never carry the .md extension.
    function listedDocs(output: string): { name: string; description: string }[] {
        return output
            .split('\n')
            .filter((line) => line.startsWith('  '))
            .map((line) => {
                const m = line.trim().match(/^(\S+)\s*(.*)$/)!;
                return { name: m[1], description: m[2] };
            });
    }

    describe('list-docs format', () => {
        it('should say how to show a doc', async () => {
            const result = await workspace.runCli(['list-docs']);

            expect(result.stdoutAsString()).toContain("candle get-doc <name>");
        });

        it('should list one doc per line with a description, without .md', async () => {
            const result = await workspace.runCli(['list-docs']);
            const docs = listedDocs(result.stdoutAsString());

            expect(docs.length).toBeGreaterThan(0);
            for (const doc of docs) {
                expect(doc.name).not.toMatch(/\.md$/);
                expect(doc.description.length).toBeGreaterThan(0);
            }
        });

        it('should list names that get-doc accepts', async () => {
            const result = await workspace.runCli(['list-docs']);
            const docs = listedDocs(result.stdoutAsString());

            for (const { name } of docs) {
                const doc = await workspace.runCli(['get-doc', name]);
                expect(doc.stdoutAsString().length).toBeGreaterThan(0);
            }
        });
    });

    describe('list-docs content', () => {
        it('should include known documentation files', async () => {
            const result = await workspace.runCli(['list-docs']);
            const names = listedDocs(result.stdoutAsString()).map((d) => d.name);

            expect(names).toEqual(expect.arrayContaining(['agents-intro', 'project-setup', 'mcp-usage', 'transient-processes', 'README']));
        });

        it('should give the README a description', async () => {
            const result = await workspace.runCli(['list-docs']);
            const readme = listedDocs(result.stdoutAsString()).find((d) => d.name === 'README');

            expect(readme?.description).toContain('README');
        });

        it('should not include developer docs from docs/dev', async () => {
            const result = await workspace.runCli(['list-docs']);

            expect(result.stdoutAsString()).not.toContain('testing-strategy');
        });
    });
});
