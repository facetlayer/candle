import { describe, it, expect, beforeAll, beforeEach, afterEach } from 'vitest';
import * as path from 'path';
import * as fs from 'fs';
import { spawn } from 'child_process';
import { clearTestData, getTestDataDirectory, getCandleBinPath } from './utils';

const TEST_NAME = 'list-format';
const TEST_STATE_DIR = getTestDataDirectory(TEST_NAME);
const CANDLE_BIN = getCandleBinPath();
const TEST_DIR = path.join(__dirname, 'sampleServers');
const SIMPLE_SERVER = path.join(TEST_DIR, 'simpleServer.js');

async function runCommand(args: string[], options: { cwd?: string } = {}): Promise<{ stdout: string, stderr: string, code: number }> {
    return new Promise((resolve) => {
        const env = {
            ...process.env,
            CANDLE_DATABASE_DIR: TEST_STATE_DIR
        };
        
        const proc = spawn('node', [CANDLE_BIN, ...args], {
            cwd: options.cwd || TEST_DIR,
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

describe('List Format', () => {
    beforeAll(() => {
        clearTestData(TEST_NAME);
    });
    
    afterEach(async () => {
        await runCommand(['kill-all']).catch(() => {});
    });
    
    it('should show correct column headers and format', async () => {
        // Start a process using the test-format service
        const proc = spawn('node', [CANDLE_BIN, 'run', 'test-format'], {
            cwd: TEST_DIR,
            env: {
                ...process.env,
                CANDLE_DATABASE_DIR: TEST_STATE_DIR
            }
        });
        
        // Wait for process to start
        await new Promise(resolve => setTimeout(resolve, 2000));
        
        // List processes
        const listResult = await runCommand(['list']);
        
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
        
        // Kill the candle process
        proc.kill('SIGTERM');
        await new Promise(resolve => setTimeout(resolve, 1000));
        
        // Clean up
        await runCommand(['kill-all']);
    });
});
