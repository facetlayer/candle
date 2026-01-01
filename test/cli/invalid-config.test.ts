import { describe, it, expect, afterAll, afterEach } from 'vitest';
import { MCPStdinSubprocess } from 'expect-mcp';
import { TestWorkspace } from '../TestWorkspace';

const workspace = new TestWorkspace('invalid-config');

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
        it('should show error details when service fails to start', async () => {
            // Start command fails because the service's startup command fails
            const result = await workspace.runCli(['start', 'invalid-startup-command'], { ignoreExitCode: true });

            // The CLI should exit with an error
            expect(result.failed()).toBe(true);

            // The stderr should contain the failure message and the actual error from the process
            const stderr = result.stderrAsString();
            expect(stderr).toContain("Process 'invalid-startup-command' failed to start");
            expect(stderr).toContain('Cannot find module');
            expect(stderr).toContain('nonexistent-script.js');
            expect(stderr).toContain('MODULE_NOT_FOUND');
        });

        it('should also capture error in logs', async () => {
            // Start command fails
            await workspace.runCli(['start', 'invalid-startup-command'], { ignoreExitCode: true });

            // Wait for logs to be captured
            await new Promise(resolve => setTimeout(resolve, 500));

            // The logs command should also show the error
            const logsResult = await workspace.runCli(['logs', 'invalid-startup-command']);
            const logsText = logsResult.stdoutAsString();

            expect(logsText).toContain('Cannot find module');
            expect(logsText).toContain('MODULE_NOT_FOUND');
        });
    });

    describe('MCP', () => {
        it('should return error with details when service fails to start', async () => {
            app = workspace.createMcpApp();

            const result = await app.callTool('StartService', {
                name: 'invalid-startup-command',
            });

            // The MCP call should return an error
            expect(result.isError).toBe(true);

            // The error should contain details about the failure
            const errorText = result.getTextContent() ?? '';
            expect(errorText).toContain('failed to start');
        });

        it('should capture error in GetLogs when service fails', async () => {
            app = workspace.createMcpApp();

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

            // The logs should contain the error details
            expect(logsText).toContain('Cannot find module');
            expect(logsText).toContain('MODULE_NOT_FOUND');
        });
    });
});
