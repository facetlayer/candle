import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import * as fs from 'fs';
import { createRunCandleCommand, getTestTempDirectory } from './utils';

const TEST_STATE_DIR = getTestTempDirectory('list-db');
const runCandleCommand = createRunCandleCommand(TEST_STATE_DIR);

describe('List Command', () => {
  beforeAll(() => {
    if (fs.existsSync(TEST_STATE_DIR)) {
      fs.rmSync(TEST_STATE_DIR, { recursive: true, force: true });
    }
    fs.mkdirSync(TEST_STATE_DIR, { recursive: true });
  });

  afterEach(async () => {
    await runCandleCommand(['kill-all']).catch(() => {});
  });

  it('should show uptime in reasonable range for recently started service', async () => {
    await runCandleCommand(['start', 'echo']);
    await runCandleCommand(['wait-for-log', 'echo', '--message', 'Echo server started']);

    const result = await runCandleCommand(['list']);

    const lines = result.stdout.split('\n');
    const dataLine = lines.find(line => line.includes('echo'));

    expect(dataLine).toBeDefined();

    const uptimeMatch = dataLine!.match(/(\d+)m\s+(\d+)s|(\d+)s/);
    expect(uptimeMatch).toBeDefined();

    if (uptimeMatch![1]) {
      const minutes = parseInt(uptimeMatch![1], 10);
      expect(minutes).toBeLessThan(1);
    }
  });
});
