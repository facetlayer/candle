import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import * as path from 'path';
import { spawn, ChildProcess } from 'child_process';
import { clearTestData, getTestDataDirectory, getCandleBinPath } from './utils';

const TEST_NAME = 'wait-for-log';
const TEST_STATE_DIR = getTestDataDirectory(TEST_NAME);
const CANDLE_BIN = getCandleBinPath();
const CLI_PATH = path.join(CANDLE_BIN, 'dist', 'main-cli.js');
const TEST_DIR = path.join(__dirname, 'sampleServers');

async function runCommand(args: string[], options: { cwd?: string, timeout?: number } = {}): Promise<{ stdout: string, stderr: string, code: number }> {
    return new Promise((resolve) => {
        const env = {
            ...process.env,
            CANDLE_DATABASE_DIR: TEST_STATE_DIR
        };

        const proc = spawn('node', [CLI_PATH, ...args], {
            cwd: options.cwd ?? TEST_DIR,
            env
        });
        
        let stdout = '';
        let stderr = '';
        
        proc.stdout.on('data', (data) => stdout += data);
        proc.stderr.on('data', (data) => stderr += data);
        
        // Set up timeout if provided
        let timeoutId: NodeJS.Timeout | null = null;
        if (options.timeout) {
            timeoutId = setTimeout(() => {
                proc.kill('SIGKILL');
            }, options.timeout);
        }
        
        proc.on('close', (code) => {
            if (timeoutId) {
                clearTimeout(timeoutId);
            }
            resolve({ stdout, stderr, code: code || 0 });
        });
    });
}

async function startService(serviceName: string): Promise<void> {
    const result = await runCommand(['start', serviceName]);
    if (result.code !== 0) {
        throw new Error(`Failed to start service ${serviceName}: ${result.stderr}`);
    }
    
    // Wait a bit for the service to fully start
    await new Promise(resolve => setTimeout(resolve, 2000));
}

describe('Wait for Log Functionality', () => {
    beforeAll(() => {
        clearTestData(TEST_NAME);
    });
    
    afterEach(async () => {
        await runCommand(['kill-all']).catch(() => {});
    });
    
    it('should wait for a specific log message and exit successfully', async () => {
        // Start the delayed logger service
        await startService('delayed-logger');
        
        // Wait for a short time to ensure the service is running
        await new Promise(resolve => setTimeout(resolve, 500));
        
        // Wait for the "Server started" message
        const result = await runCommand([
            'wait-for-log', 
            'delayed-logger', 
            '--message', 
            'Server started',
            '--timeout',
            '10'
        ], { timeout: 15000 });
        
        expect(result.code).toBe(0);
        expect(result.stdout).toContain('Found message "Server started" in existing logs.');
    }, 20000);
    
    it('should timeout if message is not found within timeout period', async () => {
        // Start the delayed logger service
        await startService('delayed-logger');
        
        // Wait for a message that will never appear, with a short timeout
        const result = await runCommand([
            'wait-for-log', 
            'delayed-logger', 
            '--message', 
            'This message will never appear',
            '--timeout',
            '3'
        ], { timeout: 10000 });
        
        expect(result.code).toBe(1);
        expect(result.stderr).toContain('Timeout: Message "This message will never appear" not found within 3000ms');
    }, 15000);
    
    it('should detect message that already exists in logs', async () => {
        // Start the delayed logger service and wait for initial message
        await startService('delayed-logger');
        
        // Wait for the service to output the initial message
        await new Promise(resolve => setTimeout(resolve, 3000));
        
        // Now wait for a message that should already be in the logs
        const result = await runCommand([
            'wait-for-log', 
            'delayed-logger', 
            '--message', 
            'Server initializing',
            '--timeout',
            '5'
        ], { timeout: 10000 });
        
        expect(result.code).toBe(0);
        expect(result.stdout).toContain('Found message "Server initializing" in existing logs.');
    }, 15000);
    
    it('should fail if no process is found', async () => {
        // Try to wait for logs from a non-existent service
        const result = await runCommand([
            'wait-for-log', 
            'non-existent-service', 
            '--message', 
            'Any message',
            '--timeout',
            '5'
        ], { timeout: 10000 });
        
        expect(result.code).toBe(1);
        expect(result.stderr).toContain("No service 'non-existent-service' configured for directory");
    }, 10000);
    
    it('should wait for message that appears later', async () => {
        // Start the delayed logger service
        await startService('delayed-logger');
        
        // Wait for a short time to ensure the service is running
        await new Promise(resolve => setTimeout(resolve, 500));
        
        // Wait for the "Server ready to accept connections" message which appears after 4 seconds
        const result = await runCommand([
            'wait-for-log', 
            'delayed-logger', 
            '--message', 
            'ready to accept connections',
            '--timeout',
            '10'
        ], { timeout: 15000 });
        
        expect(result.code).toBe(0);
        expect(result.stdout).toContain('Found message "ready to accept connections" in logs.');
    }, 20000);
});
