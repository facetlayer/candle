import { describe, it, expect } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-get-doc');

describe('CLI Get-Doc Command', () => {

    describe('basic get-doc functionality', () => {
        it('should display getting-started documentation', async () => {
            const result = await workspace.runCli(['get-doc', 'getting-started']);

            expect(result.stdoutAsString()).toContain('Getting Started');
            expect(result.stdoutAsString().length).toBeGreaterThan(100);
        });

        it('should exit quickly', async () => {
            const startTime = Date.now();
            await workspace.runCli(['get-doc', 'getting-started']);
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
        });
    });

    describe('get-doc output format', () => {
        it('should output document content to stdout', async () => {
            const result = await workspace.runCli(['get-doc', 'getting-started']);

            expect(result.stdoutAsString().length).toBeGreaterThan(0);
            expect(result.stderrAsString()).toBe('');
        });
    });

    describe('get-doc name matching', () => {
        it('should not prefix-match a doc name', async () => {
            const result = await workspace.runCli(['get-doc', 'start'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('Doc file not found: start');
        });

        it('should accept the name with or without .md', async () => {
            const bare = await workspace.runCli(['get-doc', 'agents-intro']);
            const withExt = await workspace.runCli(['get-doc', 'agents-intro.md']);

            expect(bare.stdoutAsString()).toBe(withExt.stdoutAsString());
        });

        it('should strip YAML frontmatter', async () => {
            const result = await workspace.runCli(['get-doc', 'getting-started']);
            const output = result.stdoutAsString();

            expect(output.startsWith('---')).toBe(false);
            expect(output).not.toContain('description: Quick start guide');
        });

        it('should report the README at the repo root', async () => {
            const result = await workspace.runCli(['get-doc', 'README']);

            expect(result.stdoutAsString()).toContain('(File source: README.md)');
            expect(result.stdoutAsString()).not.toContain('docs/README.md');
        });

        it('should report docs/ as the source for other docs', async () => {
            const result = await workspace.runCli(['get-doc', 'getting-started']);

            expect(result.stdoutAsString()).toContain('(File source: docs/getting-started.md)');
        });
    });

    describe('get-doc for specific docs', () => {
        it('should get transient-processes doc', async () => {
            const result = await workspace.runCli(['get-doc', 'transient-processes']);

            expect(result.stdoutAsString()).toContain('Transient');
        });
    });
});
