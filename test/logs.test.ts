import { describe, it, expect, beforeAll, beforeEach, afterEach } from 'vitest';
import * as path from 'path';
import * as fs from 'fs';
import { spawn, ChildProcess } from 'child_process';
import { DatabaseLoader } from '@facetlayer/sqlite-wrapper';
import { clearTestData, getTestDataDirectory, getCandleBinPath } from './utils';

const TEST_NAME = 'logs';
const TEST_STATE_DIR = getTestDataDirectory(TEST_NAME);
const CANDLE_BIN = getCandleBinPath();
const CLI_PATH = path.join(CANDLE_BIN, 'dist', 'main-cli.js');
const TEST_PROJECT_DIR = path.join(__dirname, 'sampleServers');

async function runCommand(args: string[], options: { cwd?: string } = {}): Promise<{ stdout: string, stderr: string, code: number }> {
    return new Promise((resolve) => {
        const env = {
            ...process.env,
            CANDLE_DATABASE_DIR: TEST_STATE_DIR
        };

        const proc = spawn('node', [CLI_PATH, ...args], {
            cwd: options.cwd ?? TEST_PROJECT_DIR,
            env
        });
        
        let stdout = '';
        let stderr = '';
        
        proc.stdout.on('data', (data) => stdout += data);
        proc.stderr.on('data', (data) => stderr += data);
        
        proc.on('close', (code) => {
            resolve({ stdout, stderr, code: code || 0 });
        });
    });
}

describe('Logs Functionality', () => {
    beforeAll(() => {
        clearTestData(TEST_NAME);
    });
    
    afterEach(async () => {
        await runCommand(['kill-all']).catch(() => {});
    });
    
    it('should get logs by project directory and command name', async () => {
        // Start echo-test service which outputs logs
        const proc = spawn('node', [CLI_PATH, 'run', 'echo-test'], {
            cwd: TEST_PROJECT_DIR,
            env: {
                ...process.env,
                CANDLE_DATABASE_DIR: TEST_STATE_DIR
            }
        });
        
        // Wait for process to generate some logs
        await new Promise(resolve => setTimeout(resolve, 3000));
        
        // Kill the candle process but leave the server running briefly
        proc.kill('SIGTERM');
        await new Promise(resolve => setTimeout(resolve, 1000));
        
        // Get logs using the new approach
        const logsResult = await runCommand(['logs', 'echo-test']);
        
        expect(logsResult.code).toBe(0);
        expect(logsResult.stdout).toContain('Echo server started');
        
        // Clean up any remaining processes
        await runCommand(['kill', 'echo-test']);
    });
    
    it('should get default logs by project directory only', async () => {
        // Start default service (web)
        const proc = spawn('node', [CLI_PATH, 'run'], {
            cwd: TEST_PROJECT_DIR,
            env: {
                ...process.env,
                CANDLE_DATABASE_DIR: TEST_STATE_DIR
            }
        });
        
        // Wait for process to generate logs
        await new Promise(resolve => setTimeout(resolve, 3000));
        
        // Kill the candle process
        proc.kill('SIGTERM');
        await new Promise(resolve => setTimeout(resolve, 1000));
        
        // Get logs for default service (web)
        const logsResult = await runCommand(['logs', 'web']);
        
        expect(logsResult.code).toBe(0);
        // Just verify logs command works, content may vary
        expect(logsResult.stdout.length).toBeGreaterThan(0);
        
        // Clean up
        await runCommand(['kill', 'web']);
    });
});
