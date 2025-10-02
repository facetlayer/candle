import 'expect-mcp/vitest-setup';
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

describe('MCP StartService Tests', () => {
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

        expect(result.logs).toBeDefined();
        expect(result.logs[0]).toContain('Started');
        expect(result.logs[0]).toContain('node simpleServer.js');

        // Check ListServices
        const listServices = await app.callTool('ListServices');
        expect(listServices.result.processes.length).toEqual(1);
        expect(listServices.result.processes[0].serviceName).toEqual('web');

        // Check logs
        const getLogs = await app.callTool('GetLogs', { serviceName: 'web' });
        expect(getLogs.logs.length).toBeGreaterThan(0);

        const killService = await app.callTool('KillService', { serviceName: 'web' });
        expect(killService.logs.length).toBeGreaterThan(0);

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
        expect(result.error).toBeDefined();
        expect(result.error.message).toContain('nonexistent-service');

        await app.close();
    });

});
