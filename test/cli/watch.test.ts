import { describe, it, expect, afterAll } from 'vitest';
import { TestWorkspace } from './utils';

const workspace = new TestWorkspace('cli-watch');

afterAll(() => workspace.cleanup());

describe('CLI Watch Command', () => {
  it('prints the existing log output for the watched service', async () => {
    await workspace.runCli(['start', 'echo']);
    await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

    // Use the hidden --exit-after-ms flag so the watch command terminates on its own.
    const result = await workspace.runCli(['watch', 'echo', '--exit-after-ms', '2000']);
    const output = result.stdoutAsString();

    // The header should always be printed.
    expect(output).toContain("Watching process 'echo'");

    // Regression check: watch must actually print the captured log output, not just the
    // header. The recency-window filter previously discarded every log line, so watch
    // printed nothing.
    expect(output).toContain('Echo server started');
  });

  it('streams new log output produced while watching', async () => {
    await workspace.runCli(['start', 'echo']);
    await workspace.runCli(['wait-for-log', 'echo', '--message', 'Echo server started']);

    const result = await workspace.runCli(['watch', 'echo', '--exit-after-ms', '2500']);
    const output = result.stdoutAsString();

    // The echo server emits a counter line every second; watching for ~2.5s should
    // capture at least one of these live updates.
    expect(output).toMatch(/Echo \d+:/);
  });
});
