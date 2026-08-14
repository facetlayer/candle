import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from '../TestWorkspace';

const workspace = new TestWorkspace('list-format');

const SHELL = 'node ../../sampleServers/testProcess.js';

describe('List Format', () => {
    afterAll(() => workspace.cleanup());

    it('should show the multiline detail view for list', async () => {
        // Start a process using the test-format service
        await workspace.runCli(['start', 'test-format']);

        // Wait for the service to start up by waiting for the expected log message
        await workspace.runCli(['wait-for-log', 'test-format', '--message', 'Test server started']);

        const output = (await workspace.runCli(['list'])).stdoutAsString();
        const lines = output.split('\n');

        // Header line, then the two indented detail lines.
        expect(lines[0]).toMatch(/^test-format {2}RUNNING {2}pid \d+ {2}uptime \S/);
        expect(lines[1]).toBe(`  command:   ${SHELL}`);
        expect(lines[2]).toBe(`  directory: ${workspace.dbDir}`);

        // The table headers belong to 'candle ps' now.
        expect(output).not.toContain('NAME');
        expect(output).not.toContain('COMMAND');
        expect(output).not.toContain('DIRECTORY');
        expect(output).not.toContain('UPTIME');

        // Check that old headers are NOT present
        expect(output).not.toContain('LAUNCH_ID');
        expect(output).not.toContain('WRAPPER_PID');
    });

    it('should show the compact table for ps', async () => {
        await workspace.runCli(['start', 'test-format']);
        await workspace.runCli(['wait-for-log', 'test-format', '--message', 'Test server started']);

        const output = (await workspace.runCli(['ps'])).stdoutAsString();
        const headerLine = output.split('\n')[0];

        expect(headerLine.indexOf('NAME')).toBeLessThan(headerLine.indexOf('STATUS'));
        expect(headerLine.indexOf('STATUS')).toBeLessThan(headerLine.indexOf('PID'));
        expect(headerLine.indexOf('PID')).toBeLessThan(headerLine.indexOf('UPTIME'));

        // The two widest columns are dropped to save horizontal space.
        expect(output).not.toContain('COMMAND');
        expect(output).not.toContain('DIRECTORY');
        expect(output).not.toContain(SHELL);

        expect(output).toContain('test-format');
        expect(output).toMatch(/RUNNING|not running/);
    });
});
