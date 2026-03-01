import { describe, it, expect, afterAll, beforeAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-start');

describe('CLI Check-Start Command', () => {
    beforeAll(() => workspace.ensureSubdir('test'));
    afterAll(() => workspace.cleanup());

    it('should start a service when not running', async () => {
        // Make sure it's not running
        await workspace.runCli(['kill', 'echo'], { ignoreExitCode: true });

        const result = await workspace.runCli(['check-start', 'echo']);

        expect(result.stdoutAsString()).toContain('Started');
        expect(result.stdoutAsString()).toContain('echo');
    });

    it('should skip starting when service is already running', async () => {
        // Start the service first
        await workspace.runCli(['start', 'echo']);
        await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

        // Now check-start should be a no-op
        const result = await workspace.runCli(['check-start', 'echo']);

        expect(result.stdoutAsString()).toContain('already running');
        expect(result.stdoutAsString()).not.toContain('Started');
    });

    it('should work with multiple service names', async () => {
        // Start echo but not web
        await workspace.runCli(['kill'], { ignoreExitCode: true });
        await workspace.runCli(['start', 'echo']);
        await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

        // check-start both: echo should skip, web should start
        const result = await workspace.runCli(['check-start', 'echo', 'web']);

        const output = result.stdoutAsString();
        expect(output).toContain("already running");
        expect(output).toContain("Started");
        expect(output).toContain("web");
    });
});
