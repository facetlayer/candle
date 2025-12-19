import { describe, it, expect, beforeAll, beforeEach, afterEach } from 'vitest';
import * as path from 'path';
import { runShellCommand } from '@facetlayer/subprocess-wrapper';
import { getCandleBinPath } from '../utils';

const TEST_STATE_DIR = path.join(__dirname, 'db');
const CANDLE_BIN = getCandleBinPath();
const CLI_PATH = path.join(CANDLE_BIN, 'dist', 'main-cli.js');
const TEST_PROJECT_DIR = __dirname;

async function runCandleCommand(args: string[], options: { cwd?: string, env?: any } = {}): Promise<{ stdout: string, stderr: string, code: number }> {
    const env = {
        ...process.env,
        CANDLE_DATABASE_DIR: TEST_STATE_DIR,
        ...(options.env || {})
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

// For tests that need to start a service and wait for it
async function startServiceAndWait(serviceName: string, waitMessage: string = 'Test server started successfully'): Promise<void> {
    await runCandleCommand(['start', serviceName]);
    await runCandleCommand(['wait-for-log', serviceName, '--message', waitMessage]);
}

describe('Candle Functional Tests', () => {
    beforeAll(() => {
        // Create and clear the db directory
        const fs = require('fs');
        if (fs.existsSync(TEST_STATE_DIR)) {
            fs.rmSync(TEST_STATE_DIR, { recursive: true, force: true });
        }
        fs.mkdirSync(TEST_STATE_DIR, { recursive: true });
    });
    
    afterEach(async () => {
        // Kill all processes
        await runCandleCommand(['kill-all']).catch(() => {});
    });
    
    it('should show error when no setup file is found', async () => {
        // Test in a directory without a config file
        const result = await runCandleCommand(['run'], { cwd: '/tmp' });

        expect(result.code).not.toBe(0);
        expect(result.stderr).toContain('No .candle.json or .candle-setup.json file found');
    });
    
    it('should run default service from setup file', async () => {
        // Start the default service (web)
        await startServiceAndWait('web');

        // Verify process is running by listing
        const listResult = await runCandleCommand(['list']);
        expect(listResult.stdout).toContain('web');
        expect(listResult.code).toBe(0);

        // Kill via candle
        const killResult = await runCandleCommand(['kill', 'web']);
        expect(killResult.code).toBe(0);
    });
    
    it('should handle named services', async () => {
        // Start a named service
        await startServiceAndWait('web');

        // List processes
        const listResult = await runCandleCommand(['list']);
        expect(listResult.stdout).toContain('web');
        // Process should be either RUNNING or STOPPED (might have exited quickly)
        expect(listResult.stdout).toMatch(/RUNNING|STOPPED/);

        // Kill via candle
        const killResult = await runCandleCommand(['kill', 'web']);
        expect(killResult.code).toBe(0);
    });
    
    it('should show logs for a process', async () => {
        // Start a process that outputs logs
        await startServiceAndWait('echo', 'Echo server started');

        // Get logs
        const logsResult = await runCandleCommand(['logs', 'echo']);
        expect(logsResult.stdout).toContain('Echo server started');
    });
});
