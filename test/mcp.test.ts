import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import * as path from 'path';
import { spawn } from 'child_process';
import { clearTestData, getTestDataDirectory, getCandleBinPath } from './utils';

const TEST_NAME = 'mcp';
const TEST_STATE_DIR = getTestDataDirectory(TEST_NAME);
const CANDLE_BIN = getCandleBinPath();
const CLI_PATH = path.join(CANDLE_BIN, 'dist', 'main-cli.js');
const TEST_PROJECT_DIR = __dirname;

async function runMcpCommand(input: any, options: { cwd?: string } = {}): Promise<{ stdout: string, stderr: string, code: number }> {
    return new Promise((resolve) => {
        const env = {
            ...process.env,
            CANDLE_DATABASE_DIR: TEST_STATE_DIR
        };

        const proc = spawn('node', [CLI_PATH, '--mcp'], {
            cwd: options.cwd ?? TEST_PROJECT_DIR,
            env
        });
        
        let stdout = '';
        let stderr = '';
        
        proc.stdout.on('data', (data) => stdout += data);
        proc.stderr.on('data', (data) => stderr += data);
        
        // Send MCP input
        proc.stdin.write(JSON.stringify(input) + '\n');
        proc.stdin.end();
        
        proc.on('close', (code) => {
            resolve({ stdout, stderr, code: code || 0 });
        });
        
        // Timeout after 10 seconds
        setTimeout(() => {
            proc.kill('SIGTERM');
            resolve({ stdout, stderr, code: 1 });
        }, 10000);
    });
}

describe('MCP StartService Tests', () => {
    beforeAll(() => {
        clearTestData(TEST_NAME);
    });
    
    afterEach(async () => {
        // Kill all processes after each test
        await new Promise((resolve) => {
            const env = {
                ...process.env,
                CANDLE_DATABASE_DIR: TEST_STATE_DIR
            };
            
            const proc = spawn('node', [CLI_PATH, 'kill-all'], {
                cwd: TEST_PROJECT_DIR,
                env
            });
            
            proc.on('close', () => resolve(undefined));
            
            // Timeout
            setTimeout(() => {
                proc.kill('SIGTERM');
                resolve(undefined);
            }, 5000);
        });
    });
    
    it('should handle StartService with no parameters when default service exists', async () => {
        // Test calling StartService with no name parameter - should use default service
        const callToolRequest = {
            jsonrpc: "2.0",
            id: 1,
            method: "tools/call",
            params: {
                name: "StartService",
                arguments: {}  // No name parameter provided
            }
        };
        
        const result = await runMcpCommand(callToolRequest);
        
        // Debug output for failing tests
        if (result.code !== 0 || !result.stdout.includes('Started')) {
            console.log('Exit code:', result.code);
            console.log('Stdout:', result.stdout);
            console.log('Stderr:', result.stderr);
        }
        
        // Should not crash and should return success
        expect(result.code).toBe(0);
        // MCP response is now JSON format
        expect(result.stdout).toContain('Started');
        expect(result.stdout).toContain('node simpleServer.js');
    });
    
    it('should handle StartService with invalid service name', async () => {
        const callToolRequest = {
            jsonrpc: "2.0",
            id: 1, 
            method: "tools/call",
            params: {
                name: "StartService",
                arguments: {
                    name: "nonexistent-service"
                }
            }
        };
        
        const result = await runMcpCommand(callToolRequest);
        
        // Should return proper error, not crash
        expect(result.code).toBe(0); // MCP should handle the error gracefully  
        expect(result.stdout).toContain('error');
        expect(result.stdout).toContain('nonexistent-service');
    });
    
});
