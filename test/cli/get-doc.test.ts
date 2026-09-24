import { describe, it, expect } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-get-doc');

describe('CLI Get-Doc Command', () => {

    describe('basic get-doc functionality', () => {
        it('should display project-setup documentation', async () => {
            const result = await workspace.runCli(['get-doc', 'project-setup']);

            expect(result.stdoutAsString()).toContain('Project Setup');
            expect(result.stdoutAsString().length).toBeGreaterThan(100);
        });

        it('should exit quickly', async () => {
            const startTime = Date.now();
            await workspace.runCli(['get-doc', 'project-setup']);
            const elapsed = Date.now() - startTime;

            expect(elapsed).toBeLessThan(2000);
        });
    });

    describe('get-doc for non-existent doc', () => {
        it('should error for unknown document', async () => {
            const result = await workspace.runCli(['get-doc', 'nonexistent-doc-xyz'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
        });
    });

    describe('get-doc without name', () => {
        it('should error when no document name provided', async () => {
            const result = await workspace.runCli(['get-doc'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('get-doc requires a <name>');
            expect(result.stderrAsString()).not.toContain('Doc file not found');
        });
    });

    describe('get-doc output format', () => {
        it('should output document content to stdout', async () => {
            const result = await workspace.runCli(['get-doc', 'project-setup']);

            expect(result.stdoutAsString().length).toBeGreaterThan(0);
            expect(result.stderrAsString()).toBe('');
        });
    });

    describe('get-doc name matching', () => {
        it('should not prefix-match a doc name', async () => {
            const result = await workspace.runCli(['get-doc', 'project'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('Doc file not found: project');
        });

        it('should accept the name with or without .md', async () => {
            const bare = await workspace.runCli(['get-doc', 'agents-intro']);
            const withExt = await workspace.runCli(['get-doc', 'agents-intro.md']);

            expect(bare.stdoutAsString()).toBe(withExt.stdoutAsString());
        });

        it('should strip YAML frontmatter', async () => {
            const result = await workspace.runCli(['get-doc', 'project-setup']);
            const output = result.stdoutAsString();

            expect(output.startsWith('---')).toBe(false);
            expect(output).not.toContain('description: Set up a project');
        });

        it('should not print a file source annotation', async () => {
            const result = await workspace.runCli(['get-doc', 'project-setup']);

            expect(result.stdoutAsString()).not.toContain('File source');
        });
    });

    describe('get-doc for specific docs', () => {
        it('should get transient-processes doc', async () => {
            const result = await workspace.runCli(['get-doc', 'transient-processes']);

            expect(result.stdoutAsString()).toContain('Transient');
        });

        it('should get mcp-usage doc', async () => {
            const result = await workspace.runCli(['get-doc', 'mcp-usage']);

            expect(result.stdoutAsString()).toContain('candle mcp');
        });
    });
});
