import { describe, it, expect } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-get-doc');
const cli = workspace.createCli();

describe('CLI Get-Doc Command', () => {

    describe('basic get-doc functionality', () => {
        it('should display getting-started documentation', async () => {
            const result = await cli(['get-doc', 'getting-started']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Getting Started');
            expect(result.stdout.length).toBeGreaterThan(100);
        });

        it('should exit quickly', async () => {
            const startTime = Date.now();
            const result = await cli(['get-doc', 'getting-started']);
            const elapsed = Date.now() - startTime;

            expect(result.code).toBe(0);
            expect(elapsed).toBeLessThan(2000);
        });
    });

    describe('get-doc for non-existent doc', () => {
        it('should error for unknown document', async () => {
            const result = await cli(['get-doc', 'nonexistent-doc-xyz']);

            expect(result.code).not.toBe(0);
        });
    });

    describe('get-doc without name', () => {
        it('should error when no document name provided', async () => {
            const result = await cli(['get-doc']);

            expect(result.code).not.toBe(0);
        });
    });

    describe('get-doc output format', () => {
        it('should output document content to stdout', async () => {
            const result = await cli(['get-doc', 'getting-started']);

            expect(result.code).toBe(0);
            expect(result.stdout.length).toBeGreaterThan(0);
            expect(result.stderr).toBe('');
        });
    });

    describe('get-doc for specific docs', () => {
        it('should get transient-processes doc', async () => {
            const result = await cli(['get-doc', 'transient-processes']);

            expect(result.code).toBe(0);
            expect(result.stdout).toContain('Transient');
        });
    });
});
