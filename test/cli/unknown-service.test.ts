import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

// Every command that takes a service name reports an unknown one the same way.
const workspace = new TestWorkspace('cli-unknown-service');

afterAll(() => workspace.cleanup());

const COMMANDS: string[][] = [
    ['list', 'nope'],
    ['ps', 'nope'],
    ['kill', 'nope'],
    ['restart', 'nope'],
    ['start', 'nope'],
    ['run', 'nope'],
    ['logs', 'nope'],
    ['wait-for-log', 'nope', '--message', 'x', '--timeout', '1'],
    ['list-ports', 'nope'],
    ['open-browser', 'nope'],
    ['clear-logs', 'nope'],
    ['watch', 'nope'],
];

describe('unknown service names', () => {
    for (const args of COMMANDS) {
        it(`${args[0]} prints the shared error and exits 1`, async () => {
            const result = await workspace.runCli(args, { ignoreExitCode: true });
            expect(result.exitCode).toBe(1);
            expect(result.stderrAsString().trim()).toBe(
                `Error: No service 'nope' configured for directory: ${workspace.dbDir}`,
            );
        });
    }

    it('list-all, being system-wide, names no directory', async () => {
        const result = await workspace.runCli(['list-all', 'nope'], { ignoreExitCode: true });
        expect(result.exitCode).toBe(1);
        expect(result.stderrAsString().trim()).toBe("Error: No running service named 'nope'");
    });
});
