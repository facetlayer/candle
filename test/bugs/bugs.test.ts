import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import * as path from 'path';
import * as fs from 'fs';
import { runShellCommand } from '@facetlayer/subprocess-wrapper';
import { getCandleBinPath } from '../utils';

const TEST_STATE_DIR = path.join(__dirname, 'db');
const CANDLE_BIN = getCandleBinPath();
const CLI_PATH = path.join(CANDLE_BIN, 'dist', 'main-cli.js');
const TEST_PROJECT_DIR = __dirname;

async function runCandleCommand(
  args: string[],
  options: { cwd?: string; env?: any } = {}
): Promise<{ stdout: string; stderr: string; code: number }> {
  const env = {
    ...process.env,
    CANDLE_DATABASE_DIR: TEST_STATE_DIR,
    ...(options.env || {}),
  };

  const result = await runShellCommand('node', [CLI_PATH, ...args], {
    cwd: options.cwd ?? TEST_PROJECT_DIR,
    env,
  });

  return {
    stdout: result.stdoutAsString(),
    stderr: Array.isArray(result.stderr) ? result.stderr.join('\n') : result.stderr || '',
    code: result.exitCode || 0,
  };
}

describe('Bug Fixes', () => {
  beforeAll(() => {
    // Create and clear the db directory
    if (fs.existsSync(TEST_STATE_DIR)) {
      fs.rmSync(TEST_STATE_DIR, { recursive: true, force: true });
    }
    fs.mkdirSync(TEST_STATE_DIR, { recursive: true });
  });

  afterEach(async () => {
    await runCandleCommand(['kill-all']).catch(() => {});
  });

  describe('Bug #1: add-service EISDIR error', () => {
    it('should add a service to an existing .candle-setup.json file', async () => {
      // Create a temporary directory with a config file
      const tempDir = path.join(TEST_STATE_DIR, 'add-service-test');
      if (fs.existsSync(tempDir)) {
        fs.rmSync(tempDir, { recursive: true, force: true });
      }
      fs.mkdirSync(tempDir, { recursive: true });

      // Create a minimal config file
      const configPath = path.join(tempDir, '.candle-setup.json');
      fs.writeFileSync(configPath, JSON.stringify({ services: [] }, null, 2));

      // Try to add a service
      const result = await runCandleCommand(['add-service', 'test-service', 'echo hello'], {
        cwd: tempDir,
      });

      // Should succeed (exit code 0)
      expect(result.code).toBe(0);
      expect(result.stdout).toContain("Service 'test-service' added successfully");

      // Verify the config file was updated
      const updatedConfig = JSON.parse(fs.readFileSync(configPath, 'utf8'));
      expect(updatedConfig.services).toHaveLength(1);
      expect(updatedConfig.services[0].name).toBe('test-service');
      expect(updatedConfig.services[0].shell).toBe('echo hello');
    });
  });

  describe('Bug #2: erase-database vs clear-database command mismatch', () => {
    it('should recognize erase-database command from help', async () => {
      // The help shows 'erase-database' but the switch handles 'clear-database'
      const result = await runCandleCommand(['erase-database']);

      // Should NOT say "Unrecognized command"
      expect(result.stderr).not.toContain('Unrecognized command');
      expect(result.code).toBe(0);
    });
  });

  describe('Bug #3: uptime calculation is incorrect', () => {
    it('should show uptime in reasonable range for recently started service', async () => {
      // Start a service and wait for it to be ready
      await runCandleCommand(['start', 'echo']);
      await runCandleCommand(['wait-for-log', 'echo', '--message', 'Echo server started']);

      // List and check uptime
      const result = await runCandleCommand(['list']);

      // Parse the uptime from output
      // Format: NAME  STATUS  PID  UPTIME  COMMAND  DIRECTORY
      const lines = result.stdout.split('\n');
      const dataLine = lines.find(line => line.includes('echo'));

      expect(dataLine).toBeDefined();

      // Extract the uptime value - it should be just seconds for a just-started process
      // Uptime appears after PID column, like "1s" or "5s"
      // For a just-started process (< 60 seconds), it should NOT show minutes
      const uptimeMatch = dataLine!.match(/(\d+)m\s+(\d+)s|(\d+)s/);
      expect(uptimeMatch).toBeDefined();

      if (uptimeMatch![1]) {
        // If there are minutes, the service was started more than 60 seconds ago
        // which should not happen in this test
        const minutes = parseInt(uptimeMatch![1], 10);
        expect(minutes).toBeLessThan(1); // Should be less than 1 minute
      }
      // If only seconds (uptimeMatch[3]), that's correct for a just-started process
    });
  });

  describe('Bug #4: duplicate kill messages during restart', () => {
    it('should not print duplicate kill message when starting over an existing process', async () => {
      // Start a service first
      await runCandleCommand(['start', 'echo']);
      await runCandleCommand(['wait-for-log', 'echo', '--message', 'Echo server started']);

      // Start again without killing first - handleRun will kill internally
      // The bug is that handleRun calls killExistingProcess which prints "Killed"
      // This shouldn't happen when called from restart (since restart already killed)
      // But even in start, it may print the message which is unexpected
      const startResult = await runCandleCommand(['start', 'echo']);

      // Count "Killed" messages - there should be exactly 1 (the internal kill)
      // or 0 if we decide start shouldn't kill running processes
      const startOutput = startResult.stdout + startResult.stderr;
      const killedInStart = (startOutput.match(/Killed/g) || []).length;

      // Currently the bug is: restart prints "Killed" twice
      // This test verifies that start only prints it once
      // When we fix restart, we'll need to ensure it doesn't double-print
      expect(killedInStart).toBeLessThanOrEqual(1);
    });
  });

  describe('Bug #5: empty args shows incomplete help', () => {
    it('should show full help with commands when no args provided', async () => {
      const result = await runCandleCommand([]);

      // Help might go to stdout or stderr depending on yargs config
      const output = result.stdout + result.stderr;

      // Should show the Commands section, not just Options
      expect(output).toContain('Commands:');
      expect(output).toContain('run');
      expect(output).toContain('start');
      expect(output).toContain('kill');
    });
  });

});
