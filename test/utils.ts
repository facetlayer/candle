import * as path from 'path';
import * as fs from 'fs';
import { runShellCommand } from '@facetlayer/subprocess-wrapper';

export interface CommandResult {
    stdout: string;
    stderr: string;
    code: number;
}

export function getTestDataDirectory(testName: string) {
    return path.join(__dirname, 'tempdata', testName);
}

export function getTestTempDirectory(testName: string) {
    return path.join(__dirname, 'temp', testName);
}

export function getSampleServersDirectory() {
    return path.join(__dirname, 'sampleServers');
}

export function getCliPath() {
    return path.join(__dirname, '..', 'dist', 'main-cli.js');
}

export function getCandleBinPath() {
    return path.join(__dirname, '..');
}

export function createRunCandleCommand(testStateDir: string, defaultCwd?: string) {
    const cliPath = getCliPath();
    const cwd = defaultCwd ?? getSampleServersDirectory();

    return async function runCandleCommand(
        args: string[],
        options: { cwd?: string; env?: Record<string, string> } = {}
    ): Promise<CommandResult> {
        const env = {
            ...process.env,
            CANDLE_DATABASE_DIR: testStateDir,
            // Ensure consistent output width for snapshot testing
            FORCE_COLOR: '0',
            ...(options.env || {}),
        };

        const result = await runShellCommand('node', [cliPath, ...args], {
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

export function clearTestData(testName: string): string {
    const dir = getTestDataDirectory(testName);
    if (fs.existsSync(dir)) {
        fs.rmSync(dir, { recursive: true, force: true });
    }
    fs.mkdirSync(dir, { recursive: true });
    return dir;
}
