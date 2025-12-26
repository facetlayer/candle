import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import * as fs from 'fs';
import { createRunCandleCommand, getTestTempDirectory } from './utils';

const TEST_STATE_DIR = getTestTempDirectory('restart-db');
const runCandleCommand = createRunCandleCommand(TEST_STATE_DIR);

describe('Restart Command', () => {
  beforeAll(() => {
    if (fs.existsSync(TEST_STATE_DIR)) {
      fs.rmSync(TEST_STATE_DIR, { recursive: true, force: true });
    }
    fs.mkdirSync(TEST_STATE_DIR, { recursive: true });
  });

  afterEach(async () => {
    await runCandleCommand(['kill-all']).catch(() => {});
  });

  it('should not print duplicate kill message when starting over an existing process', async () => {
    await runCandleCommand(['start', 'echo']);
    await runCandleCommand(['wait-for-log', 'echo', '--message', 'Echo server started']);

    const startResult = await runCandleCommand(['start', 'echo']);

    const startOutput = startResult.stdout + startResult.stderr;
    const killedInStart = (startOutput.match(/Killed/g) || []).length;

    expect(killedInStart).toBeLessThanOrEqual(1);
  });

  it('should exit after restart and not stay in watch mode', async () => {
    await runCandleCommand(['start', 'echo']);
    await runCandleCommand(['wait-for-log', 'echo', '--message', 'Echo server started']);

    const startTime = Date.now();
    const result = await runCandleCommand(['restart', 'echo']);
    const elapsed = Date.now() - startTime;

    expect(elapsed).toBeLessThan(10000);
    expect(result.code).toBe(0);

    const listResult = await runCandleCommand(['list']);
    expect(listResult.stdout).toContain('echo');
    expect(listResult.stdout).toContain('RUNNING');
  });
});
