import * as fs from 'fs';
import * as path from 'path';
import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-restart');

describe('CLI Restart Command', () => {
    afterAll(() => workspace.cleanup());

    describe('basic restart functionality', () => {
        it('should restart a running service', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['restart', 'echo']);

            expect(result.stdoutAsString()).toContain('Started');
        });

        it('should exit quickly after restart', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const startTime = Date.now();
            await workspace.runCli(['restart', 'echo']);
            const elapsed = Date.now() - startTime;

            expect(elapsed).toBeLessThan(5000);
        });

        it('should keep service running after restart', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            await workspace.runCli(['restart', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const list = await workspace.runCli(['list']);
            expect(list.stdoutAsString()).toContain('echo');
            expect(list.stdoutAsString()).toContain('RUNNING');
        });
    });

    describe('restart without running service', () => {
        it('should start the service when not already running', async () => {
            const result = await workspace.runCli(['restart', 'echo']);

            // CLI starts the service (not an error)
            expect(result.stdoutAsString()).toContain('Started');
        });
    });

    describe('restart transient processes', () => {
        it('should restart transient process with same shell', async () => {
            await workspace.runCli(['start', 'my-transient', '--shell', 'node ../../sampleServers/testProcess.js']);
            await workspace.runCli(['wait-for-log', 'my-transient', '--message', 'Test server started']);

            const result = await workspace.runCli(['restart', 'my-transient']);

            expect(result.stdoutAsString()).toContain('Started');

            // Verify it still uses same shell
            await workspace.runCli(['wait-for-log', 'my-transient', '--message', 'Test server started']);
        });
    });

    describe('restart preserves configuration', () => {
        it('should use stored shell command from DB', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Restart should use same command
            await workspace.runCli(['restart', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const logs = await workspace.runCli(['logs', 'echo']);
            expect(logs.stdoutAsString()).toContain('Echo server started');
        });
    });

    describe('restart output format', () => {
        it('should have clear success message', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['restart', 'echo']);

            expect(result.stdoutAsString()).toContain('Started');
        });

        it('should have minimal stderr on success', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['restart', 'echo']);

            // Some informational messages about cleanup are ok, but no actual errors
            expect(result.stderrAsString()).not.toContain('Error');
            expect(result.stderrAsString()).not.toContain('error:');
        });
    });

    describe('restart without name', () => {
        it('should restart all running processes when no name provided', async () => {
            // Start a service first
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Restart with no name restarts all running services
            const result = await workspace.runCli(['restart']);

            expect(result.stdoutAsString()).toContain('Started');
        });

        it('should error when no running processes to restart', async () => {
            // Make sure nothing is running
            await workspace.runCli(['kill-all']);

            const result = await workspace.runCli(['restart'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('No running processes');
        });
    });

    describe('restart unknown service', () => {
        it('should fail for unknown service', async () => {
            const result = await workspace.runCli(['restart', 'nonexistent-service'], { ignoreExitCode: true });

            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain('No service');
        });
    });

    describe('restart duplicate kill messages', () => {
        it('should not print duplicate kill messages when restarting a service', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const result = await workspace.runCli(['restart', 'echo']);

            const output = result.stdoutAsString() + result.stderrAsString();
            const killedCount = (output.match(/Killed/g) || []).length;

            expect(killedCount).toBeLessThanOrEqual(1);
        });

        it('should not print duplicate kill messages when restarting all services', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Restart without name restarts all running services
            const result = await workspace.runCli(['restart']);

            const output = result.stdoutAsString() + result.stderrAsString();
            const killedCount = (output.match(/Killed/g) || []).length;

            expect(killedCount).toBeLessThanOrEqual(1);
        });

        it('should not print duplicate kill messages even with multiple rapid restarts', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            // Rapid restarts might leave stale entries
            await workspace.runCli(['restart', 'echo']);
            const result = await workspace.runCli(['restart', 'echo']);

            const output = result.stdoutAsString() + result.stderrAsString();
            const killedCount = (output.match(/Killed/g) || []).length;

            expect(killedCount).toBeLessThanOrEqual(1);
        });
    });

    describe('restart reloads command from config', () => {
        // This suite edits its own .candle.json, so it uses a dedicated
        // workspace to avoid clobbering the shared cli-restart config.
        const reloadWorkspace = new TestWorkspace('cli-restart-reload');
        const configPath = path.join(reloadWorkspace.dbDir, '.candle.json');

        afterAll(() => reloadWorkspace.cleanup());

        function writeConfig(marker: string) {
            fs.writeFileSync(
                configPath,
                JSON.stringify({
                    services: [
                        {
                            name: 'marker',
                            shell: `node ../../sampleServers/markerServer.js ${marker} 600000`,
                        },
                    ],
                }, null, 2)
            );
        }

        it('should pick up an edited shell command on restart', async () => {
            writeConfig('marker-v1');
            await reloadWorkspace.runCli(['start', 'marker']);
            await reloadWorkspace.runCli(['wait-for-log', 'marker', '--message', 'MARKER=marker-v1']);

            // Edit the config to launch with a different command, then restart.
            // Before the fix, restart relaunched the originally-captured command
            // (marker-v1) and this new marker would never appear.
            writeConfig('marker-v2');
            await reloadWorkspace.runCli(['restart', 'marker']);

            await reloadWorkspace.runCli(['wait-for-log', 'marker', '--message', 'MARKER=marker-v2']);
        });
    });

    describe('start over existing process', () => {
        it('should not print duplicate kill message when starting over existing process', async () => {
            await workspace.runCli(['start', 'echo']);
            await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

            const startResult = await workspace.runCli(['start', 'echo']);

            const startOutput = startResult.stdoutAsString() + startResult.stderrAsString();
            const killedCount = (startOutput.match(/Killed/g) || []).length;

            expect(killedCount).toBeLessThanOrEqual(1);
        });
    });
});
