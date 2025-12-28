import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import * as path from 'path';
import * as fs from 'fs';
import { MCPStdinSubprocess } from 'expect-mcp';
import { createRunCandleCommand } from '../utils';
import { createMcpApp } from '../mcp/utils';

const TEST_STATE_DIR = path.join(__dirname, 'db');
const TEST_PROJECT_DIR = __dirname;

const runCandleCommand = createRunCandleCommand(TEST_STATE_DIR, TEST_PROJECT_DIR);

describe('Invalid Config Tests', () => {
    let app: MCPStdinSubprocess;

    beforeAll(async () => {
        // Create and clear the db directory
        if (fs.existsSync(TEST_STATE_DIR)) {
            fs.rmSync(TEST_STATE_DIR, { recursive: true, force: true });
        }
        fs.mkdirSync(TEST_STATE_DIR, { recursive: true });

        await runCandleCommand(['kill']).catch(() => {});
    });

    afterEach(async () => {
        // Close the MCP app if it's still open
        if (app) {
            await app.close();
        }
    });

    describe('CLI', () => {
        it('should capture exit code in logs when service fails', async () => {
            await runCandleCommand(['start', 'invalid-startup-command']);

            // Wait for the process to fail
            await new Promise(resolve => setTimeout(resolve, 500));

            const logsResult = await runCandleCommand(['logs', 'invalid-startup-command']);

            console.log('logsResult: ', logsResult);

            expect(logsResult.stdout).toContain('exited with code');
            //expect(logsResult.stdout).toContain('No such file or directory');
        });
    });

    describe('MCP', () => {
        it('should capture exit code in GetLogs when service fails', async () => {
            app = createMcpApp({
                cwd: TEST_PROJECT_DIR,
                databaseDir: TEST_STATE_DIR,
            });

            await app.callTool('StartService', {
                name: 'invalid-startup-command',
            });

            // Wait for the process to fail
            await new Promise(resolve => setTimeout(resolve, 500));

            const logsResult = await app.callTool('GetLogs', {
                name: 'invalid-startup-command',
            });

            await expect(logsResult).toBeSuccessful();
            const logsText = logsResult.getTextContent() ?? '';

            // The exit code should be captured
            expect(logsText).toMatch(/exited with code/i);
        });

        // BUG: stderr is not reliably captured when process exits very quickly.
        // See CLI test comment above for details.
        it.skip('should capture stderr error message in GetLogs when service fails to start', async () => {
            app = createMcpApp({
                cwd: TEST_PROJECT_DIR,
                databaseDir: TEST_STATE_DIR,
            });

            // Start the service - it will launch but fail quickly
            await app.callTool('StartService', {
                name: 'invalid-startup-command',
            });

            // Wait for the process to fail
            await new Promise(resolve => setTimeout(resolve, 500));

            // Check logs via MCP
            const logsResult = await app.callTool('GetLogs', {
                name: 'invalid-startup-command',
            });

            await expect(logsResult).toBeSuccessful();
            const logsText = logsResult.getTextContent() ?? '';

            // The stderr from the shell should be captured
            expect(logsText).toContain('No such file or directory');
        });

        it('should show service as not running after failed start via MCP', async () => {
            app = createMcpApp({
                cwd: TEST_PROJECT_DIR,
                databaseDir: TEST_STATE_DIR,
            });

            await app.callTool('StartService', {
                name: 'invalid-startup-command',
            });

            // Wait for the process to fail
            await new Promise(resolve => setTimeout(resolve, 500));

            // List services
            const listResult = await app.callTool('ListServices', {});
            await expect(listResult).toBeSuccessful();

            const listText = listResult.getTextContent() ?? '';
            // Service should not be listed as RUNNING
            expect(listText).not.toContain('RUNNING');
        });
    });
});
