import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from '../TestWorkspace';

const workspace = new TestWorkspace('list-format');

describe('List Format', () => {
    afterAll(() => workspace.cleanup());

    it('should show correct column headers and format', async () => {
        // Start a process using the test-format service
        await workspace.runCli(['start', 'test-format']);

        // Wait for the service to start up by waiting for the expected log message
        await workspace.runCli(['wait-for-log', 'test-format', '--message', 'Test server started']);

        // List processes
        const listResult = await workspace.runCli(['list']);

        // Check that the correct headers are present in the correct order
        expect(listResult.stdoutAsString()).toContain('NAME');
        expect(listResult.stdoutAsString()).toContain('COMMAND');
        expect(listResult.stdoutAsString()).toContain('DIRECTORY');
        expect(listResult.stdoutAsString()).toContain('UPTIME');
        expect(listResult.stdoutAsString()).toContain('PID');
        expect(listResult.stdoutAsString()).toContain('STATUS');

        // Check that the headers appear in the correct order
        const headerLine = listResult.stdoutAsString().split('\n')[0];
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
        expect(listResult.stdoutAsString()).not.toContain('LAUNCH_ID');
        expect(listResult.stdoutAsString()).not.toContain('WRAPPER_PID');

        // Check that the process data is shown
        expect(listResult.stdoutAsString()).toContain('test-format');
        // Process should be either RUNNING or STOPPED (might have exited quickly)
        expect(listResult.stdoutAsString()).toMatch(/RUNNING|STOPPED/);
        // Should contain either PID numbers for running processes or dashes for stopped ones
        expect(listResult.stdoutAsString()).toMatch(/\d+|-/); // Should contain PID numbers or dashes

        // Clean up is handled by afterAll
    });
});
