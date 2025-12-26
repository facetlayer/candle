import { describe, it, expect, beforeAll } from 'vitest';
import * as fs from 'fs';
import { createRunCandleCommand, getTestTempDirectory } from './utils';

const TEST_STATE_DIR = getTestTempDirectory('help-db');
const runCandleCommand = createRunCandleCommand(TEST_STATE_DIR);

describe('Help and Commands', () => {
  beforeAll(() => {
    if (fs.existsSync(TEST_STATE_DIR)) {
      fs.rmSync(TEST_STATE_DIR, { recursive: true, force: true });
    }
    fs.mkdirSync(TEST_STATE_DIR, { recursive: true });
  });

  it('should recognize erase-database command', async () => {
    const result = await runCandleCommand(['erase-database']);

    expect(result.stderr).not.toContain('Unrecognized command');
    expect(result.code).toBe(0);
  });

  it('should show full help with commands when no args provided', async () => {
    const result = await runCandleCommand([]);

    const output = result.stdout + result.stderr;

    expect(output).toContain('Commands:');
    expect(output).toContain('run');
    expect(output).toContain('start');
    expect(output).toContain('kill');
  });
});
