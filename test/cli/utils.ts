import * as path from 'path';
import * as fs from 'fs';
import { runShellCommand } from '@facetlayer/subprocess-wrapper';

/**
 * CLI Test Utilities
 *
 * Shared utilities for CLI subprocess testing. These utilities help run
 * the candle CLI as a subprocess and capture/assert on its output.
 */

const CLI_PATH = path.join(__dirname, '..', '..', 'dist', 'main-cli.js');

export interface CommandResult {
    stdout: string;
    stderr: string;
    code: number;
}

/**
 * Get path to CLI test fixtures directory
 */
export function getCliFixturesDir(): string {
    return path.join(__dirname, 'fixtures');
}

/**
 * Get path to a specific fixture directory
 */
export function getFixtureDir(name: string): string {
    return path.join(getCliFixturesDir(), name);
}

/**
 * Get path to the isolated database directory for a test suite
 */
export function getTestDbDir(testName: string): string {
    return path.join(__dirname, 'temp', testName);
}

/**
 * Get path to the sample servers directory
 */
export function getSampleServersDir(): string {
    return path.join(__dirname, '..', 'sampleServers');
}

/**
 * Ensure a clean database directory exists for a test
 */
export function ensureCleanDbDir(testName: string): string {
    const dbDir = getTestDbDir(testName);
    if (fs.existsSync(dbDir)) {
        fs.rmSync(dbDir, { recursive: true, force: true });
    }
    fs.mkdirSync(dbDir, { recursive: true });
    return dbDir;
}

/**
 * Create a function to run candle commands in a specific test context
 */
export function createCli(
    dbDir: string,
    defaultCwd?: string
): (args: string[], options?: { cwd?: string; env?: Record<string, string> }) => Promise<CommandResult> {
    const cwd = defaultCwd ?? getSampleServersDir();

    return async function runCandle(
        args: string[],
        options: { cwd?: string; env?: Record<string, string> } = {}
    ): Promise<CommandResult> {
        const env = {
            ...process.env,
            CANDLE_DATABASE_DIR: dbDir,
            // Ensure consistent output width for snapshot testing
            FORCE_COLOR: '0',
            ...(options.env || {}),
        };

        const result = await runShellCommand('node', [CLI_PATH, ...args], {
            cwd: options.cwd ?? cwd,
            env,
        });

        return {
            stdout: result.stdoutAsString(),
            stderr: Array.isArray(result.stderr) ? result.stderr.join('\n') : result.stderr || '',
            code: result.exitCode || 0,
        };
    };
}

/**
 * Normalize output for snapshot testing
 * Removes dynamic content like timestamps, PIDs, paths, etc.
 */
export function normalizeOutput(output: string): string {
    return (
        output
            // Normalize line endings
            .replace(/\r\n/g, '\n')
            // Remove trailing whitespace from lines
            .split('\n')
            .map((line) => line.trimEnd())
            .join('\n')
            // Normalize uptime values (e.g., "0m 5s" -> "<uptime>")
            .replace(/\d+m\s+\d+s|\d+s/g, '<uptime>')
            // Normalize PIDs
            .replace(/PID:\s*\d+/g, 'PID: <pid>')
            .replace(/pid\s+\d+/gi, 'pid <pid>')
            // Normalize absolute paths to project-relative
            .replace(/\/Users\/[^\s]+\/candle\//g, '<project>/')
            .replace(/\/home\/[^\s]+\/candle\//g, '<project>/')
            .replace(/C:\\[^\s]+\\candle\\/g, '<project>/')
            // Normalize temp directory paths
            .replace(/\/tmp\/[^\s]+/g, '<tmpdir>')
            // Normalize database paths in output
            .replace(/CANDLE_DATABASE_DIR=[^\s]+/g, 'CANDLE_DATABASE_DIR=<dbdir>')
            // Trim leading/trailing whitespace
            .trim()
    );
}

/**
 * Create a temporary fixture directory with a .candle.json config
 */
export function createTempFixture(
    testName: string,
    config: { services: Array<{ name: string; shell: string; root?: string }> }
): string {
    const fixtureDir = path.join(__dirname, 'temp', 'fixtures', testName);
    if (fs.existsSync(fixtureDir)) {
        fs.rmSync(fixtureDir, { recursive: true, force: true });
    }
    fs.mkdirSync(fixtureDir, { recursive: true });

    const configPath = path.join(fixtureDir, '.candle.json');
    fs.writeFileSync(configPath, JSON.stringify(config, null, 2));

    return fixtureDir;
}

/**
 * Clean up a temporary fixture directory
 */
export function cleanupTempFixture(testName: string): void {
    const fixtureDir = path.join(__dirname, 'temp', 'fixtures', testName);
    if (fs.existsSync(fixtureDir)) {
        fs.rmSync(fixtureDir, { recursive: true, force: true });
    }
}

/**
 * Create sample process script in a fixture directory
 */
export function createSampleScript(fixtureDir: string, name: string, content: string): string {
    const scriptPath = path.join(fixtureDir, name);
    fs.writeFileSync(scriptPath, content);
    return scriptPath;
}
