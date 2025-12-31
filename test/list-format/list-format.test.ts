import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from '../TestWorkspace';

const workspace = new TestWorkspace('list-format');
const cli = workspace.createCli();

describe('List Format', () => {
    afterAll(() => workspace.cleanup());

    it('should show correct column headers and format', async () => {
        // Start a process using the test-format service
        await cli(['start', 'test-format']);

        // Wait for the service to start up by waiting for the expected log message
        await cli(['wait-for-log', 'test-format', '--message', 'Test server started successfully']);

        // List processes
        const listResult = await cli(['list']);

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

        // Clean up is handled by afterAll
    });
});
