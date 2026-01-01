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

    describe('get-doc for specific docs', () => {
        it('should get transient-processes doc', async () => {
            const result = await workspace.runCli(['get-doc', 'transient-processes']);

            expect(result.stdoutAsString()).toContain('Transient');
        });
    });
});
