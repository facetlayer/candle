import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import * as path from 'path';
import * as fs from 'fs';
import { spawn, ChildProcess } from 'child_process';
import { clearTestData, getTestDataDirectory, getCandleBinPath } from './utils';

const TEST_NAME = 'mcp';
const TEST_STATE_DIR = getTestDataDirectory(TEST_NAME);
const CANDLE_BIN = getCandleBinPath();
const TEST_DIR = path.join(__dirname, 'sampleServers');

async function runMcpCommand(input: any): Promise<{ stdout: string, stderr: string, code: number }> {
    return new Promise((resolve) => {
        const env = {
            ...process.env,
            CANDLE_DATABASE_DIR: TEST_STATE_DIR
        };
        
        const proc = spawn('node', [CANDLE_BIN, '--mcp'], {
            cwd: TEST_DIR,
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
            
            const proc = spawn('node', [CANDLE_BIN, 'kill-all'], {
                cwd: TEST_DIR,
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
    
    it('should throw proper error message when no service name provided and no default service', async () => {
        // Create a setup file with no default service
        const noDefaultDir = path.join(TEST_STATE_DIR, 'no-default');
        fs.mkdirSync(noDefaultDir, { recursive: true });
        
        const setupConfig = {
            services: [
                {
                    name: "test-service",
                    shell: "echo 'test'"
                    // No default: true
                }
            ]
        };
        
        fs.writeFileSync(
            path.join(noDefaultDir, '.candle-setup.json'), 
            JSON.stringify(setupConfig, null, 2)
        );
        
        const callToolRequest = {
            jsonrpc: "2.0",
            id: 1,
            method: "tools/call", 
            params: {
                name: "StartService",
                arguments: {}  // No name parameter - should use first service
            }
        };
        
        // Run MCP command in directory with no default service
        const result = await new Promise<{ stdout: string, stderr: string, code: number }>((resolve) => {
            const env = {
                ...process.env,
                CANDLE_DATABASE_DIR: TEST_STATE_DIR
            };
            
            const proc = spawn('node', [CANDLE_BIN, '--mcp'], {
                cwd: noDefaultDir,
                env
            });
            
            let stdout = '';
            let stderr = '';
            
            proc.stdout.on('data', (data) => stdout += data);
            proc.stderr.on('data', (data) => stderr += data);
            
            // Send MCP input
            proc.stdin.write(JSON.stringify(callToolRequest) + '\n');
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
        
        // Should return error when no service name and no default service
        expect(result.code).toBe(0); // MCP handles gracefully
        expect(result.stdout).toContain('error');
        expect(result.stdout).toContain('No services configured and no service name provided');
    });
});