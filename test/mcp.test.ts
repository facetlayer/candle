import { describe, it, expect, afterAll, afterEach } from 'vitest';
import { MCPStdinSubprocess } from 'expect-mcp';
import { TestWorkspace } from './TestWorkspace';

const workspace = new TestWorkspace('mcp');

describe('MCP Integration Tests', () => {
    let app: MCPStdinSubprocess;

    afterAll(() => workspace.cleanup());

    afterEach(async () => {
        // Close the MCP app if it's still open
        if (app) {
            await app.close();
        }
    });

    it('can start and use a service', async () => {
        // Set up MCP subprocess
        app = workspace.createMcpApp({ allowDebugLogging: true });

        // Verify the StartService tool exists
        await expect(app).toHaveTool('StartService');

        // Call StartService with a service name (required)
        const result = await app.callTool('StartService', { name: 'web' });

        // Check that the call was successful
        await expect(result).toBeSuccessful();

        // Get the text content from the result
        const resultText = result.getTextContent();
        expect(resultText).toContain('Started');
        expect(resultText).toContain('node ../../sampleServers/testProcess.js');

        // Check ListServices
        const listServices = await app.callTool('ListServices', {});
        await expect(listServices).toBeSuccessful();
        const listResult = JSON.parse(listServices.getTextContent() ?? '[]');

        expect(listResult.processes.length).toBeGreaterThanOrEqual(1);
        const webProcess = listResult.processes.find((p: any) => p.serviceName === 'web');
        expect(webProcess).toBeDefined();

        // Check logs - Use a retry loop in case the logs are not available yet.
        let logsText: string | undefined;
        for (let i = 0; i < 10; i++) {
            const getLogs = await app.callTool('GetLogs', { name: 'web' });
            await expect(getLogs).toBeSuccessful();
            logsText = getLogs.getTextContent();
            if (logsText) {
                break;
            }
            await new Promise(resolve => setTimeout(resolve, 500));
        }
        expect(logsText).toBeTruthy();

        // Kill the service
        const killService = await app.callTool('KillService', { name: 'web' });
        await expect(killService).toBeSuccessful();

        await app.close();
    });

    it('should error when StartService called without name', async () => {
        app = workspace.createMcpApp();

        // Call StartService without a name
        const result = await app.callTool('StartService', {});

        // Should return error
        expect(result.isError).toBe(true);
        const errorText = result.getTextContent();
        expect(errorText).toContain('name');

        await app.close();
    });

    it('should handle StartService with invalid service name', async () => {
        // Set up MCP subprocess
        app = workspace.createMcpApp();

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
        app = workspace.createMcpApp();

        // Verify all expected tools are available
        await expect(app).toHaveTools([
            'ListServices',
            'GetLogs',
            'StartService',
            'StartTransientService',
            'KillService',
            'RestartService',
            'AddServerConfig'
        ]);

        await app.close();
    });

    it('should start a transient service with StartTransientService', async () => {
        app = workspace.createMcpApp();

        // Start a transient service
        const result = await app.callTool('StartTransientService', {
            name: 'mcp-transient',
            shell: 'node ../../sampleServers/testProcess.js'
        });

        await expect(result).toBeSuccessful();
        const resultText = result.getTextContent();
        expect(resultText).toContain('Started');
        expect(resultText).toContain('mcp-transient');

        // Verify it appears in ListServices
        const listResult = await app.callTool('ListServices', {});
        await expect(listResult).toBeSuccessful();
        const listData = JSON.parse(listResult.getTextContent() ?? '{}');
        const transientProcess = listData.processes?.find((p: any) => p.serviceName === 'mcp-transient');
        expect(transientProcess).toBeDefined();
        expect(transientProcess.status).toBe('RUNNING');

        await app.close();
    });

    it('should require shell parameter for StartTransientService', async () => {
        app = workspace.createMcpApp();

        // Try to start without shell
        const result = await app.callTool('StartTransientService', {
            name: 'no-shell-transient'
        });

        expect(result.isError).toBe(true);
        const errorText = result.getTextContent();
        expect(errorText).toContain('shell');

        await app.close();
    });

    it('should start transient service with root parameter', async () => {
        app = workspace.createMcpApp();

        // Start with root parameter pointing to a subdirectory within the workspace
        // When root is 'subdir', shell command runs from there so we go up 3 levels to reach sampleServers
        const result = await app.callTool('StartTransientService', {
            name: 'rooted-mcp-transient',
            shell: 'node ../../../sampleServers/testProcess.js',
            root: 'subdir'
        });

        await expect(result).toBeSuccessful();
        const resultText = result.getTextContent();
        expect(resultText).toContain('Started');

        await app.close();
    });

    it('should restart a running service', async () => {
        app = workspace.createMcpApp();

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
        app = workspace.createMcpApp();

        // Start a service with explicit name
        await app.callTool('StartService', { name: 'web' });

        // List with showAll: true
        const listResult = await app.callTool('ListServices', { showAll: true });
        await expect(listResult).toBeSuccessful();

        // List with showAll: false
        const listResult2 = await app.callTool('ListServices', { showAll: false });
        await expect(listResult2).toBeSuccessful();

        await app.close();
    });

    it('should get logs with custom limit', async () => {
        app = workspace.createMcpApp();

        // Start a service with explicit name
        await app.callTool('StartService', { name: 'web' });

        // Wait for some logs to be generated
        await new Promise(resolve => setTimeout(resolve, 1000));

        // Get logs with a limit
        const logsResult = await app.callTool('GetLogs', { name: 'web', limit: 10 });
        await expect(logsResult).toBeSuccessful();

        await app.close();
    });

});
