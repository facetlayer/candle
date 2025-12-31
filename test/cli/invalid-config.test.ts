import { describe, it, expect, afterAll, afterEach } from 'vitest';
import { MCPStdinSubprocess } from 'expect-mcp';
import { TestWorkspace } from '../TestWorkspace';
import { createMcpApp } from '../mcp/utils';

const workspace = new TestWorkspace('invalid-config');
const cli = workspace.createCli();

describe('Invalid Config Tests', () => {
    let app: MCPStdinSubprocess;

    afterAll(() => workspace.cleanup());

    afterEach(async () => {
        // Close the MCP app if it's still open
        if (app) {
            await app.close();
        }
    });

    describe('CLI', () => {
        it('should capture exit code in logs when service fails', async () => {
            await cli(['start', 'invalid-startup-command']);

            // Wait for the process to fail
            await new Promise(resolve => setTimeout(resolve, 500));

            const logsResult = await cli(['logs', 'invalid-startup-command']);

            expect(logsResult.stdout).toContain('exited with code');
            expect(logsResult.stdout).toContain('No such file or directory');
        });
    });

    describe('MCP', () => {
        it('should capture exit code in GetLogs when service fails', async () => {
            app = createMcpApp({
                cwd: workspace.dbDir,
                databaseDir: workspace.dbDir,
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
    });
});
