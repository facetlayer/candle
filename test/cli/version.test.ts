import * as fs from 'fs';
import * as path from 'path';
import { describe, it, expect } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-version');

/**
 * The version is owned by the Rust build: Cargo injects `version` from the workspace
 * `rust/Cargo.toml` at compile time (CARGO_PKG_VERSION). Read it straight from that manifest
 * so this test tracks the single source of truth rather than any JS package metadata.
 */
function versionFromCargoToml(): string {
    const cargoToml = fs.readFileSync(
        path.join(__dirname, '..', '..', 'rust', 'Cargo.toml'),
        'utf8'
    );
    const match = cargoToml.match(/^\s*version\s*=\s*"([^"]+)"/m);
    if (!match) {
        throw new Error('Could not find workspace version in rust/Cargo.toml');
    }
    return match[1];
}

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

        it('should match the version in rust/Cargo.toml', async () => {
            const result = await workspace.runCli(['--version']);
            const versionFromCli = result.stdoutAsString().trim();

            expect(versionFromCli).toBe(versionFromCargoToml());
        });
    });

    describe('as an option value', () => {
        it('a --version given as a --message value is searched for, not handled', async () => {
            const result = await workspace.runCli(
                ['wait-for-log', 'nope', '--message', '--version', '--timeout', '1'],
                { ignoreExitCode: true },
            );

            expect(result.failed()).toBe(true);
            expect(result.stdoutAsString()).not.toBe(versionFromCargoToml() + '\n');
        });

        it('a --help given as a --message value does not print help', async () => {
            const result = await workspace.runCli(
                ['wait-for-log', 'nope', '--message', '--help', '--timeout', '1'],
                { ignoreExitCode: true },
            );

            expect(result.failed()).toBe(true);
            expect(result.stdoutAsString()).not.toContain('wait-for-log');
        });

        it('--version after a command still prints the version', async () => {
            const result = await workspace.runCli(['list', '--version']);

            expect(result.stdoutAsString().trim()).toBe(versionFromCargoToml());
        });
    });
});
