import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import * as path from 'path';
import * as fs from 'fs';
import { mcpShell, MCPStdinSubprocess } from 'expect-mcp';
import { runShellCommand } from '@facetlayer/subprocess-wrapper';
import { getCandleBinPath } from '../utils';

const TEST_STATE_DIR = path.join(__dirname, 'db');
const CANDLE_BIN = getCandleBinPath();
const CLI_PATH = path.join(CANDLE_BIN, 'dist', 'main-cli.js');
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

describe('MCP Integration Tests', () => {
    let app: MCPStdinSubprocess;

    beforeAll(() => {
        // Create and clear the db directory
        if (fs.existsSync(TEST_STATE_DIR)) {
            fs.rmSync(TEST_STATE_DIR, { recursive: true, force: true });
        }
        fs.mkdirSync(TEST_STATE_DIR, { recursive: true });
    });

    afterEach(async () => {
        // Kill all services first before closing the MCP server
        await runCandleCommand(['kill-all']).catch(() => {});
        // Close the MCP app if it's still open
        if (app) {
            await app.close();
        }
    });

    it('can start and use a service (default service)', async () => {
        // Set up MCP subprocess
        app = mcpShell(`node ${CLI_PATH} --mcp`, {
            allowDebugLogging: true,
            cwd: TEST_PROJECT_DIR,
            env: {
                ...process.env,
                CANDLE_DATABASE_DIR: TEST_STATE_DIR
            }
        });

        // Verify the StartService tool exists
        await expect(app).toHaveTool('StartService');

        // Call StartService
        const result = await app.callTool('StartService', {});

        // Check that the call was successful
        await expect(result).toBeSuccessful();

        // Get the text content from the result
        const resultText = result.getTextContent();
        expect(resultText).toContain('Started');
        expect(resultText).toContain('node simpleServer.js');

        // Check ListServices
        const listServices = await app.callTool('ListServices', {});
        await expect(listServices).toBeSuccessful();
        const listResult = JSON.parse(listServices.getTextContent());
        expect(listResult.processes.length).toEqual(1);
        expect(listResult.processes[0].serviceName).toEqual('web');

        // Check logs
        const getLogs = await app.callTool('GetLogs', { name: 'web' });
        await expect(getLogs).toBeSuccessful();
        const logsText = getLogs.getTextContent();
        expect(logsText).toBeTruthy();

        // Kill the service
        const killService = await app.callTool('KillService', { name: 'web' });
        await expect(killService).toBeSuccessful();

        await app.close();
    });

    it('should handle StartService with invalid service name', async () => {
        // Set up MCP subprocess
        app = mcpShell(`node ${CLI_PATH} --mcp`, {
            cwd: TEST_PROJECT_DIR,
            env: {
                ...process.env,
                CANDLE_DATABASE_DIR: TEST_STATE_DIR
            }
        });

        await app.initialize();

        // Call StartService with invalid service name
        const result = await app.callTool('StartService', {
            name: 'nonexistent-service'
        });

        // Should return error in response
        expect(result.isError).toBe(true);
        const errorText = result.getTextContent();
        expect(errorText).toContain('nonexistent-service');

        await app.close();
    });

    it('should list all available tools', async () => {
        app = mcpShell(`node ${CLI_PATH} --mcp`, {
            cwd: TEST_PROJECT_DIR,
            env: {
                ...process.env,
                CANDLE_DATABASE_DIR: TEST_STATE_DIR
            }
        });

        // Verify all expected tools are available
        await expect(app).toHaveTools([
            'ListServices',
            'GetLogs',
            'StartService',
            'KillService',
            'RestartService',
            'AddServerConfig'
        ]);

        await app.close();
    });

    it('should restart a running service', async () => {
        app = mcpShell(`node ${CLI_PATH} --mcp`, {
            cwd: TEST_PROJECT_DIR,
            env: {
                ...process.env,
                CANDLE_DATABASE_DIR: TEST_STATE_DIR
            }
        });

        // Start a service first
        const startResult = await app.callTool('StartService', { name: 'web' });
        await expect(startResult).toBeSuccessful();

        // Wait a bit for the service to be running
        await new Promise(resolve => setTimeout(resolve, 500));

        // Restart the service
        const restartResult = await app.callTool('RestartService', { name: 'web' });
        await expect(restartResult).toBeSuccessful();

        // The restart operation itself succeeding is the main test
        // The service may or may not still be running depending on timing
        await expect(restartResult).toMatchTextContent(/Started|Restarted/);

        await app.close();
    });

    it('should support ListServices with showAll parameter', async () => {
        app = mcpShell(`node ${CLI_PATH} --mcp`, {
            cwd: TEST_PROJECT_DIR,
            env: {
                ...process.env,
                CANDLE_DATABASE_DIR: TEST_STATE_DIR
            }
        });

        // Start a service
        await app.callTool('StartService', {});

        // List with showAll: true
        const listResult = await app.callTool('ListServices', { showAll: true });
        await expect(listResult).toBeSuccessful();

        // List with showAll: false
        const listResult2 = await app.callTool('ListServices', { showAll: false });
        await expect(listResult2).toBeSuccessful();

        await app.close();
    });

    it('should get logs with custom limit', async () => {
        app = mcpShell(`node ${CLI_PATH} --mcp`, {
            cwd: TEST_PROJECT_DIR,
            env: {
                ...process.env,
                CANDLE_DATABASE_DIR: TEST_STATE_DIR
            }
        });

        // Start a service
        await app.callTool('StartService', {});

        // Wait for some logs to be generated
        await new Promise(resolve => setTimeout(resolve, 1000));

        // Get logs with a limit
        const logsResult = await app.callTool('GetLogs', { name: 'web', limit: 10 });
        await expect(logsResult).toBeSuccessful();

        await app.close();
    });

});
