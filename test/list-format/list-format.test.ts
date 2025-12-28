import { describe, it, expect, beforeAll, beforeEach, afterEach } from 'vitest';
import * as path from 'path';
import { runShellCommand } from '@facetlayer/subprocess-wrapper';
import { getCliPath } from '../utils';

const TEST_STATE_DIR = path.join(__dirname, 'db');
const CLI_PATH = getCliPath();
const TEST_PROJECT_DIR = __dirname;

async function runCandleCommand(args: string[], options: { cwd?: string } = {}): Promise<{ stdout: string, stderr: string, code: number }> {
    const env = {
        ...process.env,
        CANDLE_DATABASE_DIR: TEST_STATE_DIR
    };

    const result = await runShellCommand('node', [CLI_PATH, ...args], {
        cwd: options.cwd ?? TEST_PROJECT_DIR,
        env
    });

    return {
        stdout: result.stdoutAsString(),
        stderr: Array.isArray(result.stderr) ? result.stderr.join('\n') : (result.stderr || ''),
        code: result.exitCode || 0
    };
}

describe('List Format', () => {
    beforeAll(() => {
        // Create and clear the db directory
        const fs = require('fs');
        if (fs.existsSync(TEST_STATE_DIR)) {
            fs.rmSync(TEST_STATE_DIR, { recursive: true, force: true });
        }
        fs.mkdirSync(TEST_STATE_DIR, { recursive: true });
    });
    
    afterEach(async () => {
        await runCandleCommand(['kill-all']).catch(() => {});
    });
    
    it('should show correct column headers and format', async () => {
        // Start a process using the test-format service
        await runCandleCommand(['start', 'test-format']);

        // Wait for the service to start up by waiting for the expected log message
        await runCandleCommand(['wait-for-log', 'test-format', '--message', 'Test server started successfully']);

        // List processes
        const listResult = await runCandleCommand(['list']);

        expect(listResult.code).toBe(0);
        
        // Check that the correct headers are present in the correct order
        expect(listResult.stdout).toContain('NAME');
        expect(listResult.stdout).toContain('COMMAND');
        expect(listResult.stdout).toContain('DIRECTORY');
        expect(listResult.stdout).toContain('UPTIME');
        expect(listResult.stdout).toContain('PID');
        expect(listResult.stdout).toContain('STATUS');
        
        // Check that the headers appear in the correct order
        const headerLine = listResult.stdout.split('\n')[0];
        const nameIndex = headerLine.indexOf('NAME');
        const commandIndex = headerLine.indexOf('COMMAND');
        const directoryIndex = headerLine.indexOf('DIRECTORY');
        const uptimeIndex = headerLine.indexOf('UPTIME');
        const pidIndex = headerLine.indexOf('PID');
        const statusIndex = headerLine.indexOf('STATUS');
        
        expect(nameIndex).toBeLessThan(statusIndex);
        expect(statusIndex).toBeLessThan(pidIndex);
        expect(pidIndex).toBeLessThan(uptimeIndex);
        expect(uptimeIndex).toBeLessThan(commandIndex);
        expect(commandIndex).toBeLessThan(directoryIndex);
        
        // Check that old headers are NOT present
        expect(listResult.stdout).not.toContain('LAUNCH_ID');
        expect(listResult.stdout).not.toContain('WRAPPER_PID');
        
        // Check that the process data is shown
        expect(listResult.stdout).toContain('test-format');
        // Process should be either RUNNING or STOPPED (might have exited quickly)
        expect(listResult.stdout).toMatch(/RUNNING|STOPPED/);
        // Should contain either PID numbers for running processes or dashes for stopped ones
        expect(listResult.stdout).toMatch(/\d+|-/); // Should contain PID numbers or dashes
        
        // Clean up is handled by afterEach
    });
});
