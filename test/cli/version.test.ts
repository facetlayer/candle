import { describe, it, expect } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-version');

describe('CLI Version Command', () => {

    describe('--version flag', () => {
        it('should display version number', async () => {
            const result = await workspace.runCli(['--version']);

            expect(result.stdoutAsString()).toMatch(/\d+\.\d+\.\d+/);
        });

        it('should output only the version number', async () => {
            const result = await workspace.runCli(['--version']);

            const lines = result.stdoutAsString().trim().split('\n');
            expect(lines.length).toBe(1);
            expect(lines[0]).toMatch(/^\d+\.\d+\.\d+/);
        });

        it('should have no stderr output', async () => {
            const result = await workspace.runCli(['--version']);

            expect(result.stderrAsString()).toBe('');
        });
    });

    describe('-v shorthand', () => {
        it('should work with -v flag', async () => {
            const result = await workspace.runCli(['-v']);

            expect(result.stdoutAsString()).toMatch(/\d+\.\d+\.\d+/);
        });
    });

    describe('version consistency', () => {
        it('should return consistent version across calls', async () => {
            const result1 = await workspace.runCli(['--version']);
            const result2 = await workspace.runCli(['--version']);

            expect(result1.stdoutAsString().trim()).toBe(result2.stdoutAsString().trim());
        });

        it('should match package.json version', async () => {
            const result = await workspace.runCli(['--version']);
            const versionFromCli = result.stdoutAsString().trim();

            const pkg = require('../../package.json');
            expect(versionFromCli).toBe(pkg.version);
        });
    });
});
