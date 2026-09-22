import * as os from 'os';
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-list-ports');

interface PortRow {
    serviceName: string;
    pid: number;
    port: number;
    address: string;
    protocol: string;
    isChildProcess: boolean;
}

function parsePorts(stdout: string): PortRow[] {
    const parsed = JSON.parse(stdout);
    expect(Array.isArray(parsed.ports)).toBe(true);
    return parsed.ports;
}

describe('CLI list-ports / list-ports-all / open-browser', () => {
    beforeAll(async () => {
        await workspace.runCli(['start', 'web', 'idle']);
        await workspace.runCli(['wait-for-log', 'web', '--message', 'Now listening']);
    });

    afterAll(() => workspace.cleanup());

    describe('list-ports <name>', () => {
        it('lists only the named service', async () => {
            const result = await workspace.runCli(['list-ports', 'web']);
            const lines = result.stdoutAsString().split('\n');
            expect(lines[0]).toContain('SERVICE');
            expect(lines.some(line => line.startsWith('web '))).toBe(true);
        });

        it('leaves out services that were not named', async () => {
            const result = await workspace.runCli(['list-ports', 'idle']);
            expect(result.stdoutAsString()).not.toMatch(/^web /m);
            expect(result.stdoutAsString()).toContain('No open ports found');
        });

        it('errors on a service that is not configured', async () => {
            const result = await workspace.runCli(['list-ports', 'nope'], { ignoreExitCode: true });
            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain("No service 'nope' configured");
            expect(result.stdoutAsString()).not.toContain('SERVICE');
        });
    });

    describe('--json', () => {
        it('list-ports --json prints a ports array', async () => {
            const result = await workspace.runCli(['list-ports', '--json']);
            const ports = parsePorts(result.stdoutAsString());
            const web = ports.find(p => p.serviceName === 'web');
            expect(web).toBeDefined();
            expect(web!.port).toBeGreaterThan(0);
            expect(web!.protocol).toBe('TCP');
        });

        it('list-ports <name> --json respects the name filter', async () => {
            const result = await workspace.runCli(['list-ports', 'idle', '--json']);
            expect(parsePorts(result.stdoutAsString())).toEqual([]);
        });

        it('list-ports-all --json prints a ports array', async () => {
            const result = await workspace.runCli(['list-ports-all', '--json']);
            const ports = parsePorts(result.stdoutAsString());
            expect(ports.some(p => p.serviceName === 'web')).toBe(true);
        });
    });

    describe('list-ports-all outside a project', () => {
        it('works from a directory with no .candle.json', async () => {
            const result = await workspace.runCli(['list-ports-all'], { cwd: os.tmpdir() });
            expect(result.stdoutAsString()).toMatch(/^web /m);
        });

        it('works with --json from a directory with no .candle.json', async () => {
            const result = await workspace.runCli(['list-ports-all', '--json'], { cwd: os.tmpdir() });
            const ports = parsePorts(result.stdoutAsString());
            expect(ports.some(p => p.serviceName === 'web')).toBe(true);
        });
    });

    describe('open-browser', () => {
        it('errors on a service that is not configured', async () => {
            const result = await workspace.runCli(['open-browser', 'nope'], { ignoreExitCode: true });
            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain("No service 'nope' configured");
            expect(result.stderrAsString()).not.toContain('No open ports');
        });

        it('still reports "no open ports" for a configured service that is not listening', async () => {
            const result = await workspace.runCli(['open-browser', 'idle'], { ignoreExitCode: true });
            expect(result.failed()).toBe(true);
            expect(result.stderrAsString()).toContain("No open ports found for service 'idle'");
        });
    });
});
